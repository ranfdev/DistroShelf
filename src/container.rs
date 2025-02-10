use crate::{
    distrobox::{ContainerInfo, Status},
    known_distros::{known_distro_by_image, KnownDistro}, remote_resource::RemoteResource,
};

use crate::tagged_object::TaggedObject;
use gtk::glib::{derived_properties, Properties};

use adw::prelude::*;
use glib::subclass::prelude::*;
use gtk::glib;
use std::cell::RefCell;

mod imp {
    use crate::remote_resource::RemoteResource;

    use super::*;

    // This contains all the container informations given by distrobox, plus an associated KnownDistro struct
    #[derive(Default, Properties)]
    #[properties(wrapper_type=super::Container)]
    pub struct Container {
        #[property(get, set)]
        pub name: RefCell<String>,
        #[property(get, set)]
        pub status_tag: RefCell<String>,
        #[property(get, set)]
        pub status_detail: RefCell<String>,
        #[property(get, set)]
        pub image: RefCell<String>,
        #[property(get, set)]
        pub distro: RefCell<Option<KnownDistro>>,
        #[property(get, set)]
        pub apps: RefCell<RemoteResource>,
    }

    #[derived_properties]
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
}

impl From<ContainerInfo> for Container {
    fn from(value: ContainerInfo) -> Self {
        let distro = known_distro_by_image(&value.image);

        let (status_tag, status_detail) = match value.status {
            Status::Up(v) => ("up", v),
            Status::Created(v) => ("created", v),
            Status::Exited(v) => ("exited", v),
            Status::Other(v) => ("other", v),
        };
        let this: Self = glib::Object::builder()
            .property("name", value.name)
            .property("image", value.image)
            .property("distro", distro)
            .property("status-tag", status_tag)
            .property("status-detail", status_detail)
            .build();

        this
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}
