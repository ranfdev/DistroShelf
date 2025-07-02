use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::{clone, BoxedAnyObject};
use gtk::{gio, glib};

use crate::container::Container;
use crate::distrobox::ExportableApp;

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
        pub toolbar_view: adw::ToolbarView,
        pub content: gtk::Box,
        pub scrolled_window: gtk::ScrolledWindow,
        pub stack: gtk::Stack,
        pub error_label: gtk::Label,
        pub list_box: gtk::ListBox,
    }

    #[derived_properties]
    impl ObjectImpl for ExportableAppsDialog {
        fn constructed(&self) {
            let obj = self.obj();
            obj.set_title("Exportable Apps");
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
            self.stack.add_named(&self.error_label, Some("error"));

            let loading_page = adw::StatusPage::new();
            loading_page.set_title("Loading App List");
            loading_page.set_description(Some(
                "Please wait while we load the list of exportable apps. This may take some time if the distrobox wasn't running",
            ));
            loading_page.set_child(Some(&adw::Spinner::new()));
            self.stack.add_named(&loading_page, Some("loading"));

            self.list_box.add_css_class("boxed-list");
            self.list_box.set_selection_mode(gtk::SelectionMode::None);
            let export_apps_group = adw::PreferencesGroup::new();
            export_apps_group.set_margin_start(12);
            export_apps_group.set_margin_end(12);
            export_apps_group.set_margin_top(12);
            export_apps_group.set_margin_bottom(12);
            export_apps_group.set_title("Exportable Apps");
            export_apps_group.add(&self.list_box);
            self.stack.add_named(&export_apps_group, Some("apps"));

            let empty_page = adw::StatusPage::new();
            empty_page.set_title("No Exportable Apps");

            self.stack.add_named(&empty_page, Some("empty"));

            self.content.append(&self.scrolled_window);
            self.toolbar_view.set_content(Some(&self.content));
            self.obj().set_child(Some(&self.toolbar_view));
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
    pub fn new(container: &Container) -> Self {
        let this: Self = glib::Object::builder()
            .property("container", container)
            .build();

        let this_clone = this.clone();
        container.apps().connect_loading_notify(move |resource| {
            if resource.loading() {
                this_clone.imp().stack.set_visible_child_name("loading");
            }
        });
        let this_clone = this.clone();
        container.apps().connect_error_notify(move |resource| {
            if let Some(err) = resource.error() {
                this_clone.imp().error_label.set_label(&err);
                this_clone.imp().stack.set_visible_child_name("error");
            }
        });
        let this_clone = this.clone();
        container.apps().connect_data_changed(move |resource| {
            let apps = resource.data::<gio::ListStore>().unwrap();

            if apps.n_items() == 0 {
                this_clone.imp().stack.set_visible_child_name("empty");
                return;
            }

            this_clone.imp().stack.set_visible_child_name("apps");

            let this = this_clone.clone();
            this_clone
                .imp()
                .list_box
                .bind_model(Some(&apps), move |obj| {
                    let app = obj
                        .downcast_ref::<BoxedAnyObject>()
                        .map(|obj| obj.borrow::<ExportableApp>())
                        .unwrap();
                    this.build_row(&app).upcast()
                });
        });
        container.apps().reload();

        this
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
}
