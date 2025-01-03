// sidebar_row.rs

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{self, glib, pango};

use crate::{container::Container, distro_icon, known_distros::KnownDistro};

mod imp {
    use super::*;

    // Object holding the state
    #[derive(Default)]
    pub struct SidebarRow {
        // Widgets
        pub icon: gtk::Image,
        pub title_label: gtk::Label,
        pub subtitle_label: gtk::Label,
        pub text_box: gtk::Box,
        pub status_overlay: gtk::Overlay,
        pub status_dot: gtk::Box,

        // Data
        pub name: std::cell::RefCell<String>,
        pub status: std::cell::RefCell<String>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for SidebarRow {
        const NAME: &'static str = "SidebarRow";
        type Type = super::SidebarRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            // The layout of the widget is defined here
            klass.set_css_name("sidebar-row");
        }

        fn new() -> Self {
            Self {
                icon: gtk::Image::new(),
                title_label: gtk::Label::new(None),
                subtitle_label: gtk::Label::new(None),
                text_box: gtk::Box::new(gtk::Orientation::Vertical, 4),
                status_overlay: gtk::Overlay::new(),
                status_dot: gtk::Box::new(gtk::Orientation::Horizontal, 0),
                name: std::cell::RefCell::new(String::new()),
                status: std::cell::RefCell::new("inactive".to_string()),
            }
        }
    }

    // Trait shared by all GObjects
    impl ObjectImpl for SidebarRow {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            // Configure the main box (self)
            obj.set_orientation(gtk::Orientation::Horizontal);
            obj.set_spacing(12);
            obj.set_margin_start(6);
            obj.set_margin_end(6);
            obj.set_margin_top(8);
            obj.set_margin_bottom(8);

            // Configure the icon
            distro_icon::setup(&self.icon);

            // Configure the labels
            self.title_label.set_halign(gtk::Align::Start);
            self.title_label.add_css_class("heading");

            self.subtitle_label.set_halign(gtk::Align::Start);
            self.subtitle_label.add_css_class("caption");
            self.subtitle_label.set_opacity(0.7);

            // Configure ellipsization for both labels
            self.title_label.set_ellipsize(pango::EllipsizeMode::Middle);
            self.subtitle_label.set_ellipsize(pango::EllipsizeMode::Middle);

            // Configure status dot
            self.status_dot.set_size_request(8, 8);
            self.status_dot.add_css_class("status-dot");
            self.status_dot.add_css_class("inactive");
            self.status_dot.set_valign(gtk::Align::Start);
            self.status_dot.set_halign(gtk::Align::End);
            self.status_dot.set_margin_end(2);
            self.status_dot.set_margin_top(2);

            // Build the widget hierarchy
            self.text_box.append(&self.title_label);
            self.text_box.append(&self.subtitle_label);

            let content_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            content_box.append(&self.icon);
            content_box.append(&self.text_box);

            self.status_overlay.set_child(Some(&content_box));
            self.status_overlay.add_overlay(&self.status_dot);

            obj.append(&self.status_overlay);
        }
    }

    // Trait shared by all widgets
    impl WidgetImpl for SidebarRow {}

    // Trait shared by all boxes
    impl BoxImpl for SidebarRow {}
}

// Implementation of the public interface
glib::wrapper! {
    pub struct SidebarRow(ObjectSubclass<imp::SidebarRow>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl SidebarRow {
    pub fn new(container: &Container) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.set_data(container);
        obj
    }

    fn set_data(&self, container: &Container) {
        let imp = self.imp();
        imp.name.replace(container.name());

        distro_icon::set_image(&imp.icon, &container.image());

        imp.title_label.set_text(&container.name());
        imp.subtitle_label.set_text(&container.image());
        
        // Update status indicator
        self.set_status(container.status_str());
    }

    pub fn name(&self) -> String {
        self.imp().name.borrow().clone()
    }

    pub fn title(&self) -> String {
        self.imp().title_label.text().to_string()
    }

    pub fn subtitle(&self) -> String {
        self.imp().subtitle_label.text().to_string()
    }

    pub fn set_status(&self, status: &str) {
        let imp = self.imp();
        imp.status.replace(status.to_string());
        
        // Remove all status classes
        imp.status_dot.remove_css_class("active");
        imp.status_dot.remove_css_class("inactive");
        imp.status_dot.remove_css_class("error");
        
        // Add the appropriate class
        imp.status_dot.add_css_class(status);
    }

    pub fn status(&self) -> String {
        self.imp().status.borrow().clone()
    }
}
