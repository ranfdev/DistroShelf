use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use gtk::glib;
use gtk::prelude::*;
use tracing::{error, info, warn};

use crate::fakers::{Command, CommandRunner, FdMode};
use crate::query::Query;

use gtk::subclass::prelude::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Terminal {
    pub name: String,
    pub program: String,
    /// Arguments that come after the program but before the separator_arg.
    /// For example, for flatpak terminals: ["run", "org.gnome.Console"]
    #[serde(default)]
    pub extra_args: Vec<String>,
    pub separator_arg: String,
    pub read_only: bool,
}

impl Terminal {
    /// Returns a unique identifier for this terminal combining program and extra_args.
    /// This is used for deduplication since multiple terminals may use the same program
    /// (e.g., multiple flatpak terminals all use "flatpak" as the program).
    pub fn full_command_id(&self) -> String {
        if self.extra_args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.extra_args.join(" "))
        }
    }
}

static SUPPORTED_TERMINALS: LazyLock<Vec<Terminal>> = LazyLock::new(|| {
    [
        ("GNOME Console", "kgx", "--"),
        ("GNOME Terminal", "gnome-terminal", "--"),
        ("Konsole", "konsole", "-e"),
        ("Xfce Terminal", "xfce4-terminal", "-x"),
        ("Tilix", "tilix", "-e"),
        ("Kitty", "kitty", "--"),
        ("Alacritty", "alacritty", "-e"),
        ("WezTerm", "wezterm", "-e"),
        ("elementary Terminal", "io.elementary.terminal", "--"),
        ("Ptyxis", "ptyxis", "--"),
        ("Foot", "footclient", "-e"),
        ("Xterm", "xterm", "-e"),
        ("COSMIC Terminal", "cosmic-term", "-e"),
        ("Ghostty", "ghostty", "-e"),
        ("Terminator", "terminator", "-x"),
        ("QTerminal", "qterminal", "-e"),
        ("Deepin Terminal", "deepin-terminal", "-e"),
    ]
    .iter()
    .map(|(name, program, separator_arg)| Terminal {
        name: name.to_string(),
        program: program.to_string(),
        extra_args: vec![],
        separator_arg: separator_arg.to_string(),
        read_only: true,
    })
    .collect()
});

static FLATPAK_TERMINAL_CANDIDATES: LazyLock<Vec<Terminal>> = LazyLock::new(|| {
    let base_terminals = [
        ("Ptyxis", "app.devsuite.Ptyxis", "--"),
        ("GNOME Console", "org.gnome.Console", "--"),
        // ("BlackBox", "com.raggesilver.BlackBox", "--"), for some reason it doesn't work
        ("WezTerm", "org.wezfurlong.wezterm", "start --"),
        ("Foot", "page.codeberg.dnkl.foot", "-e"),
    ];

    let mut candidates = Vec::new();
    for (name, app_id, separator_arg) in base_terminals {
        // Stable
        candidates.push(Terminal {
            name: format!("{} (Flatpak)", name),
            program: "flatpak".to_string(),
            extra_args: vec!["run".to_string(), app_id.to_string()],
            separator_arg: separator_arg.to_string(),
            read_only: true,
        });
        // Devel
        candidates.push(Terminal {
            name: format!("{} Devel (Flatpak)", name),
            program: "flatpak".to_string(),
            extra_args: vec!["run".to_string(), format!("{}.Devel", app_id)],
            separator_arg: separator_arg.to_string(),
            read_only: true,
        });
    }
    candidates
});

mod imp {
    use super::*;
    use std::cell::{OnceCell, RefCell};
    use std::sync::OnceLock;

    pub struct TerminalRepository {
        pub list: RefCell<Vec<Terminal>>,
        pub custom_list_path: PathBuf,
        pub command_runner: OnceCell<CommandRunner>,
        pub json_terminals_query: Query<Vec<Terminal>>,
        pub flatpak_terminals_query: Query<Vec<Terminal>>,
    }

