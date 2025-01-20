use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::Properties;
use gtk::glib;
use std::cell::RefCell;

use crate::{distro_icon, known_distros::known_distro_by_image};

mod imp {
    use crate::known_distros::KnownDistro;

    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::DistroComboRowItem)]
    pub struct DistroComboRowItem {
        #[property(get, construct_only)]
        pub image: RefCell<Option<String>>,
        pub distro: RefCell<Option<KnownDistro>>,
        pub icon: gtk::Image,
        pub label: gtk::Label,
    }

    #[glib::derived_properties]
    impl ObjectImpl for DistroComboRowItem {
        fn constructed(&self) {
            distro_icon::setup(&self.icon);

            self.label.set_xalign(0.0);

            let obj = self.obj();
            obj.append(&self.icon);
            obj.append(&self.label);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DistroComboRowItem {
        const NAME: &'static str = "DistroComboRowItem";
        type Type = super::DistroComboRowItem;
        type ParentType = gtk::Box;
    }

    impl WidgetImpl for DistroComboRowItem {}
    impl BoxImpl for DistroComboRowItem {}
}

glib::wrapper! {
    pub struct DistroComboRowItem(ObjectSubclass<imp::DistroComboRowItem>)
    @extends gtk::Box, gtk::Widget;
}
impl DistroComboRowItem {
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
    }
}
