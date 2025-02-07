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
use crate::distrobox_store::DistroboxStore;
use crate::tagged_object::TaggedObject;

mod imp {

    use gtk::glib::BoxedAnyObject;

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::ExportableAppsStore)]
    pub struct ExportableAppsStore {
        #[property(get, set)]
        distrobox_store: RefCell<DistroboxStore>,
        #[property(get)]
        pub apps: gio::ListStore,
        #[property(get, set)]
        container: RefCell<Container>,
        #[property(get, set)]
        current_view: RefCell<String>,
        #[property(get, set)]
        error: RefCell<String>,
    }

    impl Default for ExportableAppsStore {
        fn default() -> Self {
            Self {
                distrobox_store: Default::default(),
                apps: gio::ListStore::new::<BoxedAnyObject>(),
                current_view: RefCell::new("loading".into()),
                error: Default::default(),
                container: Default::default(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for ExportableAppsStore {}

    #[glib::object_subclass]
    impl ObjectSubclass for ExportableAppsStore {
        const NAME: &'static str = "ExportableAppsStore";
        type Type = super::ExportableAppsStore;
    }
}

glib::wrapper! {
    pub struct ExportableAppsStore(ObjectSubclass<imp::ExportableAppsStore>);
}
impl ExportableAppsStore {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
    pub fn reload_apps(&self) {
        self.set_current_view("loading");
        let this = self.clone();

        glib::MainContext::default().spawn_local(async move {
            let apps = this
                .distrobox_store()
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
        self.distrobox_store()
            .do_export(&self.container().name(), desktop_file_path);
        self.reload_apps();
    }
    pub fn unexport(&self, desktop_file_path: &str) {
        self.distrobox_store()
            .do_unexport(&self.container().name(), desktop_file_path);
        self.reload_apps();
    }
    pub fn launch(&self, app: ExportableApp) {
        self.distrobox_store()
            .do_launch(&self.container().name(), app);
    }
}

impl Default for ExportableAppsStore {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
