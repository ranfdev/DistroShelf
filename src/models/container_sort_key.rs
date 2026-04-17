use gtk::glib;

/// Sort key for container list models.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "ContainerSortKey")]
pub enum ContainerSortKey {
    #[default]
    Name,
    CreationDate,
    LastUsedDate,
}
