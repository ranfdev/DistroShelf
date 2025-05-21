use crate::{
    distrobox::{Command, ContainerInfo, ExportableApp, Status},
    distrobox_task::DistroboxTask,
    known_distros::{known_distro_by_image, KnownDistro},
    remote_resource::RemoteResource,
    root_store::RootStore,
};

use gtk::{
    gio,
    glib::{derived_properties, BoxedAnyObject, Properties},
};

use adw::prelude::*;
use glib::subclass::prelude::*;
use gtk::glib;
use std::{cell::RefCell, path::Path};

mod imp {
    use crate::remote_resource::RemoteResource;

    use super::*;

    // This contains all the container informations given by distrobox, plus an associated KnownDistro struct
    #[derive(Default, Properties)]
    #[properties(wrapper_type=super::Container)]
    pub struct Container {
        #[property(get, set)]
        root_store: RefCell<RootStore>,
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
    pub fn from_info(root_store: &RootStore, value: ContainerInfo) -> Self {
        let distro = known_distro_by_image(&value.image);

        let (status_tag, status_detail) = match value.status {
            Status::Up(v) => ("up", v),
            Status::Created(v) => ("created", v),
            Status::Exited(v) => ("exited", v),
            Status::Other(v) => ("other", v),
        };
        let this: Self = glib::Object::builder()
            .property("root-store", root_store)
            .property("name", value.name)
            .property("image", value.image)
            .property("distro", distro)
            .property("status-tag", status_tag)
            .property("status-detail", status_detail)
            .build();

        let this_clone = this.clone();
        let loader = move |apps_list: Option<&gio::ListStore>| {
            dbg!(apps_list);
            let this = this_clone.clone();
            let mut apps_list = apps_list
                .cloned()
                .unwrap_or_else(gio::ListStore::new::<BoxedAnyObject>);
            async move {
                let apps = this
                    .root_store()
                    .distrobox()
                    .list_apps(&this.name())
                    .await?;

                apps_list.remove_all();
                apps_list.extend(apps.into_iter().map(BoxedAnyObject::new));

                // Listing the apps starts the container, we need to update its status
                this.root_store().load_containers();
                Ok(apps_list)
            }
        };
        this.set_apps(RemoteResource::new::<gio::ListStore, _>(loader));

        this
    }

}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}
