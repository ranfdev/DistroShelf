use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::Properties;
use gtk::glib;
use std::cell::RefCell;

use crate::models::{known_distro_by_image, KnownDistro};

mod imp {
    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::DistroIcon)]
    pub struct DistroIcon {
        pub icon_image: gtk::Image,
        #[property(get, set = Self::set_image)]
        pub image: RefCell<String>,
        #[property(get, set = Self::set_colored)]
        pub colored: std::cell::Cell<bool>,
    }

    impl DistroIcon {
        fn set_image(&self, image: &str) {
            self.image.replace(image.to_string());
            
            let distro = known_distro_by_image(image);
            if let Some(distro) = &distro {
                let icon_theme = gtk::IconTheme::for_display(&self.icon_image.display());
                let icon = icon_theme.lookup_icon(
                    &distro.icon_name(),
                    &[KnownDistro::default_icon_name()],
                    32,
                    1,
                    gtk::TextDirection::None,
                    gtk::IconLookupFlags::empty(),
                );

                self.icon_image.set_paintable(Some(&icon));
                
                // Remove any existing distro-specific classes (but not distro-color-fg)
                let css_classes = self.icon_image.css_classes();
                for i in 0..css_classes.len() {
                    if let Some(gstring) = css_classes.get(i) {
                        let class = gstring.as_str();
                        if class.starts_with("distro-") && class != "distro-color-fg" {
                            self.icon_image.remove_css_class(class);
                        }
                    }
                }
                
                self.icon_image.add_css_class(&format!("distro-{}", distro.name()));
            } else {
                self.icon_image.set_icon_name(Some(KnownDistro::default_icon_name()));
            }
        }

        fn set_colored(&self, colored: bool) {
            self.colored.set(colored);
            
            if colored {
                self.icon_image.add_css_class("distro-color-fg");
            } else {
                self.icon_image.remove_css_class("distro-color-fg");
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for DistroIcon {
        fn constructed(&self) {
            self.parent_constructed();
            
            let obj = self.obj();
            
            // Configure the icon image
            self.icon_image.set_icon_size(gtk::IconSize::Large);
            self.icon_image.add_css_class("distro-color-fg");
            
            // Add the icon image to the box
            obj.append(&self.icon_image);
            
            // Initialize colored state
            self.colored.set(true);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DistroIcon {
        const NAME: &'static str = "DistroIcon";
        type Type = super::DistroIcon;
        type ParentType = gtk::Box;
    }

    impl WidgetImpl for DistroIcon {}
    impl BoxImpl for DistroIcon {}
}

glib::wrapper! {
    pub struct DistroIcon(ObjectSubclass<imp::DistroIcon>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl DistroIcon {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
}

impl Default for DistroIcon {
    fn default() -> Self {
        Self::new()
    }
}
