// You can copy/paste this file every time you need a simple GObject
// to hold some data

use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib::BoxedAnyObject;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::sync::OnceLock;

use crate::container::Container;
use crate::distrobox::ExportableApp;
use crate::distrobox_service::DistroboxService;
use crate::tagged_object::TaggedObject;

mod imp {

    use gtk::glib::BoxedAnyObject;

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::ExportableAppsDialogModel)]
    pub struct ExportableAppsDialogModel {
        #[property(get, set)]
        distrobox_service: RefCell<DistroboxService>,
        #[property(get)]
        pub apps: gio::ListStore,
        #[property(get, set)]
        container: RefCell<Container>,
        #[property(get, set)]
        current_view: RefCell<String>,
        #[property(get, set)]
        error: RefCell<String>,
    }

    impl Default for ExportableAppsDialogModel {
        fn default() -> Self {
            Self {
                distrobox_service: Default::default(),
                apps: gio::ListStore::new::<BoxedAnyObject>(),
                current_view: RefCell::new("loading".into()),
                error: Default::default(),
                container: Default::default(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ExportableAppsDialogModel {}

    #[glib::object_subclass]
    impl ObjectSubclass for ExportableAppsDialogModel {
        const NAME: &'static str = "ExportableAppsDialogModel";
        type Type = super::ExportableAppsDialogModel;
    }
}

glib::wrapper! {
    pub struct ExportableAppsDialogModel(ObjectSubclass<imp::ExportableAppsDialogModel>);
}
impl ExportableAppsDialogModel {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
    pub fn reload_apps(&self) {
        self.set_current_view("loading");
        let this = self.clone();

        glib::MainContext::default().spawn_local(async move {
            let apps = this
                .distrobox_service()
                .list_apps(&this.container().name())
                .await;

            match apps {
                Ok(apps) => {
                    let mut apps_list = this.imp().apps.clone();
                    apps_list.remove_all();
                    apps_list.extend(apps.into_iter().map(|app| BoxedAnyObject::new(app)));
                    this.set_current_view("apps");
                }
                Err(e) => {
                    this.set_error(e.to_string());
                    this.set_current_view("error");
                }
            };
        });
    }
    pub fn export(&self, desktop_file_path: &str) {
        self.distrobox_service()
            .do_export(&self.container().name(), desktop_file_path);
        self.reload_apps();
    }
    pub fn unexport(&self, desktop_file_path: &str) {
        self.distrobox_service()
            .do_unexport(&self.container().name(), desktop_file_path);
        self.reload_apps();
    }
    pub fn launch(&self, app: ExportableApp) {
        self.distrobox_service()
            .do_launch(&self.container().name(), app);
    }
}

impl Default for ExportableAppsDialogModel {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
