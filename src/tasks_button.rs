use crate::{
    distrobox_task::DistroboxTask,
    root_store::{self, RootStore},
    task_manager_dialog::TaskManagerDialog,
    task_manager_store::TaskManagerStore,
};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{
    self,
    glib::{self, clone},
    pango,
};
use im_rc::Vector;
use std::sync::OnceLock;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::clone;
    use gtk::glib::{derived_properties, Properties};

    use crate::{
        gtk_utils::reaction, root_store::RootStore, tagged_object::TaggedObject,
        task_manager_store::TaskManagerStore,
    };

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

            // Bind the warning icon's visibility to the store's "has-warning" property.
            obj.root_store()
                .task_manager_store()
                .unwrap()
                .bind_property("has-warning", warning_icon, "visible")
                .sync_create()
                .build();

            hbox.append(&label);
            hbox.append(warning_icon);
            self.button.set_child(Some(&hbox));

            // When the button is clicked, present the TaskManagerDialog.
            self.button.connect_clicked(clone!(
                #[weak(rename_to=this)]
                obj,
                move |_| {
                    this.root_store()
                        .set_current_dialog(TaggedObject::with_object(
                            "task-manager",
                            &this.root_store().task_manager_store().unwrap(),
                        ));
                }
            ));

            obj.set_child(Some(&self.button));
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
