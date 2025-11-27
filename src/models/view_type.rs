use gtk::glib;

/// Enum representing the different main views in the application.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "ViewType")]
pub enum ViewType {
    #[default]
    Main,
    /// The welcome/setup view shown when distrobox is not available
    Welcome,
}

impl ViewType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ViewType::Main => "main",
            ViewType::Welcome => "welcome",
        }
    }
}
