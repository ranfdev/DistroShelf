// sidebar_row.rs

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{self, glib, pango};

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

        // Data
        pub id: std::cell::RefCell<String>,
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
                id: std::cell::RefCell::new(String::new()),
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
            obj.set_margin_start(12);
            obj.set_margin_end(12);
            obj.set_margin_top(8);
            obj.set_margin_bottom(8);

            // Configure the icon
            self.icon.set_icon_size(gtk::IconSize::Large);

            // Configure the labels
            self.title_label.set_halign(gtk::Align::Start);
            self.title_label.add_css_class("heading");

            self.subtitle_label.set_halign(gtk::Align::Start);
            self.subtitle_label.add_css_class("caption");
            self.subtitle_label.set_opacity(0.7);

            // Configure ellipsization for both labels
            self.title_label.set_ellipsize(pango::EllipsizeMode::End);
            self.subtitle_label.set_ellipsize(pango::EllipsizeMode::End);

            // Build the widget hierarchy
            self.text_box.append(&self.title_label);
            self.text_box.append(&self.subtitle_label);

            obj.append(&self.icon);
            obj.append(&self.text_box);
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
    pub fn new(id: &str, icon_name: &str, title: &str, subtitle: &str) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.set_data(id, icon_name, title, subtitle);
        obj
    }

    pub fn set_data(&self, id: &str, icon_name: &str, title: &str, subtitle: &str) {
        let imp = self.imp();
        imp.id.replace(id.to_string());
        imp.icon.set_icon_name(Some(icon_name));
        imp.title_label.set_text(title);
        imp.subtitle_label.set_text(subtitle);
    }

    pub fn id(&self) -> String {
        self.imp().id.borrow().clone()
    }

    pub fn title(&self) -> String {
        self.imp().title_label.text().to_string()
    }

    pub fn subtitle(&self) -> String {
        self.imp().subtitle_label.text().to_string()
    }
}
