use std::sync::LazyLock;

#[derive(Clone, Debug)]
pub struct SupportedTerminal {
    pub name: String,
    pub program: String,
    pub separator_arg: String,
}

pub static SUPPORTED_TERMINALS: LazyLock<Vec<SupportedTerminal>> = LazyLock::new(|| {
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
    ]
    .iter()
    .map(|(name, program, separator_arg)| SupportedTerminal {
        name: name.to_string(),
        program: program.to_string(),
        separator_arg: separator_arg.to_string(),
    })
    .collect()
});

pub fn terminal_by_name(name: &str) -> Option<SupportedTerminal> {
    SUPPORTED_TERMINALS.iter().find(|x| x.name == name).cloned()
}
