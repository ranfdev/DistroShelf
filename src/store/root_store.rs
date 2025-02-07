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
use crate::distrobox::CreateArgs;
use crate::distrobox_store::DistroboxStore;
use crate::distrobox_task::DistroboxTask;
use crate::gtk_utils::reconcile_list_by_key;
use crate::tagged_object::TaggedObject;
use crate::welcome_view_store::WelcomeViewStore;
use crate::exportable_apps_store::ExportableAppsStore;

mod imp {

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::RootStore)]
    pub struct RootStore {
        #[property(get, set)]
        distrobox_store: RefCell<DistroboxStore>,
        #[property(get)]
        containers: gio::ListStore,
        #[property(get, set, nullable)]
        selected_container: RefCell<Option<crate::container::Container>>,
        #[property(get, set)]
        current_sidebar_view: RefCell<String>,
        #[property(get, set)]
        current_view: RefCell<TaggedObject>,
        #[property(get, set)]
        current_dialog: RefCell<TaggedObject>,
    }

    impl Default for RootStore {
        fn default() -> Self {
            Self {
                distrobox_store: Default::default(),
                containers: gio::ListStore::new::<crate::container::Container>(),
                selected_container: Default::default(),
                current_view: Default::default(),
                current_sidebar_view: Default::default(),
                current_dialog: Default::default(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for RootStore {}

    #[glib::object_subclass]
    impl ObjectSubclass for RootStore {
        const NAME: &'static str = "RootStore";
        type Type = super::RootStore;
    }
}

glib::wrapper! {
    pub struct RootStore(ObjectSubclass<imp::RootStore>);
}
impl RootStore {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
    pub fn bind_distrobox_store(self, store: &DistroboxStore) {
        let this = self.clone();
        self.set_distrobox_store(store);

        store.connect_version_changed(move |store| {
            if store.version().is_error() {
                this.set_current_view(&TaggedObject::with_object("welcome", &WelcomeViewStore::new(&this)));
            }
        });
        let this = self.clone();
        store.connect_containers_changed(move |store| {
            if let Some(data) = store.containers().data() {
                let values: Vec<_> = data.values().cloned().collect();
                reconcile_list_by_key(this.containers(), &values[..], |item| item.name());
                if values.len() == 0 {
                    this.set_current_sidebar_view("no-distroboxes");
                } else {
                    this.set_current_sidebar_view("distroboxes");
                }
            }
        });
        store.load_container_infos();
    }

    pub fn selected_container_name(&self) -> Option<String> {
        self.selected_container().map(|c| c.name())
    }

    pub fn upgrade_container(&self) {
        let task = self.distrobox_store()
            .do_upgrade(&self.selected_container_name().unwrap());
        self.view_task(&task);
    }
    pub fn clone_container(&self, new_name: &str) {
        if let Some(container) = self.selected_container() {
            if !new_name.is_empty() {
                if container.status_tag() == "up" {
                    self.distrobox_store().do_stop(&container.name());
                }
                let task = self
                    .distrobox_store()
                    .do_clone(&container.name(), &new_name);
                self.view_task(&task);
            }
        }
    }
    pub fn view_task(&self, task: &DistroboxTask) {
                self.set_current_dialog(TaggedObject::with_object("task", task));
    }
    pub fn view_exportable_apps(&self) {
        let this = self.clone();
        let exportable_apps_store = ExportableAppsStore::new();
        exportable_apps_store.set_distrobox_store(self.distrobox_store());
        exportable_apps_store.set_container(this.selected_container().unwrap());
        this.set_current_dialog(TaggedObject::with_object("exportable-apps", &exportable_apps_store));

        exportable_apps_store.reload_apps();
    }
    pub fn create_container(&self, args: CreateArgs) {
        let task = self.distrobox_store().do_create(args);
        self.view_task(&task);
    }
}

impl Default for RootStore {
    fn default() -> Self {
        Self::new()
    }
}
