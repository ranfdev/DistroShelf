use crate::{
    distrobox::{ContainerInfo, Status},
    known_distros::{known_distro_by_image, KnownDistro},
};

use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib;
use std::cell::RefCell;
use std::sync::OnceLock;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Container {
        pub name: RefCell<String>,
        pub status: RefCell<Status>,
        pub image: RefCell<String>,
        pub distro: RefCell<Option<KnownDistro>>,
    }

    impl ObjectImpl for Container {}

    #[glib::object_subclass]
    impl ObjectSubclass for Container {
        const NAME: &'static str = "Container";
        type Type = super::Container;
    }
}

glib::wrapper! {
    pub struct Container(ObjectSubclass<imp::Container>);
}
impl Container {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn name(&self) -> String {
        self.imp().name.borrow().clone()
    }

    pub fn status(&self) -> Status {
        self.imp().status.borrow().clone()
    }


    pub fn image(&self) -> String {
        self.imp().image.borrow().clone()
    }

    pub fn distro(&self) -> Option<KnownDistro> {
        self.imp().distro.borrow().clone()
    }
}

impl From<ContainerInfo> for Container {
    fn from(value: ContainerInfo) -> Self {
        let distro = known_distro_by_image(&value.image);

        let this = Self::new();
        this.imp().name.replace(value.name);
        this.imp().status.replace(value.status);
        this.imp().image.replace(value.image);
        this.imp().distro.replace(distro);
        this
    }
}
