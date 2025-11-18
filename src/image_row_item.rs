use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::Properties;
use gtk::glib;
use std::cell::RefCell;

use crate::{distro_icon, known_distros::known_distro_by_image};

mod imp {
    use gtk::pango;

    use crate::known_distros::KnownDistro;

    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::ImageRowItem)]
    pub struct ImageRowItem {
        #[property(get, construct_only)]
        pub image: RefCell<Option<String>>,
        pub distro: RefCell<Option<KnownDistro>>,
        pub icon: gtk::Image,
        pub label: gtk::Label,
    }

    #[glib::derived_properties]
    impl ObjectImpl for ImageRowItem {
        fn constructed(&self) {
            distro_icon::setup(&self.icon);

            self.label.set_xalign(0.0);
            self.label.set_ellipsize(pango::EllipsizeMode::Middle);
            self.label.set_has_tooltip(true);
            

            let obj = self.obj();
            obj.add_css_class("distro-row-item");
            obj.set_spacing(6);
            obj.append(&self.icon);
            obj.append(&self.label);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ImageRowItem {
        const NAME: &'static str = "ImageRowItem";
        type Type = super::ImageRowItem;
        type ParentType = gtk::Box;
    }

    impl WidgetImpl for ImageRowItem {}
    impl BoxImpl for ImageRowItem {}
}

glib::wrapper! {
    pub struct ImageRowItem(ObjectSubclass<imp::ImageRowItem>)
    @extends gtk::Box, gtk::Widget,
    @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}
impl ImageRowItem {
    pub fn new() -> Self {
        let this: Self = glib::Object::builder().build();
        this
    }
    pub fn set_image(&self, image: &str) {
        let imp = self.imp();

        distro_icon::set_image(&imp.icon, image);

        let distro = known_distro_by_image(image);
        imp.distro.replace(distro);

        imp.label.set_label(image);
        imp.label.set_tooltip_text(Some(image));
    }
}

impl Default for ImageRowItem {
    fn default() -> Self {
        Self::new()
    }
}
