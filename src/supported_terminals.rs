use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
};

use gtk::glib;
use tracing::{error, info, warn};

use crate::fakers::{CommandRunner, Command, FdMode};

use gtk::subclass::prelude::*;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Terminal {
    pub name: String,
    pub program: String,
    pub separator_arg: String,
    pub read_only: bool,
}

// Mapping of flatpak app IDs to terminal metadata
static FLATPAK_TERMINAL_MAPPINGS: LazyLock<Vec<(&'static str, &'static str, &'static str)>> = LazyLock::new(|| {
    vec![
        ("org.gnome.Console", "GNOME Console", "--"),
        ("org.gnome.Console.Devel", "GNOME Console", "--"),
        ("org.gnome.Terminal", "GNOME Terminal", "--"),
        ("org.kde.konsole", "Konsole", "-e"),
        ("org.xfce.Terminal", "Xfce Terminal", "-x"),
        ("com.gexperts.Tilix", "Tilix", "-e"),
        ("io.github.kovidgoyal.kitty", "Kitty", "--"),
        ("io.alacritty.Alacritty", "Alacritty", "-e"),
        ("org.wezfurlong.wezterm", "WezTerm", "-e"),
        ("io.elementary.terminal", "elementary Terminal", "--"),
        ("app.devsuite.Ptyxis", "Ptyxis", "--"),
        ("app.devsuite.Ptyxis.Devel", "Ptyxis", "--"),
        ("org.codeberg.dnkl.foot", "Foot", "-e"),
        ("com.system76.CosmicTerm", "COSMIC Terminal", "-e"),
        ("com.mitchellh.ghostty", "Ghostty", "-e"),
        ("com.gexperts.Terminator", "Terminator", "-x"),
        ("org.lxqt.QTerminal", "QTerminal", "-e"),
    ]
});

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
        separator_arg: separator_arg.to_string(),
        read_only: true,
    })
    .collect()
});

mod imp {
    use super::*;
    use std::{
        cell::{OnceCell, RefCell},
    };

    pub struct TerminalRepository {
        pub list: RefCell<Vec<Terminal>>,
        pub custom_list_path: PathBuf,
        pub command_runner: OnceCell<CommandRunner>,
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
    pub fn new(command_runner: CommandRunner) -> Self {
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
            warn!(
                "Failed to load custom terminals from JSON file {:?}",
                &this.imp().custom_list_path
            );
        }

        list.sort_by(|a, b| a.name.cmp(&b.name));
        this.imp().list.replace(list);
        this
    }

    /// Discover flatpak terminals installed on the system
    async fn discover_flatpak_terminals(&self) -> Vec<Terminal> {
        let mut flatpak_terminals = Vec::new();

        // Get list of installed flatpak applications
        let mut command = Command::new_with_args(
            "flatpak",
            &["list", "--app", "--columns=application"],
        );
        command.stdout = FdMode::Pipe;
        command.stderr = FdMode::Pipe;

        let output = match self
            .imp()
            .command_runner
            .get()
            .unwrap()
            .output(command.clone())
            .await
        {
            Ok(output) => output,
            Err(e) => {
                info!("Failed to run flatpak list command: {}", e);
                return flatpak_terminals;
            }
        };

        if !output.status.success() {
            info!("flatpak list command failed");
            return flatpak_terminals;
        }

        let installed_apps = String::from_utf8_lossy(&output.stdout);
        
        // Check each installed app against our known terminal mappings
        for (app_id, base_name, separator_arg) in FLATPAK_TERMINAL_MAPPINGS.iter() {
            if installed_apps.lines().any(|line| line.trim() == *app_id) {
                // Get the command that the flatpak executes
                if let Some(command_name) = self.get_flatpak_command(app_id).await {
                    // Determine variant suffix
                    let variant = if app_id.ends_with(".Devel") {
                        " (Flatpak, Devel)"
                    } else {
                        " (Flatpak)"
                    };
                    
                    let terminal = Terminal {
                        name: format!("{}{}", base_name, variant),
                        program: format!("flatpak run {}", app_id),
                        separator_arg: separator_arg.to_string(),
                        read_only: true,
                    };
                    flatpak_terminals.push(terminal);
                    info!("Discovered flatpak terminal: {} -> {}", app_id, command_name);
                }
            }
        }

        flatpak_terminals
    }

    /// Get the command that a flatpak application executes
    async fn get_flatpak_command(&self, app_id: &str) -> Option<String> {
        let mut command = Command::new_with_args("flatpak", &["info", app_id]);
        command.stdout = FdMode::Pipe;
        command.stderr = FdMode::Pipe;

        let output = self
            .imp()
            .command_runner
            .get()
            .unwrap()
            .output(command)
            .await
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let info = String::from_utf8_lossy(&output.stdout);
        
        // Look for the "Command:" line in the flatpak info output
        for line in info.lines() {
            if line.trim().starts_with("Command:") {
                let command = line.split(':').nth(1)?.trim();
                // Extract just the binary name from the path (e.g., /app/bin/ptyxis -> ptyxis)
                return command.split('/').last().map(|s| s.to_string());
            }
        }

        None
    }

