use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use gtk::glib::clone;

use crate::gtk_utils::reaction;
use crate::models::{DistroboxTask, RootStore};

use gtk::glib::{Properties, derived_properties};
use std::cell::RefCell;

mod imp {

    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type=super::TaskManagerDialog)]
    pub struct TaskManagerDialog {
        #[property(get, construct_only)]
        pub root_store: RefCell<RootStore>,
        pub dialog: adw::Dialog,
        pub toolbar_view: adw::ToolbarView,
        pub navigation_view: adw::NavigationView,
        pub content: gtk::Box,
        pub scrolled_window: gtk::ScrolledWindow,
        pub stack: gtk::Stack,
        pub list_page_content: gtk::Box,
        pub list_box: gtk::ListBox,
        pub status_page: adw::StatusPage,

        pub selected_task_view: adw::ToolbarView,
    }

    #[derived_properties]
    impl ObjectImpl for TaskManagerDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_title("Running Tasks");
            obj.set_content_width(360);
            obj.set_content_height(640);

            let header_bar = adw::HeaderBar::new();
            self.toolbar_view.add_top_bar(&header_bar);

            self.content.set_orientation(gtk::Orientation::Vertical);
            self.content.set_spacing(6);

            self.scrolled_window.set_vexpand(true);
            self.scrolled_window.set_propagate_natural_height(true);

            self.stack
                .set_transition_type(gtk::StackTransitionType::Crossfade);

            self.status_page.set_title("No Running Tasks");
            self.status_page.set_description(Some(
                "Tasks such as starting, stopping and upgrading will appear here.",
            ));
            self.stack.add_named(&self.status_page, Some("empty"));
            self.stack.set_visible_child_name("empty");

            self.list_box.set_selection_mode(gtk::SelectionMode::None);
            self.list_box.set_valign(gtk::Align::Start);
            self.list_box.add_css_class("boxed-list");
            self.list_box.set_margin_bottom(12);

            let this = self.obj().clone();
            let root_store = self.obj().root_store();
            self.list_box
                .bind_model(Some(root_store.tasks().inner()), move |obj| {
                    let task = obj.downcast_ref::<DistroboxTask>().unwrap();
                    this.build_row(task).upcast()
                });

            self.list_page_content
                .set_orientation(gtk::Orientation::Vertical);
            self.list_page_content.set_spacing(6);
            self.list_page_content.set_vexpand(true);
            self.list_page_content.set_margin_top(12);
            self.list_page_content.set_margin_bottom(12);
            self.list_page_content.set_margin_start(12);
            self.list_page_content.set_margin_end(12);
            self.list_page_content.append(&self.list_box);

            let clear_btn = gtk::Button::with_label("Clear Ended Tasks");
            clear_btn.set_valign(gtk::Align::End);
            clear_btn.connect_clicked(clone!(
                #[weak(rename_to=this)]
                obj,
                move |_| {
                    this.root_store().clear_ended_tasks();
                }
            ));
            self.list_page_content.append(&clear_btn);
            self.stack.add_named(&self.list_page_content, Some("list"));

            self.selected_task_view.add_top_bar(&adw::HeaderBar::new());

            self.navigation_view.add(&adw::NavigationPage::new(
                &self.toolbar_view,
                "Manage Tasks",
            ));
            let this = self.obj().clone();
            if root_store.tasks().len() == 0 {
                this.imp().stack.set_visible_child_name("empty");
            } else {
                this.imp().stack.set_visible_child_name("list");
            }
            root_store
                .tasks()
                .inner()
                .connect_items_changed(move |tasks, _, _, _| {
                    dbg!(tasks.n_items());
                    if tasks.n_items() == 0 {
                        this.imp().stack.set_visible_child_name("empty");
                    } else {
                        this.imp().stack.set_visible_child_name("list");
                    }
                });

            let this = self.obj().clone();
            reaction!(root_store.selected_task(), move |task: Option<
                DistroboxTask,
            >| {
                if let Some(task) = task {
                    this.build_task_view(&task);
                    this.imp().navigation_view.push(&adw::NavigationPage::new(
                        &this.imp().selected_task_view,
                        "Task Details",
                    ));
                }
            });
            let this = self.obj().clone();
            self.navigation_view.connect_popped(move |_, _| {
                this.root_store().set_selected_task(None::<&DistroboxTask>);
            });

            let this = self.obj().clone();
            self.obj().connect_closed(move |_| {
                this.root_store().set_selected_task(None::<&DistroboxTask>);
            });

            self.scrolled_window.set_child(Some(&self.stack));
            self.content.append(&self.scrolled_window);

            self.toolbar_view.set_content(Some(&self.content));
            self.obj().set_child(Some(&self.navigation_view));
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TaskManagerDialog {
        const NAME: &'static str = "TaskManagerDialog";
        type Type = super::TaskManagerDialog;
        type ParentType = adw::Dialog;
    }

    impl WidgetImpl for TaskManagerDialog {}
    impl AdwDialogImpl for TaskManagerDialog {}
}

