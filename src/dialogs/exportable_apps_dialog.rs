use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::{clone, BoxedAnyObject};
use gtk::{gio, glib, pango};

use crate::container::Container;
use crate::distrobox::{ExportableApp, ExportableBinary};
use crate::gtk_utils::reaction;
use crate::fakers::Command;

use std::cell::RefCell;

use glib::VariantTy;
use gtk::glib::{derived_properties, Properties};

mod imp {
    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type=super::ExportableAppsDialog)]
    pub struct ExportableAppsDialog {
        #[property(get, set)]
        pub container: RefCell<Container>,
        pub dialog: adw::Dialog,
        pub toast_overlay: adw::ToastOverlay,
        pub toolbar_view: adw::ToolbarView,
        pub content: gtk::Box,
        pub scrolled_window: gtk::ScrolledWindow,
        pub stack: gtk::Stack,
        pub error_label: gtk::Label,
        pub list_box: gtk::ListBox,
        pub binaries_list_box: gtk::ListBox,
        pub binary_name_entry: adw::EntryRow,
        pub export_apps_group: adw::PreferencesGroup,
        pub export_binaries_group: adw::PreferencesGroup,
    }

    #[derived_properties]
    impl ObjectImpl for ExportableAppsDialog {
        fn constructed(&self) {
            let obj = self.obj();
            obj.set_title("Manage Exports");
            obj.set_content_width(360);
            obj.set_content_height(640);

            self.toolbar_view.add_top_bar(&adw::HeaderBar::new());

            self.content.set_orientation(gtk::Orientation::Vertical);
            self.content.set_spacing(6);

            self.scrolled_window.set_vexpand(true);
            self.scrolled_window.set_propagate_natural_height(true);
            self.scrolled_window.set_child(Some(&self.stack));

            self.stack
                .set_transition_type(gtk::StackTransitionType::Crossfade);
            self.error_label.set_wrap_mode(pango::WrapMode::WordChar);
            self.error_label.set_wrap(true);
            self.stack.add_named(&self.error_label, Some("error"));

            let loading_page = adw::StatusPage::new();
            loading_page.set_title("Loading Exports");
            loading_page.set_description(Some(
                "Please wait while we load the list of exportable apps and binaries. This may take some time if the distrobox wasn't running",
            ));
            loading_page.set_child(Some(&adw::Spinner::new()));
            self.stack.add_named(&loading_page, Some("loading"));

            self.list_box.add_css_class("boxed-list");
            self.list_box.set_selection_mode(gtk::SelectionMode::None);
            self.export_apps_group.set_margin_start(12);
            self.export_apps_group.set_margin_end(12);
            self.export_apps_group.set_margin_top(12);
            self.export_apps_group.set_margin_bottom(12);
            self.export_apps_group.set_title("Exportable Apps");
            self.export_apps_group.set_description(Some("No exportable apps found"));
            self.export_apps_group.add(&self.list_box);

            // Setup binary export input
            self.binary_name_entry.set_title("Export New Binary");
            self.binary_name_entry.set_show_apply_button(true);
            self.binary_name_entry
                .add_css_class("add-binary-entry-row");

            self.binaries_list_box.add_css_class("boxed-list");
            self.binaries_list_box.set_selection_mode(gtk::SelectionMode::None);
            self.binaries_list_box.set_margin_top(12);
            
            self.export_binaries_group.set_margin_start(12);
            self.export_binaries_group.set_margin_end(12);
            self.export_binaries_group.set_margin_top(0);
            self.export_binaries_group.set_margin_bottom(12);
            self.export_binaries_group.set_title("Exported Binaries");
            self.export_binaries_group.set_description(Some("No exported binaries"));
            self.export_binaries_group.add(&self.binary_name_entry);
            self.export_binaries_group.add(&self.binaries_list_box);

            let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            content_box.append(&self.export_apps_group);
            content_box.append(&self.export_binaries_group);
            self.stack.add_named(&content_box, Some("apps"));

            self.content.append(&self.scrolled_window);
            self.toolbar_view.set_content(Some(&self.content));
            self.toast_overlay.set_child(Some(&self.toolbar_view));
            self.obj().set_child(Some(&self.toast_overlay));
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ExportableAppsDialog {
        const NAME: &'static str = "ExportableAppsDialog";
        type Type = super::ExportableAppsDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.install_action(
                "dialog.export-app",
                Some(VariantTy::STRING),
                |this, _action, target| {
                    let file_path = target.unwrap().str().unwrap();
                    this.container().export(file_path);
                },
            );
            klass.install_action(
                "dialog.unexport-app",
                Some(VariantTy::STRING),
                |this, _action, target| {
                    let file_path = target.unwrap().str().unwrap();
                    this.container().unexport(file_path);
                },
            );
            klass.install_action(
                "dialog.export-binary",
                Some(VariantTy::STRING),
                |this, _action, target| {
                    let binary_path = target.unwrap().str().unwrap();
                    this.container().export_binary(binary_path);
                },
            );
            klass.install_action(
                "dialog.unexport-binary",
                Some(VariantTy::STRING),
                |this, _action, target| {
                    let binary_path = target.unwrap().str().unwrap();
                    this.container().unexport_binary(binary_path);
                },
            );
        }
    }

    impl WidgetImpl for ExportableAppsDialog {}
    impl AdwDialogImpl for ExportableAppsDialog {}
}

