use crate::store::root_store::RootStore;
use crate::supported_terminals;
use crate::widgets::TerminalComboRow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::{Properties, clone, derived_properties};
use gtk::{gio, glib};
use std::cell::RefCell;
use tracing::error;

mod imp {
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::PreferencesDialog)]
    pub struct PreferencesDialog {
        #[property(get, set, construct)]
        pub root_store: RefCell<RootStore>,
        pub terminal_combo_row: RefCell<Option<TerminalComboRow>>,
        pub delete_btn: gtk::Button,
        pub add_terminal_btn: gtk::Button,
    }

    impl Default for PreferencesDialog {
        fn default() -> Self {
            Self {
                root_store: RefCell::new(RootStore::default()),
                terminal_combo_row: RefCell::new(None),
                delete_btn: gtk::Button::new(),
                add_terminal_btn: gtk::Button::new(),
            }
        }
    }

    #[derived_properties]
    impl ObjectImpl for PreferencesDialog {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.set_title("Preferences");

            let page = adw::PreferencesPage::new();

            // Terminal Settings Group
            let terminal_group = adw::PreferencesGroup::new();
            terminal_group.set_title("Terminal Settings");

            // Initialize terminal combo row
            let terminal_combo_row = TerminalComboRow::new_with_params(obj.root_store());
            self.terminal_combo_row
                .replace(Some(terminal_combo_row.clone()));

            // Initialize delete button
            self.delete_btn.set_label("Delete");
            self.delete_btn.add_css_class("destructive-action");
            self.delete_btn.add_css_class("pill");

            // Set initial delete button state
            if let Some(selected) = terminal_combo_row.selected_item() {
                let selected_name = selected
                    .downcast_ref::<gtk::StringObject>()
                    .unwrap()
                    .string();
                let is_read_only = obj
                    .root_store()
                    .terminal_repository()
                    .is_read_only(&selected_name);

                self.delete_btn.set_sensitive(!is_read_only);
            }

            // Connect delete button
            self.delete_btn.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.handle_delete_terminal();
                }
            ));

            // Update delete button when selection changes
            terminal_combo_row.connect_selected_item_notify(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.update_delete_button_state();
                }
            ));

            terminal_group.add(&terminal_combo_row);

            // Initialize add terminal button
            self.add_terminal_btn.set_label("Add Custom");
            self.add_terminal_btn.add_css_class("pill");
            self.add_terminal_btn.set_halign(gtk::Align::Start);

            // Connect add terminal button
            self.add_terminal_btn.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.show_add_terminal_dialog();
                }
            ));

            let button_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            button_box.set_margin_start(12);
            button_box.set_margin_end(12);
            button_box.set_margin_top(12);
            button_box.set_margin_bottom(12);

            button_box.append(&self.delete_btn);
            button_box.append(&self.add_terminal_btn);
            terminal_group.add(&button_box);

            page.add(&terminal_group);

            // Distrobox Settings Group
            let distrobox_group = adw::PreferencesGroup::new();
            distrobox_group.set_title("Distrobox Settings");

            let distrobox_source_row = adw::ComboRow::new();
            distrobox_source_row.set_title("Distrobox Source");
            let model = gtk::StringList::new(&["System (host)", "Bundled Version"]);
            distrobox_source_row.set_model(Some(&model));

            // Bind to settings
            let settings = gio::Settings::new("com.ranfdev.DistroShelf");
            // We need to map string to index and vice versa
            // 0 -> host, 1 -> bundled

            if settings.string("distrobox-executable") == "bundled" {
                distrobox_source_row.set_selected(1);
            } else {
                distrobox_source_row.set_selected(0);
            }

            distrobox_source_row.connect_selected_notify(move |row| {
                let settings = gio::Settings::new("com.ranfdev.DistroShelf");
                if row.selected() == 1 {
                    let _ = settings.set_string("distrobox-executable", "bundled");
                } else {
                    let _ = settings.set_string("distrobox-executable", "host");
                }
            });

            distrobox_group.add(&distrobox_source_row);

            // Add version row
            let version_row = adw::ActionRow::new();
            version_row.set_title("Distrobox Version");

            let version_label = gtk::Label::new(None);
            version_label.add_css_class("dim-label");
            version_row.add_suffix(&version_label);

            // Bind to distrobox_version query
            obj.root_store().distrobox_version().connect_success(clone!(
                #[weak]
                version_label,
                move |version| {
                    version_label.set_text(&version);
                }
            ));
            obj.root_store().distrobox_version().connect_error(clone!(
                #[weak]
                version_label,
                move |_| {
                    version_label.set_text("Not available");
                }
            ));

            // Set initial value if already loaded
            if let Some(version) = obj.root_store().distrobox_version().data() {
                version_label.set_text(&version);
            } else {
                version_label.set_text("—");
            }

            distrobox_group.add(&version_row);

            // Add "Re-download Bundled Version" button
            let redownload_btn = gtk::Button::new();
            redownload_btn.set_label("Re-download Bundled");
            redownload_btn.add_css_class("pill");
            redownload_btn.set_halign(gtk::Align::Center);
            redownload_btn.set_margin_top(12);
            redownload_btn.set_margin_bottom(12);

            redownload_btn.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    // Trigger download and open task manager
                    obj.root_store().download_distrobox();
                    obj.root_store()
                        .set_current_dialog(crate::tagged_object::TaggedObject::new(
                            "task-manager",
                        ));
                }
            ));

            distrobox_group.add(&redownload_btn);

            page.add(&distrobox_group);
            obj.add(&page);
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesDialog {
        const NAME: &'static str = "PreferencesDialog";
        type Type = super::PreferencesDialog;
        type ParentType = adw::PreferencesDialog;
    }

    impl WidgetImpl for PreferencesDialog {}
    impl AdwDialogImpl for PreferencesDialog {}
    impl PreferencesDialogImpl for PreferencesDialog {}
}

