use gtk::glib;

use super::Container;

/// Enum representing the different dialog types that can be shown in the application.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "DialogType")]
pub enum DialogType {
    #[default]
    None,
    ExportableApps,
    CreateDistrobox,
    TaskManager,
    Preferences,
    CommandLog,
}

/// Parameters that can be passed when opening a dialog.
#[derive(Debug, Default, Clone)]
pub struct DialogParams {
    /// Container to clone from (used by CreateDistrobox dialog)
    pub clone_source: Option<Container>,
}

impl DialogParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_clone_source(mut self, container: Container) -> Self {
        self.clone_source = Some(container);
        self
    }
}