    impl Default for TerminalRepository {
        fn default() -> Self {
            let custom_list_path = glib::user_data_dir().join("distroshelf-terminals.json");
            Self {
                list: RefCell::new(vec![]),
                custom_list_path,
                command_runner: OnceCell::new(),
                json_terminals_query: Query::new("json_terminals".into(), || async { Ok(vec![]) }),
                flatpak_terminals_query: Query::new("flatpak_terminals".into(), || async {
                    Ok(vec![])
                }),
            }
        }
    }
    impl ObjectImpl for TerminalRepository {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS
                .get_or_init(|| vec![glib::subclass::Signal::builder("terminals-changed").build()])
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TerminalRepository {
        const NAME: &'static str = "TerminalRepository";
        type Type = super::TerminalRepository;
    }
}

glib::wrapper! {
    pub struct TerminalRepository(ObjectSubclass<imp::TerminalRepository>);
}

impl TerminalRepository {
    pub fn new(command_runner: CommandRunner) -> Self {
        let this: Self = glib::Object::builder().build();
        this.imp()
            .command_runner
            .set(command_runner.clone())
            .map_err(|_| "command runner already set")
            .unwrap();

        let mut list = SUPPORTED_TERMINALS.clone();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        this.imp().list.replace(list);

        // Set up the json terminals query fetcher
        let custom_list_path = this.imp().custom_list_path.clone();
        this.imp().json_terminals_query.set_fetcher(move || {
            let custom_list_path = custom_list_path.clone();
            async move {
                match Self::load_terminals_from_json(&custom_list_path) {
                    Ok(terminals) => Ok(terminals),
                    Err(e) if !custom_list_path.exists() => Ok(vec![]),
                    Err(e) => {
                        warn!(
                            "Failed to load custom terminals from JSON file {:?}: {}",
                            custom_list_path,
                            e
                        );
                        Err(e)
                    }
                }
            }
        });

        // Set up the flatpak terminals query fetcher
        let runner = command_runner.clone();
        this.imp().flatpak_terminals_query.set_fetcher(move || {
            let runner = runner.clone();
            async move { Self::fetch_flatpak_terminals(&runner).await }
        });

        let this_clone = this.clone();
        this.json_terminals_query().connect_success(move |terminals| {
            this_clone.apply_custom_terminals(terminals.clone());
            this_clone.emit_by_name::<()>("terminals-changed", &[]);
        });

        // Connect to query success to update the terminal list
        let this_clone = this.clone();
        this.flatpak_terminals_query()
            .connect_success(move |terminals| {
                this_clone.apply_flatpak_terminals(terminals.clone());
                this_clone.emit_by_name::<()>("terminals-changed", &[]);
            });

        this
    }

    async fn fetch_flatpak_terminals(runner: &CommandRunner) -> anyhow::Result<Vec<Terminal>> {
        // Get list of installed flatpaks
        let mut cmd = Command::new_with_args("flatpak", ["list", "--app", "--columns=application"]);
        cmd.stdout = FdMode::Pipe;
        cmd.stderr = FdMode::Pipe;

        let output = runner.output_string(cmd).await?;
        let installed_apps: HashSet<&str> = output.lines().collect();

        let mut found_terminals = Vec::new();
        for terminal in FLATPAK_TERMINAL_CANDIDATES.iter() {
            // Extract app_id from extra_args (e.g., ["run", "org.gnome.Console"])
            if let Some(app_id) = terminal.extra_args.get(1) {
                if installed_apps.contains(app_id.as_str()) {
                    found_terminals.push(terminal.clone());
                }
            }
        }

        Ok(found_terminals)
    }

    fn apply_custom_terminals(&self, terminals: Vec<Terminal>) {
        let mut list = self.imp().list.borrow_mut();
        list.retain(|terminal| terminal.read_only);
        list.extend(terminals.into_iter().map(|mut terminal| {
            terminal.read_only = false;
            terminal
        }));
        list.sort_by(|a, b| a.name.cmp(&b.name));
    }

    fn apply_flatpak_terminals(&self, terminals: Vec<Terminal>) {
        let mut list = self.imp().list.borrow_mut();

        let flatpak_candidate_ids: HashSet<String> = FLATPAK_TERMINAL_CANDIDATES
            .iter()
            .map(|terminal| terminal.full_command_id())
            .collect();
        list.retain(|terminal| !flatpak_candidate_ids.contains(&terminal.full_command_id()));

        let existing_ids: HashSet<String> = list.iter().map(|t| t.full_command_id()).collect();
        let new_terminals: Vec<Terminal> = terminals
            .into_iter()
            .filter(|t| !existing_ids.contains(&t.full_command_id()))
            .collect();

        list.extend(new_terminals);
        list.sort_by(|a, b| a.name.cmp(&b.name));
    }