glib::wrapper! {
    pub struct PreferencesDialog(ObjectSubclass<imp::PreferencesDialog>)
        @extends adw::PreferencesDialog, adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}

impl PreferencesDialog {
    pub fn new(root_store: RootStore) -> Self {
        glib::Object::builder()
            .property("root-store", root_store)
            .build()
    }

    fn update_delete_button_state(&self) {
        let imp = self.imp();
        if let (Some(terminal_combo_row), Some(delete_btn)) = (
            imp.terminal_combo_row.borrow().as_ref(),
            Some(&imp.delete_btn),
        ) {
            if let Some(selected) = terminal_combo_row.selected_item() {
                let selected_name = selected
                    .downcast_ref::<gtk::StringObject>()
                    .unwrap()
                    .string();
                let is_read_only = self
                    .root_store()
                    .terminal_repository()
                    .is_read_only(&selected_name);

                delete_btn.set_sensitive(!is_read_only);
            }
        }
    }

    fn handle_delete_terminal(&self) {
        let imp = self.imp();
        let terminal_combo_row = match imp.terminal_combo_row.borrow().as_ref() {
            Some(row) => row.clone(),
            None => return,
        };

        let selected = terminal_combo_row
            .selected_item()
            .and_downcast_ref::<gtk::StringObject>()
            .unwrap()
            .string();

        let dialog = adw::AlertDialog::builder()
            .heading("Delete this terminal?")
            .body(format!(
                "{} will be removed from the terminal list.\nThis action cannot be undone.",
                selected
            ))
            .close_response("cancel")
            .default_response("cancel")
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");

        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.connect_response(
            Some("delete"),
            clone!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                selected,
                move |d, _| {
                    match this
                        .root_store()
                        .terminal_repository()
                        .delete_terminal(&selected)
                    {
                        Ok(_) => {
                            glib::MainContext::ref_thread_default().spawn_local(async move {
                                if let Some(terminal_combo_row) =
                                    this.imp().terminal_combo_row.borrow().as_ref()
                                {
                                    terminal_combo_row.reload_terminals();
                                    terminal_combo_row.set_selected_by_name(
                                        &this
                                            .root_store()
                                            .terminal_repository()
                                            .default_terminal()
                                            .await
                                            .map(|x| x.name)
                                            .unwrap_or_default(),
                                    );
                                }

                                this.add_toast(adw::Toast::new("Terminal removed successfully"));
                            });
                        }
                        Err(err) => {
                            error!(error = %err, "Failed to delete terminal");
                            this.add_toast(adw::Toast::new("Failed to delete terminal"));
                        }
                    }
                    d.close();
                }
            ),
        );

