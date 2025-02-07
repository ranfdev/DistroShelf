use adw::prelude::*;

use crate::known_distros::{known_distro_by_image, KnownDistro};

pub fn setup(image_widget: &gtk::Image) {
    image_widget.set_icon_size(gtk::IconSize::Large);
    image_widget.add_css_class("distro-color-fg");
}

pub fn set_image(image_widget: &gtk::Image, container_image: &str) {
    let distro = known_distro_by_image(container_image);
    if let Some(distro) = &distro {
        let icon_theme = gtk::IconTheme::for_display(&image_widget.display());
        let icon = icon_theme.lookup_icon(
            &distro.icon_name(),
            &[&KnownDistro::default_icon_name()],
            32,
            1,
            gtk::TextDirection::None,
            gtk::IconLookupFlags::empty(),
        );

        image_widget.set_paintable(Some(&icon));
        image_widget.add_css_class(&distro.name());
    } else {
        image_widget.set_icon_name(Some(&KnownDistro::default_icon_name()));
    }
}