    pub fn load_all(&self) {
        self.json_terminals_query().refetch();
        self.flatpak_terminals_query().refetch();
    }

    pub fn json_terminals_query(&self) -> Query<Vec<Terminal>> {
        self.imp().json_terminals_query.clone()
    }

    pub fn flatpak_terminals_query(&self) -> Query<Vec<Terminal>> {
        self.imp().flatpak_terminals_query.clone()
    }

    pub fn is_read_only(&self, name: &str) -> bool {
        self.imp()
            .list
            .borrow()
            .iter()
            .find(|x| x.name == name)
            .is_some_and(|x| x.read_only)
    }

    pub fn save_terminal(&self, terminal: Terminal) -> anyhow::Result<()> {
        if self.is_read_only(terminal.name.as_str()) {
            return Err(anyhow::anyhow!("Cannot modify read-only terminal"));
        }
        {
            let mut list = self.imp().list.borrow_mut();
            list.retain(|x| x.name != terminal.name);
            list.push(terminal);

            list.sort_by(|a, b| a.name.cmp(&b.name));
        }

        self.save_terminals_to_json();
        Ok(())
    }

    pub fn delete_terminal(&self, name: &str) -> anyhow::Result<()> {
        if self.is_read_only(name) {
            return Err(anyhow::anyhow!("Cannot modify read-only terminal"));
        }
        {
            self.imp().list.borrow_mut().retain(|x| x.name != name);
        }
        self.save_terminals_to_json();
        Ok(())
    }

    pub fn terminal_by_name(&self, name: &str) -> Option<Terminal> {
        self.imp()
            .list
            .borrow()
            .iter()
            .find(|x| x.name == name)
            .cloned()
    }

    pub fn terminal_by_program(&self, program: &str) -> Option<Terminal> {
        self.imp()
            .list
            .borrow()
            .iter()
            .find(|x| x.program == program || x.full_command_id() == program)
            .cloned()
    }

    pub fn all_terminals(&self) -> Vec<Terminal> {
        self.imp().list.borrow().clone()
    }

    fn save_terminals_to_json(&self) {
        let list: Vec<Terminal> = self
            .imp()
            .list
            .borrow()
            .iter()
            .filter(|x| !x.read_only)
            .cloned()
            .collect::<Vec<_>>();

        match serde_json::to_string(&list) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.imp().custom_list_path, json) {
                    error!(
                        "Failed to write custom terminals to {:?}: {}",
                        &self.imp().custom_list_path,
                        e
                    );
                }
            }
            Err(e) => {
                error!("Failed to serialize custom terminals: {}", e);
            }
        }
    }

    fn load_terminals_from_json(path: &Path) -> anyhow::Result<Vec<Terminal>> {
        let data = std::fs::read_to_string(path)?;
        let list: Vec<Terminal> = serde_json::from_str(&data)?;
        Ok(list)
    }

    pub async fn default_terminal(&self) -> Option<Terminal> {
        let mut command = Command::new_with_args(
            "gsettings",
            [
                "get",
                "org.gnome.desktop.default-applications.terminal",
                "exec",
            ],
        );
        command.stdout = FdMode::Pipe;
        command.stderr = FdMode::Pipe;

        let Some(runner) = self.imp().command_runner.get() else {
            error!("Command runner not initialized");
            return None;
        };

        let Ok(output) = runner.output(command.clone()).await else {
            error!("Failed to get default terminal, running {:?}", &command);
            return None;
        };

        let Ok(terminal_program) = String::from_utf8(output.stdout) else {
            error!("Default terminal output is not valid UTF-8");
            return None;
        };

        let terminal_program = terminal_program.trim().trim_matches('\'');
        if terminal_program.is_empty() {
            return None;
        }
        info!("Default terminal program: {}", terminal_program);
        self.terminal_by_program(terminal_program).or_else(|| {
            error!(
                "Terminal program {} not found in the list",
                terminal_program
            );
            None
        })
    }
}

impl Default for TerminalRepository {
    fn default() -> Self {
        Self::new(CommandRunner::default())
    }
}