glib::wrapper! {
    pub struct ExportableAppsDialog(ObjectSubclass<imp::ExportableAppsDialog>)
        @extends adw::Dialog, gtk::Widget;
}
impl ExportableAppsDialog {
    /// Check if a binary exists on the host system
    /// Handles both binary names (e.g., "nvim") and paths (e.g., "/usr/bin/nvim")
    async fn binary_exists_on_host(container: &Container, binary_name_or_path: &str) -> bool {
        // If it contains a '/', treat it as a path
        if binary_name_or_path.contains('/') {
            // For paths, we need to check on the host using a command
            // We'll use 'test -e' which returns 0 if the file exists
            let expanded_path = if binary_name_or_path.starts_with("~/") {
                // Expand home on host - use $HOME variable
                format!("$HOME/{}", &binary_name_or_path[2..])
            } else {
                binary_name_or_path.to_string()
            };
            
            let mut cmd = Command::new("test");
            cmd.args(["-e", &expanded_path]);
            
            let command_runner = container.root_store().command_runner();
            if let Ok(output) = command_runner.output(cmd).await {
                output.status.success()
            } else {
                false
            }
        } else {
            // It's a binary name, check using 'which' on the host
            let mut cmd = Command::new("which");
            cmd.arg(binary_name_or_path);
            
            let command_runner = container.root_store().command_runner();
            if let Ok(output) = command_runner.output(cmd).await {
                output.status.success()
            } else {
                false
            }
        }
    }
    
    pub fn new(container: &Container) -> Self {
        let this: Self = glib::Object::builder()
            .property("container", container)
            .build();

        let this_clone = this.clone();
        let apps = this.container().apps();
        let binaries = this.container().binaries();
        reaction! {
            (apps.error(), binaries.error()),
            move |(e1, e2): (Option<String>, Option<String>)| {
                if let Some(err) = e1.or(e2) {
                    this_clone.imp().error_label.set_label(&err);
                    this_clone.imp().stack.set_visible_child_name("error");
                }
            }
        };
        
        let this_clone = this.clone();
        let apps = this.container().apps();
        let render_apps = move || {
            let apps = apps.data::<gio::ListStore>();
            let n_apps = apps.as_ref().map(|s| s.n_items()).unwrap_or(0);

            // Update description based on whether there are apps
            if n_apps == 0 {
                this_clone.imp().export_apps_group.set_description(Some("No exportable apps found"));
            } else {
                this_clone.imp().export_apps_group.set_description(None);
            }

            this_clone.imp().stack.set_visible_child_name("apps");
            let this = this_clone.clone();
            this_clone
                .imp()
                .list_box
                .bind_model(apps.as_ref(), move |obj| {
                    let app = obj
                        .downcast_ref::<BoxedAnyObject>()
                        .map(|obj| obj.borrow::<ExportableApp>())
                        .unwrap();
                    this.build_row(&app).upcast()
                });
                
        };
        
        let this_clone = this.clone();
        let binaries = this.container().binaries();
        let render_binaries = move || {
            let binaries = binaries.data::<gio::ListStore>();
            let n_binaries = binaries.as_ref().map(|s| s.n_items()).unwrap_or(0);

            // Update description based on whether there are binaries
            if n_binaries == 0 {
                this_clone.imp().export_binaries_group.set_description(Some("No exported binaries"));
            } else {
                this_clone.imp().export_binaries_group.set_description(None);
            }

            this_clone.imp().stack.set_visible_child_name("apps");
            let this = this_clone.clone();
            this_clone
                .imp()
                .binaries_list_box
                .bind_model(binaries.as_ref(), move |obj| {
                    let binary = obj
                        .downcast_ref::<BoxedAnyObject>()
                        .map(|obj| obj.borrow::<ExportableBinary>())
                        .unwrap();
                    this.build_binary_row(&binary).upcast()
                });
        };

        let this_clone = this.clone();
        let apps = this.container().apps();
        let binaries = this.container().binaries();
        reaction! {
            (apps.loading(), binaries.loading()),
            move |(b1, b2): (bool, bool)| {
                if b1 || b2 {
                    this_clone.imp().stack.set_visible_child_name("loading");
                } else {
                    render_apps();
                    render_binaries();
                }
            }
        };

        
        // Connect the binary name entry apply signal
        let this_clone = this.clone();
        this.imp()
            .binary_name_entry
            .connect_apply(move |entry| {
                let binary_name = entry.text().to_string();
                if !binary_name.is_empty() {
                    let this = this_clone.clone();
                    let binary_name_clone = binary_name.clone();
                    let container = this_clone.container();
                    
                    // Check if binary exists on host and show confirmation dialog if needed
                    glib::spawn_future_local(async move {
                        let exists = Self::binary_exists_on_host(&container, &binary_name).await;
                        
                        if exists {
                            // Show confirmation dialog
                            let dialog = adw::AlertDialog::new(
                                Some("Binary Already Exists on Host"),
                                Some(&format!(
                                    "The binary '{}' already exists on your host system.\n\nDo you want to continue?",
                                    binary_name_clone
                                )),
                            );
                            dialog.add_response("cancel", "Cancel");
                            dialog.add_response("export", "Export Anyway");
                            dialog.set_response_appearance("export", adw::ResponseAppearance::Destructive);
                            dialog.set_default_response(Some("cancel"));
                            dialog.set_close_response("cancel");
                            
                            let this_inner = this.clone();
                            let binary_name_inner = binary_name_clone.clone();
                            dialog.connect_response(None, move |_dialog, response| {
                                if response == "export" {
                                    this_inner.do_export_binary(&binary_name_inner);
                                }
                            });
                            
                            dialog.present(Some(&this));
                        } else {
                            // No conflict, export directly
                            this.do_export_binary(&binary_name_clone);
                        }
                    });
                    
                    entry.set_text("");
                }
            });

        container.apps().reload();
        container.binaries().reload();

        this
    }
    
