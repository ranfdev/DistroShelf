use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::clone;
use gtk::{gio, glib};

use crate::distrobox::{self, ExportableApp};
use crate::distrobox_service::DistroboxService;
use crate::resource::{Resource, SharedResource};

use std::cell::{OnceCell, RefCell};

use glib::VariantTy;
use im_rc::Vector;

mod imp {

    use super::*;

    #[derive(Default)]
    pub struct ExportableAppsDialog {
        pub dialog: adw::Dialog,
        pub toolbar_view: adw::ToolbarView,
        pub content: gtk::Box,
        pub scrolled_window: gtk::ScrolledWindow,
        pub apps: SharedResource<Vector<distrobox::ExportableApp>, anyhow::Error>,
        pub distrobox_service: OnceCell<DistroboxService>,
        pub container: RefCell<String>,
    }

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

            let obj = self.obj().clone();
            self.apps
                .set_callback(
                    move |res: Resource<Vector<ExportableApp>, anyhow::Error>| match res {
                        Resource::Error(err, _) => {
                            obj.imp().scrolled_window.set_child(Some(
                                &gtk::Label::builder()
                                    .label(format!("Error: {}", err))
                                    .wrap(true)
                                    .build(),
                            ));
                        }
                        Resource::Loading(_) => {
                            obj.imp()
                                .scrolled_window
                                .set_child(Some(&obj.handle_ui_loading()));
                        }
                        Resource::Loaded(res) => {
                            obj.imp()
                                .scrolled_window
                                .set_child(Some(&obj.handle_ui_loaded(&res)));
                        }
                        Resource::Unitialized => {}
                    },
                );

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
                    if let Resource::Loaded(apps) = this.imp().apps.resource() {
                        let app_found = apps.iter().find(|app| app.desktop_file_path == file_path);
                        let container = this.imp().container.borrow();
                        this.imp()
                            .distrobox_service
                            .get()
                            .unwrap()
                            .do_export(&container, app_found.unwrap().clone());

                        this.load_apps();
                    }
                },
            );
            klass.install_action(
                "dialog.unexport-app",
                Some(VariantTy::STRING),
                |this, _action, target| {
                    let file_path = target.unwrap().str().unwrap();
                    if let Resource::Loaded(apps) = this.imp().apps.resource() {
                        let app_found = apps.iter().find(|app| app.desktop_file_path == file_path);
                        let container = this.imp().container.borrow();
                        this.imp()
                            .distrobox_service
                            .get()
                            .unwrap()
                            .do_unexport(&container, app_found.unwrap().clone());

                        this.load_apps();
                    }
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
    pub fn new(container: &str, distrobox_service: DistroboxService) -> Self {
        let this: Self = glib::Object::builder().build();
        let container = container.to_string();
        this.imp().container.replace(container.to_string());
        this.imp()
            .distrobox_service
            .set(distrobox_service.clone())
            .unwrap();

        this.load_apps();
        this
    }
    pub fn load_apps(&self) {
        let container = self.imp().container.borrow().clone();
        let this = self.clone();
        let future = {
            async move {
                this.imp()
                    .distrobox_service
                    .get()
                    .unwrap()
                    .list_apps(&container)
                    .await
                    .map_err(|e| e.into())
            }
        };

        self.imp().apps.load(future);
    }
    pub fn handle_ui_loading(&self) -> impl IsA<gtk::Widget> {
        // TODO: replace with new libadwaita spinner when available
        let status_page = adw::StatusPage::new();
        status_page.set_title("Loading App List");
        status_page.set_description(Some(
            "Please wait while we load the list of exportable apps. This may take some time if the distrobox wasn't running",
        ));
        status_page
    }
    pub fn handle_ui_loaded(&self, apps: &Vector<ExportableApp>) -> impl IsA<gtk::Widget> {
        let export_apps_group = adw::PreferencesGroup::new();
        export_apps_group.set_margin_start(12);
        export_apps_group.set_margin_end(12);
        export_apps_group.set_margin_top(12);
        export_apps_group.set_margin_bottom(12);
        export_apps_group.set_title("Exportable Apps");
        let container = self.imp().container.borrow().clone();
        for app in apps {
            export_apps_group.add(&self.build_row(&container, &app));
        }
        export_apps_group
    }
    pub fn build_row(&self, container: &str, app: &ExportableApp) -> adw::ActionRow {
        // Create the action row
        let row = adw::ActionRow::new();
        row.set_title(&app.entry.name);
        row.set_subtitle(&app.desktop_file_path);
        row.set_activatable(true);

        let container = container.to_string();
        row.connect_activated(clone!(
            #[weak(rename_to=this)]
            self,
            #[strong]
            app,
            move |_| {
                this.imp()
                    .distrobox_service
                    .get()
                    .unwrap()
                    .do_launch(&container, app.clone());
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
                Some("Export App"),
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