    /// Reload terminals including flatpak discoveries
    pub async fn reload_with_flatpak_discovery(&self) {
        let mut list = SUPPORTED_TERMINALS.clone();
        
        // Add flatpak terminals
        let flatpak_terminals = self.discover_flatpak_terminals().await;
        list.extend(flatpak_terminals);
        
        // Add custom terminals
        if let Ok(loaded_list) = Self::load_terminals_from_json(&self.imp().custom_list_path) {
            list.extend(loaded_list);
        }

        list.sort_by(|a, b| a.name.cmp(&b.name));
        self.imp().list.replace(list);
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
        command.stdout = FdMode::Pipe;
        command.stderr = FdMode::Pipe;

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
        Self::new(CommandRunner::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakers::NullCommandRunnerBuilder;

    #[test]
    fn test_discover_flatpak_terminals() {
        smol::block_on(async {
            let flatpak_list_output = "org.gnome.Console\napp.devsuite.Ptyxis\napp.devsuite.Ptyxis.Devel\n";
            let console_info = "Ref: app/org.gnome.Console/x86_64/stable\nID: org.gnome.Console\nCommand: /app/bin/kgx\n";
            let ptyxis_info = "Ref: app/app.devsuite.Ptyxis/x86_64/stable\nID: app.devsuite.Ptyxis\nCommand: /app/bin/ptyxis\n";
            let ptyxis_devel_info = "Ref: app/app.devsuite.Ptyxis.Devel/x86_64/stable\nID: app.devsuite.Ptyxis.Devel\nCommand: /app/bin/ptyxis\n";

            let runner = NullCommandRunnerBuilder::new()
                .cmd(&["flatpak", "list", "--app", "--columns=application"], flatpak_list_output)
                .cmd(&["flatpak", "info", "org.gnome.Console"], console_info)
                .cmd(&["flatpak", "info", "app.devsuite.Ptyxis"], ptyxis_info)
                .cmd(&["flatpak", "info", "app.devsuite.Ptyxis.Devel"], ptyxis_devel_info)
                .build();

            let repo = TerminalRepository::new(runner);
            let flatpak_terminals = repo.discover_flatpak_terminals().await;

            // Should discover 3 terminals
            assert_eq!(flatpak_terminals.len(), 3);

            // Check that GNOME Console was discovered
            let console = flatpak_terminals.iter().find(|t| t.name.contains("GNOME Console"));
            assert!(console.is_some());
            let console = console.unwrap();
            assert_eq!(console.name, "GNOME Console (Flatpak)");
            assert_eq!(console.program, "flatpak run org.gnome.Console");
            assert_eq!(console.separator_arg, "--");
            assert!(console.read_only);

            // Check that Ptyxis was discovered
            let ptyxis = flatpak_terminals.iter().find(|t| t.name == "Ptyxis (Flatpak)");
            assert!(ptyxis.is_some());
            let ptyxis = ptyxis.unwrap();
            assert_eq!(ptyxis.program, "flatpak run app.devsuite.Ptyxis");

            // Check that Ptyxis Devel was discovered with proper variant
            let ptyxis_devel = flatpak_terminals.iter().find(|t| t.name == "Ptyxis (Flatpak, Devel)");
            assert!(ptyxis_devel.is_some());
            let ptyxis_devel = ptyxis_devel.unwrap();
            assert_eq!(ptyxis_devel.program, "flatpak run app.devsuite.Ptyxis.Devel");
        });
    }

    #[test]
    fn test_discover_no_flatpak_terminals() {
        smol::block_on(async {
            let flatpak_list_output = "";

            let runner = NullCommandRunnerBuilder::new()
                .cmd(&["flatpak", "list", "--app", "--columns=application"], flatpak_list_output)
                .build();

            let repo = TerminalRepository::new(runner);
            let flatpak_terminals = repo.discover_flatpak_terminals().await;

            assert_eq!(flatpak_terminals.len(), 0);
        });
    }

    #[test]
    fn test_get_flatpak_command() {
        smol::block_on(async {
            let info_output = "Ref: app/org.gnome.Console/x86_64/stable\nID: org.gnome.Console\nCommand: /app/bin/kgx\n";

            let runner = NullCommandRunnerBuilder::new()
                .cmd(&["flatpak", "info", "org.gnome.Console"], info_output)
                .build();

            let repo = TerminalRepository::new(runner);
            let command = repo.get_flatpak_command("org.gnome.Console").await;

            assert_eq!(command, Some("kgx".to_string()));
        });
    }

    #[test]
    fn test_reload_with_flatpak_discovery() {
        smol::block_on(async {
            let flatpak_list_output = "app.devsuite.Ptyxis\n";
            let ptyxis_info = "Ref: app/app.devsuite.Ptyxis/x86_64/stable\nID: app.devsuite.Ptyxis\nCommand: /app/bin/ptyxis\n";

            let runner = NullCommandRunnerBuilder::new()
                .cmd(&["flatpak", "list", "--app", "--columns=application"], flatpak_list_output)
                .cmd(&["flatpak", "info", "app.devsuite.Ptyxis"], ptyxis_info)
                .build();

            let repo = TerminalRepository::new(runner);
            repo.reload_with_flatpak_discovery().await;

            let all_terminals = repo.all_terminals();
            
            // Should have both system terminals and the discovered flatpak terminal
            assert!(all_terminals.len() > SUPPORTED_TERMINALS.len());
            
            // Check that the flatpak variant is present
            let ptyxis_flatpak = all_terminals.iter().find(|t| t.name == "Ptyxis (Flatpak)");
            assert!(ptyxis_flatpak.is_some());
            
            // Check that the system Ptyxis is also still present
            let ptyxis_system = all_terminals.iter().find(|t| t.name == "Ptyxis");
            assert!(ptyxis_system.is_some());
        });
    }
}