    /// Helper method to perform the actual export of a binary
    fn do_export_binary(&self, binary_name: &str) {
        let task = self.container().export_binary(binary_name);
        
        // Monitor task status to show error toasts
        let this = self.clone();
        let binary_name_clone = binary_name.to_string();
        reaction!(task.status(), move |status: String| {
            match status.as_str() {
                "failed" => {
                    let error_ref = task.error();
                    let error_msg = if let Some(err) = error_ref.as_ref() {
                        format!("Failed to export '{}': {}", binary_name_clone, err)
                    } else {
                        format!("Failed to export '{}'", binary_name_clone)
                    };
                    let toast = adw::Toast::new(&error_msg);
                    toast.set_timeout(5);
                    this.imp().toast_overlay.add_toast(toast);
                }
                _ => {}
            }
        });
    }
    
    pub fn build_row(&self, app: &ExportableApp) -> adw::ActionRow {
        // Create the action row
        let row = adw::ActionRow::new();
        row.set_title(&app.entry.name);
        row.set_subtitle(&app.desktop_file_path);
        row.set_activatable(true);

        row.connect_activated(clone!(
            #[weak(rename_to=this)]
            self,
            #[strong]
            app,
            move |_| {
                this.container().launch(app.clone());
            }
        ));

        // Create the menu button
        let menu_button = gtk::MenuButton::new();
        menu_button.set_icon_name("view-more-symbolic");
        menu_button.set_valign(gtk::Align::Center);
        menu_button.add_css_class("flat");

        // Create the menu model
        let menu_model = gio::Menu::new();
        if !app.exported {
            let export_action = gio::MenuItem::new(
                Some("Export App"),
                Some(&format!("dialog.export-app(\"{}\")", app.desktop_file_path)),
            );
            menu_model.append_item(&export_action);
        } else {
            let unexport_action = gio::MenuItem::new(
                Some("Unexport App"),
                Some(&format!(
                    "dialog.unexport-app(\"{}\")",
                    app.desktop_file_path
                )),
            );
            menu_model.append_item(&unexport_action);
        }

        // Set up the popover menu
        let popover = gtk::PopoverMenu::from_model(Some(&menu_model));
        menu_button.set_popover(Some(&popover));

        // Add the menu button to the action row
        row.add_suffix(&menu_button);

        row
    }
    
    pub fn build_binary_row(&self, binary: &ExportableBinary) -> adw::ActionRow {
        // Create the action row
        let row = adw::ActionRow::new();
        row.set_title(&binary.name);
        row.set_subtitle(&binary.source_path);

        // Create the menu button
        let menu_button = gtk::MenuButton::new();
        menu_button.set_icon_name("view-more-symbolic");
        menu_button.set_valign(gtk::Align::Center);
        menu_button.add_css_class("flat");

        // Create the menu model - only show unexport since we're only showing exported binaries
        let menu_model = gio::Menu::new();
        let unexport_action = gio::MenuItem::new(
            Some("Unexport Binary"),
            Some(&format!(
                "dialog.unexport-binary(\"{}\")",
                binary.source_path
            )),
        );
        menu_model.append_item(&unexport_action);

        // Set up the popover menu
        let popover = gtk::PopoverMenu::from_model(Some(&menu_model));
        menu_button.set_popover(Some(&popover));

        // Add the menu button to the action row
        row.add_suffix(&menu_button);

        row
    }
}
