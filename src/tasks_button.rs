use crate::tagged_object::TaggedObject;
use crate::{distrobox_task::DistroboxTask, root_store::RootStore};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::{derived_properties, Properties};
use gtk::{
    self,
    glib::{self},
};
use std::cell::RefCell;

use glib::clone;

mod imp {

    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::TasksButton)]
    pub struct TasksButton {
        #[property(get, set, construct)]
        pub root_store: RefCell<RootStore>,
        pub button: gtk::Button,
        pub warning_icon: gtk::Image,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TasksButton {
        const NAME: &'static str = "TasksButton";
        type Type = super::TasksButton;
        type ParentType = adw::Bin;
    }

    #[derived_properties]
    impl ObjectImpl for TasksButton {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            obj.set_margin_start(12);
            obj.set_margin_end(12);
            obj.set_margin_top(12);
            obj.set_margin_bottom(12);

            // Create a horizontal box with a "Tasks" label and warning icon
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let label = gtk::Label::new(Some("Tasks"));
            let warning_icon = &self.warning_icon;
            warning_icon.set_hexpand(true);
            warning_icon.set_halign(gtk::Align::End);
            warning_icon.set_icon_name(Some("dialog-warning-symbolic"));
            warning_icon.set_visible(false);

            hbox.append(&label);
            hbox.append(warning_icon);
            self.button.set_child(Some(&hbox));

            // When the button is clicked, present the TaskManagerDialog.
            self.button.connect_clicked(clone!(
                #[weak(rename_to=this)]
                obj,
                move |_| {
                    this.root_store()
                        .set_current_dialog(TaggedObject::new("task-manager"));
                }
            ));

            obj.set_child(Some(&self.button));

            let this_clone = obj.clone();
            obj.root_store().tasks().connect_items_changed(
                move |tasks, position, _removed, added| {
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
                    this_clone.imp().warning_icon.set_visible(has_warning);

                    // Listen when a new task will fail
                    for i in position..position + added {
                        dbg!(i);
                        let item = tasks.item(i);
                        let item: &DistroboxTask = item.and_downcast_ref().unwrap();
                        let this_clone = this_clone.clone();
                        item.connect_status_notify(move |item| {
                            if item.is_failed() {
                                this_clone.imp().warning_icon.set_visible(true);
                            }
                        });
                    }
                },
            );
        }
    }

    impl WidgetImpl for TasksButton {}
    impl BinImpl for TasksButton {}
}

// Implementation of the public interface
glib::wrapper! {
    pub struct TasksButton(ObjectSubclass<imp::TasksButton>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TasksButton {
    pub fn new(root_store: &RootStore) -> Self {
        glib::Object::builder()
            .property("root-store", root_store)
            .build()
    }
}
