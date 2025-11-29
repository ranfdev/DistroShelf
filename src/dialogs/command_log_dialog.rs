use crate::fakers::CommandRunnerEvent;
use crate::i18n::gettext;
use crate::models::RootStore;
use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::{Properties, derived_properties};
use gtk::{gdk, glib};
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::CommandLogDialog)]
    pub struct CommandLogDialog {
        #[property(get, set, construct)]
        pub root_store: RefCell<RootStore>,
        pub toast_overlay: adw::ToastOverlay,
        pub list_box: gtk::ListBox,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CommandLogDialog {
        const NAME: &'static str = "CommandLogDialog";
        type Type = super::CommandLogDialog;
        type ParentType = adw::Dialog;
    }

    #[derived_properties]
    impl ObjectImpl for CommandLogDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.set_title(&gettext("Command Log"));
            obj.set_content_width(800);
            obj.set_content_height(600);

            // Create toolbar view
            let toolbar_view = adw::ToolbarView::new();

            // Create header bar
            let header_bar = adw::HeaderBar::new();
            header_bar.set_title_widget(Some(&adw::WindowTitle::new(&gettext("Command Log"), "")));
            toolbar_view.add_top_bar(&header_bar);

            // Create main content
            let main_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

            // Create scrolled window for command list
            let scrolled_window = gtk::ScrolledWindow::new();
            scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
            scrolled_window.set_vexpand(true);

            let content_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
            content_box.set_margin_start(12);
            content_box.set_margin_end(12);
            content_box.set_margin_top(12);
            content_box.set_margin_bottom(12);

            let description = gtk::Label::new(Some(&gettext(
                "Executing a single task (eg: listing applications) may require multiple chains of commands. For debugging purposes, this log shows all commands executed by the application.",
            )));
            description.set_wrap(true);
            description.set_xalign(0.0);
            description.add_css_class("dim-label");
            content_box.append(&description);

            // Create list box to show all commands
            self.list_box.set_selection_mode(gtk::SelectionMode::None);
            content_box.append(&self.list_box);

            scrolled_window.set_child(Some(&content_box));
            main_box.append(&scrolled_window);

            self.toast_overlay.set_child(Some(&main_box));
            toolbar_view.set_content(Some(&self.toast_overlay));
            obj.set_child(Some(&toolbar_view));

            // Populate the command list (root_store is already set via construct property)
            obj.populate_command_list();
        }
    }

    impl WidgetImpl for CommandLogDialog {}
    impl AdwDialogImpl for CommandLogDialog {}
}

glib::wrapper! {
    pub struct CommandLogDialog(ObjectSubclass<imp::CommandLogDialog>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl CommandLogDialog {
    pub fn new(root_store: RootStore) -> Self {
        glib::Object::builder()
            .property("root-store", root_store)
            .build()
    }

    fn populate_command_list(&self) {
        let list_box = &self.imp().list_box;

        // Get command events from output tracker
        let command_events = self.root_store().command_runner().output_tracker().items();

        for event in command_events {
            let row = self.build_event_row(&event);
            list_box.append(&row);
        }
    }

    fn build_event_row(&self, event: &CommandRunnerEvent) -> gtk::ListBoxRow {
        let toast_overlay = &self.imp().toast_overlay;

        match event {
            CommandRunnerEvent::Spawned(id, command) => self.build_command_row(
                &format!("Spawned [{}]", id),
                &command.to_string(),
                "media-playback-start-symbolic",
                "spawned",
                toast_overlay,
            ),
            CommandRunnerEvent::Started(id, command) => self.build_command_row(
                &format!("Started [{}]", id),
                &command.to_string(),
                "system-run-symbolic",
                "started",
                toast_overlay,
            ),
            CommandRunnerEvent::Output(id, result) => {
                let (title, icon, css_class) = match result {
                    Ok(_) => (
                        format!("Completed [{}]", id),
                        "object-select-symbolic",
                        "success",
                    ),
                    Err(_) => (format!("Failed [{}]", id), "dialog-error-symbolic", "error"),
                };
                self.build_status_row(&title, icon, css_class)
            }
        }
    }

    fn build_command_row(
        &self,
        title: &str,
        command_str: &str,
        icon_name: &str,
        css_class: &str,
        toast_overlay: &adw::ToastOverlay,
    ) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row_box.set_margin_start(6);
        row_box.set_margin_end(6);
        row_box.set_margin_top(3);
        row_box.set_margin_bottom(3);

        let status_icon = gtk::Image::from_icon_name(icon_name);
        status_icon.add_css_class(css_class);
        status_icon.set_pixel_size(12);

        let label_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let title_label = gtk::Label::new(Some(title));
        title_label.set_xalign(0.0);
        title_label.add_css_class("caption");

        let subtitle_label = gtk::Label::new(Some(command_str));
        subtitle_label.set_xalign(0.0);
        subtitle_label.add_css_class("caption");
        subtitle_label.add_css_class("dim-label");
        subtitle_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

        label_box.append(&title_label);
        label_box.append(&subtitle_label);

        row_box.append(&status_icon);
        row_box.append(&label_box);
        row.set_child(Some(&row_box));

        // Add click handler to copy command to clipboard
        let command_owned = command_str.to_string();
        let gesture = gtk::GestureClick::new();
        gesture.connect_pressed(glib::clone!(
            #[weak]
            toast_overlay,
            move |_, _, _, _| {
                if let Some(display) = gdk::Display::default() {
                    let clipboard = display.clipboard();
                    clipboard.set_text(&command_owned);
                    toast_overlay.add_toast(adw::Toast::new(&gettext("Command copied to clipboard")));
                }
            }
        ));
        row.add_controller(gesture);

        row
    }

    fn build_status_row(&self, title: &str, icon_name: &str, css_class: &str) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row_box.set_margin_start(6);
        row_box.set_margin_end(6);
        row_box.set_margin_top(3);
        row_box.set_margin_bottom(3);

        let status_icon = gtk::Image::from_icon_name(icon_name);
        status_icon.add_css_class(css_class);
        status_icon.set_pixel_size(12);

        let title_label = gtk::Label::new(Some(title));
        title_label.set_xalign(0.0);
        title_label.add_css_class("caption");

        row_box.append(&status_icon);
        row_box.append(&title_label);
        row.set_child(Some(&row_box));

        row
    }
}
