use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::LazyLock,
};

use gtk::glib;
use tracing::{error, info};

use crate::distrobox::{wrap_capture_cmd, Command, CommandRunner, NullCommandRunner};

use gtk::subclass::prelude::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Terminal {
    pub name: String,
    pub program: String,
    pub separator_arg: String,
    pub read_only: bool,
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
    ]
    .iter()
    .map(|(name, program, separator_arg)| Terminal {
        name: name.to_string(),
        program: program.to_string(),
        separator_arg: separator_arg.to_string(),
        read_only: true,
    })
    .collect()
});

mod imp {
    use super::*;
    use std::{
        cell::{OnceCell, RefCell},
        rc::Rc,
    };

    pub struct TerminalRepository {
        pub list: RefCell<Vec<Terminal>>,
        pub custom_list_path: PathBuf,
        pub command_runner: OnceCell<Rc<dyn CommandRunner>>,
    }

    impl Default for TerminalRepository {
        fn default() -> Self {
            let custom_list_path = glib::user_data_dir().join("distroshelf-terminals.json");
            Self {
                list: RefCell::new(vec![]),
                custom_list_path,
                command_runner: OnceCell::new(),
            }
        }
    }
    impl ObjectImpl for TerminalRepository {}

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
    pub fn new(command_runner: Rc<dyn CommandRunner>) -> Self {
        let this: Self = glib::Object::builder().build();
        this.imp()
            .command_runner
            .set(command_runner)
            .map_err(|e| "command runner already set")
            .unwrap();

        let mut list = SUPPORTED_TERMINALS.clone();
        if let Ok(loaded_list) = Self::load_terminals_from_json(&this.imp().custom_list_path) {
            list.extend(loaded_list);
        } else {
            error!(
                "Failed to load custom terminals from JSON file {:?}",
                &this.imp().custom_list_path
            );
        }

        list.sort_by(|a, b| a.name.cmp(&b.name));
        this.imp().list.replace(list);
        this
    }

    pub fn is_read_only(&self, name: &str) -> bool {
        self.imp()
            .list
            .borrow()
            .iter()
            .find(|x| x.name == name)
            .map_or(false, |x| x.read_only)
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
            .find(|x| x.program == program)
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
        let json = serde_json::to_string(&*list).unwrap();
        std::fs::write(&self.imp().custom_list_path, json).unwrap();
    }

    fn load_terminals_from_json(path: &Path) -> anyhow::Result<Vec<Terminal>> {
        let data = std::fs::read_to_string(path)?;
        let list: Vec<Terminal> = serde_json::from_str(&data)?;
        Ok(list)
    }

    pub async fn default_terminal(&self) -> Option<Terminal> {
        let mut command = Command::new_with_args(
            "gsettings",
            &[
                "get",
                "org.gnome.desktop.default-applications.terminal",
                "exec",
            ],
        );
        wrap_capture_cmd(&mut command);
        let output = self
            .imp()
            .command_runner
            .get()
            .unwrap()
            .output(command.clone());
        let Ok(output) = output.await else {
            error!("Failed to get default terminal, running {:?}", &command);
            return None;
        };
        let terminal_program = String::from_utf8(output.stdout).unwrap().trim().to_string();
        let terminal_program = terminal_program.trim_matches('\'');
        if terminal_program.is_empty() {
            return None;
        }
        info!("Default terminal program: {}", terminal_program);
        self.terminal_by_program(&terminal_program).or_else(|| {
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
        Self::new(Rc::new(NullCommandRunner::default()))
    }
}