        dialog.present(Some(self));
    }

    fn show_add_terminal_dialog(&self) {
        let custom_dialog = adw::Dialog::new();
        custom_dialog.set_title("Add Custom Terminal");

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(&adw::HeaderBar::new());

        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);

        let group = adw::PreferencesGroup::new();

        // Name entry
        let name_entry = adw::EntryRow::builder().title("Terminal Name").build();

        // Program entry
        let program_entry = adw::EntryRow::builder().title("Program Path").build();

        // Separator argument entry
        let separator_entry = adw::EntryRow::builder().title("Separator Argument").build();

        group.add(&name_entry);
        group.add(&program_entry);
        group.add(&separator_entry);
        content.append(&group);

        // Add note about separator
        let info_label = gtk::Label::new(Some(
            "The separator argument is used to pass commands to the terminal.\n\
            Examples: '--' for GNOME Terminal, '-e' for xterm",
        ));
        info_label.add_css_class("caption");
        info_label.add_css_class("dim-label");
        info_label.set_wrap(true);
        info_label.set_xalign(0.0);
        info_label.set_margin_start(12);
        content.append(&info_label);

        // Buttons
        let button_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        button_box.set_margin_top(12);
        button_box.set_homogeneous(true);

        let cancel_btn = gtk::Button::with_label("Cancel");
        cancel_btn.add_css_class("pill");

        let save_btn = gtk::Button::with_label("Save");
        save_btn.add_css_class("suggested-action");
        save_btn.add_css_class("pill");

        button_box.append(&cancel_btn);
        button_box.append(&save_btn);
        content.append(&button_box);

        toolbar_view.set_content(Some(&content));
        custom_dialog.set_child(Some(&toolbar_view));

        // Connect button handlers
        cancel_btn.connect_clicked(clone!(
            #[weak]
            custom_dialog,
            move |_| {
                custom_dialog.close();
            }
        ));

        save_btn.connect_clicked(clone!(
            #[weak]
            custom_dialog,
            #[weak]
            name_entry,
            #[weak]
            program_entry,
            #[weak]
            separator_entry,
            #[weak(rename_to = this)]
            self,
            move |_| {
                let name = name_entry.text().to_string();
                let program = program_entry.text().to_string();
                let separator_arg = separator_entry.text().to_string();

                // Validate inputs
                if name.is_empty() || program.is_empty() || separator_arg.is_empty() {
                    this.add_toast(adw::Toast::new("All fields are required"));
                    return;
                }

                // Create and save the terminal
                let terminal = supported_terminals::Terminal {
                    name,
                    program,
                    separator_arg,
                    read_only: false,
                };

                match this
                    .root_store()
                    .terminal_repository()
                    .save_terminal(terminal.clone())
                {
                    Ok(_) => {
                        // Show success toast
                        let toast = adw::Toast::new("Custom terminal added successfully");

                        if let Some(terminal_combo_row) =
                            this.imp().terminal_combo_row.borrow().as_ref()
                        {
                            terminal_combo_row.reload_terminals();
                            terminal_combo_row.set_selected_by_name(&terminal.name);
                        }

                        this.add_toast(toast);
                        custom_dialog.close();
                    }
                    Err(err) => {
                        error!(error = %err, "Failed to save terminal");
                        this.add_toast(adw::Toast::new("Failed to save terminal"));
                    }
                }
            }
        ));

        custom_dialog.present(Some(self));
    }
}
