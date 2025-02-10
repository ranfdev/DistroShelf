use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::gio::prelude::ListModelExt;
use gtk::gio::prelude::ListModelExtManual;
use gtk::glib::BoxedAnyObject;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::sync::OnceLock;

use crate::container::Container;
use crate::distrobox::ExportableApp;
use crate::distrobox_store::DistroboxStore;
use crate::distrobox_task::DistroboxTask;
use crate::gtk_utils::reaction;
use crate::root_store;
use crate::root_store::RootStore;
use crate::tagged_object::TaggedObject;

mod imp {
    use std::cell::Ref;

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::TaskManagerStore)]
    pub struct TaskManagerStore {
        #[property(get, set)]
        root_store: RefCell<RootStore>,
        #[property(get, set)]
        tasks: RefCell<gio::ListStore>,
        // current_view: "empty", "list"
        #[property(get, set)]
        current_view: RefCell<String>,
        #[property(get, set)]
        has_warning: RefCell<bool>,
        #[property(get, set, nullable)]
        selected_task: RefCell<Option<DistroboxTask>>,
    }

    impl Default for TaskManagerStore {
        fn default() -> Self {
            Self {
                root_store: Default::default(),
                tasks: RefCell::new(gio::ListStore::new::<DistroboxTask>()),
                current_view: RefCell::new(String::from("empty")),
                has_warning: Default::default(),
                selected_task: Default::default(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for TaskManagerStore {}

    #[glib::object_subclass]
    impl ObjectSubclass for TaskManagerStore {
        const NAME: &'static str = "TaskManagerStore";
        type Type = super::TaskManagerStore;
    }
}

glib::wrapper! {
    pub struct TaskManagerStore(ObjectSubclass<imp::TaskManagerStore>);
}
impl TaskManagerStore {
    pub fn new(root_store: &RootStore) -> Self {
        let this: Self = glib::Object::builder()
            .property("root-store", root_store)
            .property("tasks", &root_store.distrobox_store().tasks())
            .build();
        let distrobox_store = root_store.distrobox_store();
        let this_clone = this.clone();
        distrobox_store
            .tasks()
            .connect_items_changed(move |tasks, position, removed, added| {
                this_clone.update_current_view();

                // Show warning if a task already failed
                // This loop will reset the previous warning flag if there is no failed task
                let mut has_warning = false;
                for i in 0..tasks.n_items() {
                    let item = tasks.item(i);
                    let item: &DistroboxTask = item.and_downcast_ref().unwrap();
                    if item.is_failed() {
                        has_warning = true;
                        break;
                    }
                }
                this_clone.set_has_warning(has_warning);

                // Listen when a new task will fail
                for i in position..position + added {
                    dbg!(i);
                    let item = tasks.item(i);
                    let item: &DistroboxTask = item.and_downcast_ref().unwrap();
                    let this_clone = this_clone.clone();
                    item.connect_status_notify(move |item| {
                        if item.is_failed() {
                            this_clone.set_has_warning(true);
                        }
                    });
                }
            });
        this
    }
    pub fn update_current_view(&self) {
        let new_view = if self.root_store().distrobox_store().tasks().n_items() == 0 {
            "empty"
        } else {
            "list"
        };
        self.set_current_view(new_view);
    }
    pub fn back(&self) {
        self.set_selected_task(None::<&DistroboxTask>);
    }
    pub fn select(&self, task: &DistroboxTask) {
        self.set_selected_task(Some(task));
    }
    pub fn clear_ended_tasks(&self) {
        self.root_store().distrobox_store().clear_ended_tasks();
    }
}

impl Default for TaskManagerStore {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