glib::wrapper! {
    pub struct TaskManagerDialog(ObjectSubclass<imp::TaskManagerDialog>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}

impl TaskManagerDialog {
    pub fn new(root_store: &RootStore) -> Self {
        // Build the dialog with the TaskManagerStore
        let this: Self = glib::Object::builder()
            .property("root-store", root_store)
            .build();

        this
    }

    // Build a row representing a running task.
    pub fn build_row(&self, task: &DistroboxTask) -> adw::ActionRow {
        let row = adw::ActionRow::new();
        row.set_title(&format!("{}: {}", task.target(), task.name()));
        row.set_subtitle(&task.status());

        task.connect_status_notify(clone!(
            #[weak]
            row,
            move |task| {
                row.set_subtitle(&task.status());
            }
        ));

        row.set_activatable(true);
        row.connect_activated(clone!(
            #[strong]
            task,
            #[weak(rename_to=this)]
            self,
            move |_| {
                this.root_store().set_selected_task(Some(&task));
            }
        ));
        row
    }

    fn build_task_view(&self, task: &DistroboxTask) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_bottom(12);

        let label = gtk::Label::new(Some(&format!("{}: {}", task.target(), task.name())));
        label.set_xalign(0.0);
        content.append(&label);

        let status_label = gtk::Label::new(Some(&format!("Status: {}", task.status())));
        status_label.set_xalign(0.0);
        content.append(&status_label);

        let description = task.description();
        if !description.is_empty() {
            let label = gtk::Label::new(Some(&description));
            label.set_xalign(0.0);
            label.set_wrap(true);
            content.append(&label);
        }

        // Error label
        let error_label = gtk::Label::new(None);
        error_label.set_xalign(0.0);
        error_label.set_wrap(true);
        error_label.add_css_class("error");
        content.append(&error_label);

        let update_status_ui = clone!(
            #[weak]
            status_label,
            #[weak]
            error_label,
            move |task: &DistroboxTask| {
                status_label.set_text(&format!("Status: {}", task.status()));
                if task.is_failed() {
                    if let Some(error) = task.error_message() {
                        error_label.set_text(&format!("Error: {}", error));
                        error_label.set_visible(true);
                    } else {
                        error_label.set_visible(false);
                    }
                } else {
                    error_label.set_visible(false);
                }
            }
        );

        // Initial update
        update_status_ui(task);

        task.connect_status_notify(move |task| {
            update_status_ui(task);
        });

        let text_view = gtk::TextView::builder()
            .buffer(&task.output())
            .editable(false)
            .cursor_visible(false)
            .wrap_mode(gtk::WrapMode::Word)
            .css_classes(vec!["output".to_string()])
            .top_margin(12)
            .bottom_margin(12)
            .left_margin(12)
            .right_margin(12)
            .build();

        let scrolled_window = gtk::ScrolledWindow::builder()
            .child(&text_view)
            .propagate_natural_height(true)
            .height_request(300)
            .vexpand(true)
            .build();
        content.append(&scrolled_window);

        let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        button_row.set_hexpand(true);
        button_row.set_homogeneous(true);

        let stop_btn = gtk::Button::with_label("Stop");
        stop_btn.connect_clicked(clone!(
            #[weak]
            task,
            move |_| {
                tracing::warn!(task_id = %task.name(), "Stop requested but not implemented yet");
                // TODO: implement this
                // task.stop();
            }
        ));
        stop_btn.add_css_class("destructive-action");
        stop_btn.add_css_class("pill");
        stop_btn.set_sensitive(!task.ended());
        task.connect_status_notify(clone!(
            #[weak]
            stop_btn,
            move |task| {
                stop_btn.set_sensitive(!task.ended());
            }
        ));

        // TODO: remove button row
        button_row.append(&stop_btn);
        content.append(&button_row);

        self.imp().selected_task_view.set_content(Some(&content));
    }
}
