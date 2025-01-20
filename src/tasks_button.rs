use crate::distrobox_task::DistroboxTask;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{self, glib::{self, clone}, pango};
use im_rc::Vector;
use std::sync::OnceLock;

mod imp {
    use std::cell::{Cell, RefCell};

    use glib::clone;

    use super::*;

    #[derive(Default)]
    pub struct TasksButton {
        pub menu_button: gtk::MenuButton,
        pub popover: gtk::Popover,
        pub main_content_box: gtk::Box,
        pub list_box: gtk::ListBox,
        pub status_page: adw::StatusPage,
        pub tasks: RefCell<Vector<DistroboxTask>>,
        pub pending_warning: Cell<bool>,

        pub status_stack: gtk::Stack,
        pub spinner: adw::Spinner,
        pub warning_icon: gtk::Image,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TasksButton {
        const NAME: &'static str = "TasksButton";
        type Type = super::TasksButton;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for TasksButton {
        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            obj.set_margin_start(12);
            obj.set_margin_end(12);
            obj.set_margin_top(12);
            obj.set_margin_bottom(12);

            self.list_box.set_selection_mode(gtk::SelectionMode::None);

            // Box to hold the list and clear button
            self.main_content_box.set_orientation(gtk::Orientation::Vertical);
            self.main_content_box.set_spacing(6);

            let clear_button = gtk::Button::builder()
                .label("Clear All Tasks")
                .css_classes(["flat"])
                .margin_start(6)
                .margin_end(6)
                .margin_bottom(6)
                .build();
            clear_button.connect_clicked(clone!(
                #[weak(rename_to=this)]
                self,
                move |_| {
                    this.obj().emit_by_name::<()>("clear-tasks-clicked", &[]);
                }
            ));

            self.main_content_box.append(&self.list_box);
            self.main_content_box.append(&clear_button);


            // Configure the menu button
            let obj= self.obj().clone();
            self.menu_button.set_popover(Some(&self.popover));
            self.menu_button.connect_active_notify(move |_| {
                obj.imp().pending_warning.set(false);
                obj.update_status_stack();
            });
            let obj = self.obj();

            let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            content.append(&gtk::Label::new(Some("Tasks")));

            
            let empty_bin = adw::Bin::new();
            self.status_stack.add_named(&empty_bin, Some("empty-bin"));
            self.status_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
            
            self.warning_icon.set_icon_name(Some("dialog-warning-symbolic"));
            self.warning_icon.set_halign(gtk::Align::End);
            self.status_stack.add_named(&self.warning_icon, Some("warning-icon"));

            self.spinner.set_hexpand(true);
            self.spinner.set_halign(gtk::Align::End);
            self.status_stack.add_named(&self.spinner, Some("spinner"));


            content.append(&self.status_stack);
            self.menu_button.set_child(Some(&content));

            // Configure the popover
            self.popover.set_position(gtk::PositionType::Bottom);
            self.popover.add_css_class("tasks-popover");

            // Add the menu button to the main widget
            obj.set_child(Some(&self.menu_button));

            // Configure the status page for no tasks
            self.status_page.set_title("No Tasks");
            self.status_page.set_description(Some(
                "Tasks such as adding or upgrading containers will be shown here",
            ));
            self.status_page.set_width_request(200);

            let popover = self.popover.clone();
            popover.set_child(Some(&self.status_page));

            // Connect signal to close the popover when an item is clicked
            self.list_box.connect_row_activated(clone!(
                #[weak]
                popover,
                move |_list_box, _row| {
                    popover.popdown();
                }
            ));
        }

        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("task-clicked")
                        .param_types([DistroboxTask::static_type()])
                        .build(),
                    glib::subclass::Signal::builder("clear-tasks-clicked")
                        .build(),
                ]
            })
        }
    }

    // Trait shared by all widgets
    impl WidgetImpl for TasksButton {}

    // Trait shared by all bins
    impl BinImpl for TasksButton {}
}

// Implementation of the public interface
glib::wrapper! {
    pub struct TasksButton(ObjectSubclass<imp::TasksButton>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TasksButton {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    fn add_task(&self, task: &DistroboxTask) {
        let row = self.build_task_row(task);
        self.imp().list_box.append(&row);

        task.connect_status_notify(clone!(
            #[weak(rename_to=this)]
            self, 
            move |task| {
            if task.ended() {
                if task.is_failed() {
                    this.imp().pending_warning.set(true);
                }
                this.update_status_stack();
            }
        }));
    }


    fn update_status_stack(&self) {
        let count_running = self.imp().tasks.borrow().iter().filter(|task| task.status() == "running").count();
        let count_failed = self.imp().tasks.borrow().iter().filter(|task| task.status() == "failed").count();

        let imp = self.imp();

        if count_running > 0 {
            self.imp().status_stack.set_visible_child_name("spinner");
        } else if imp.pending_warning.get() && count_failed > 0 {
            imp.status_stack.set_visible_child_name("warning-icon");
        } else {
            imp.status_stack.set_visible_child_name("empty-bin");
        }
    }

    pub fn update_tasks(&self, tasks: Vector<DistroboxTask>) {
        let imp = self.imp();
        while let Some(child) = imp.list_box.first_child() {
            imp.list_box.remove(&child);
        }
        self.update_status_stack();
        if tasks.is_empty() {
            imp.popover.set_child(Some(&imp.status_page));
        } else {
            imp.popover.set_child(Some(&imp.main_content_box));
            for task in &tasks {
                self.add_task(task);
            }
        }
        self.imp().tasks.replace(tasks);
    }

    fn build_task_row(&self, task: &DistroboxTask) -> gtk::Box {
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);

        let title_label = gtk::Label::new(Some(&format!("{}: {}", task.target(), task.name())));
        title_label.set_halign(gtk::Align::Start);
        title_label.set_ellipsize(pango::EllipsizeMode::End);
        title_label.add_css_class("title");

        let subtitle_label = gtk::Label::new(Some(&task.status()));
        task.connect_status_notify(clone!(
            #[weak]
            subtitle_label,
            move |task| {
                subtitle_label.set_text(&task.status());
            }
        ));
        subtitle_label.set_halign(gtk::Align::Start);
        subtitle_label.set_ellipsize(pango::EllipsizeMode::End);
        subtitle_label.add_css_class("subtitle");

        vbox.append(&title_label);
        vbox.append(&subtitle_label);

        let gesture = gtk::GestureClick::new();
        gesture.connect_released(
            clone!(
                #[weak(rename_to=this)]
                self,
                #[weak]
                task,
                move |_, _, _, _| {
                    this.emit_by_name::<()>("task-clicked", &[&task]);
                    println!("Task clicked: {}", task.name());
                }
            ),
        );
        vbox.add_controller(gesture);

        vbox
    }

    pub fn connect_task_clicked<F: Fn(&Self, &DistroboxTask) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("task-clicked", false, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let task = values[1].get::<DistroboxTask>().unwrap();
            f(&obj, &task);
            None
        })
    }

    pub fn connect_clear_tasks_clicked<F: Fn(&Self) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("clear-tasks-clicked", false, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            f(&obj);
            None
        })
    }
}
