use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::SignalHandlerId;
use gtk::{gio, glib};

use crate::distrobox::{self, CreateArgName, CreateArgs, Error, ExportableApp};
use crate::distrobox_service::DistroboxService;
use crate::resource::{Resource, SharedResource};

use glib::subclass::Signal;
use std::{cell::RefCell, rc::Rc};

use glib::clone;
use std::sync::OnceLock;

mod imp {

    use std::cell::{LazyCell, OnceCell};

    use crate::distro_combo_row_item;

    use super::*;

    #[derive(Default)]
    pub struct CreateDistroboxDialog {
        pub dialog: adw::Dialog,
        pub toolbar_view: adw::ToolbarView,
        pub content: gtk::Box,
        pub name_row: adw::EntryRow,
        pub image_row: adw::ComboRow,
        pub home_row: adw::SwitchRow,
        pub nvidia_row: adw::SwitchRow,
        pub init_row: adw::SwitchRow,
        pub volume_rows: Rc<RefCell<Vec<adw::EntryRow>>>,
        pub scrolled_window: gtk::ScrolledWindow,
        pub shared_resource: SharedResource<Vec<distrobox::ExportableApp>, anyhow::Error>,
        pub distrobox_service: OnceCell<DistroboxService>,
        pub current_create_args: RefCell<CreateArgs>,
    }

    impl ObjectImpl for CreateDistroboxDialog {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("create-requested").build()])
        }
        fn constructed(&self) {
            self.obj().set_title("Create a Distrobox");
            self.obj().set_content_width(480);

            let toolbar_view = adw::ToolbarView::new();
            let header = adw::HeaderBar::new();
        
            // Create view switcher and stack
            let view_stack = adw::ViewStack::new();
            
            // Create GUI creation page
            let gui_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
            gui_page.set_margin_start(12);
            gui_page.set_margin_end(12);
            gui_page.set_margin_top(12);
            gui_page.set_margin_bottom(12);
            let preferences_group = adw::PreferencesGroup::new();
            preferences_group.set_title("Settings");

            self.name_row.set_title("Name");

            self.image_row
                .set_expression(Some(&gtk::PropertyExpression::new(
                    gtk::StringObject::static_type(),
                    None::<gtk::Expression>,
                    "string",
                )));
            let item_factory = gtk::SignalListItemFactory::new();
            item_factory.connect_setup(|_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                item.set_child(Some(&distro_combo_row_item::DistroComboRowItem::new()));
            });
            item_factory.connect_bind(|_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                let image = item
                    .item()
                    .and_downcast::<gtk::StringObject>()
                    .unwrap()
                    .string();
                let child = item.child();
                let child: &distro_combo_row_item::DistroComboRowItem =
                    child.and_downcast_ref().unwrap();
                child.set_image(&image);
            });
            self.image_row.set_factory(Some(&item_factory));
            self.image_row.set_enable_search(true);
            self.image_row
                .set_search_match_mode(gtk::StringFilterMatchMode::Substring);
            self.image_row.set_title("Base Image");
            self.image_row.set_use_subtitle(true);

            self.home_row.set_title("Mount Home Directory");

            let file_chooser_fun = clone!(move || {
                // let file_chooser = gtk::FileChooserDialog::new(
                //     Some("Select Home Directory"),
                //     Some(&obj),
                //     gtk::FileChooserAction::SelectFolder,
                //     &[
                //         ("Cancel", gtk::ResponseType::Cancel),
                //         ("Select", gtk::ResponseType::Accept),
                //     ],
                // );

                // file_chooser.connect_response(
                //     clone!(@weak home_row => move |file_chooser, response| {
                //     if response == gtk::ResponseType::Accept {
                //     if let Some(folder) = file_chooser.file() {
                //     home_row.set_subtitle(&folder.path().unwrap().display().to_string());
                //     }
                //     }
                //     file_chooser.close();
                //     }),
                // );

                // file_chooser.show();
            });

            let home_path_button = gtk::Button::from_icon_name("folder-symbolic");
            home_path_button.set_sensitive(false);
            home_path_button.set_valign(gtk::Align::Center);
            home_path_button.set_tooltip_text(Some("Select Home Directory"));
            home_path_button.add_css_class("flat");
            let file_chooser_fun_clone = file_chooser_fun.clone();
            self.home_row.connect_active_notify(clone!(
                #[weak]
                home_path_button,
                move |row| {
                    if row.is_active() {
                        file_chooser_fun_clone();
                    }
                    home_path_button.set_sensitive(row.is_active());
                }
            ));
            home_path_button.connect_clicked(move |_| file_chooser_fun());

            self.home_row.add_suffix(&home_path_button);

            let nvidia_row = adw::SwitchRow::new();
            nvidia_row.set_title("NVIDIA Support");

            let init_row = adw::SwitchRow::new();
            init_row.set_title("Init process");

            preferences_group.add(&self.name_row);
            preferences_group.add(&self.image_row);
            preferences_group.add(&self.home_row);
            preferences_group.add(&nvidia_row);
            preferences_group.add(&init_row);

            let volumes_group = self.obj().build_volumes_group();
            gui_page.append(&preferences_group);
            gui_page.append(&volumes_group);

            let create_btn = gtk::Button::with_label("Create");
            create_btn.set_halign(gtk::Align::Center);

            let obj = self.obj();
            create_btn.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    let res = obj.update_create_args();
                    obj.update_errors(&res);
                    match res {
                        Ok(()) => {
                            obj.emit_by_name_with_values("create-requested", &[]);
                        }
                        _ => {}
                    }
                }
            ));
            create_btn.add_css_class("suggested-action");
            create_btn.add_css_class("pill");
            create_btn.set_margin_top(12);

            gui_page.append(&create_btn);

            // Create page for assemble from file
            let assemble_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
            assemble_page.set_margin_start(12);
            assemble_page.set_margin_end(12);
            assemble_page.set_margin_top(12);
            assemble_page.set_margin_bottom(12);

            let assemble_group = adw::PreferencesGroup::new();
            assemble_group.set_title("Assemble from File");
            assemble_group.set_description(Some("Create a container from an assemble file"));

            let file_row = adw::ActionRow::new();
            file_row.set_title("Select File");
            file_row.set_subtitle("No file selected");
            file_row.set_activatable(true);

            let file_icon = gtk::Image::from_icon_name("document-open-symbolic");
            file_row.add_suffix(&file_icon);

            let obj = self.obj();
            file_row.connect_activated(clone!(
                #[weak]
                obj,
                #[weak]
                file_row,
                move |_| {
                    let file_dialog = gtk::FileDialog::builder()
                        .title("Select Assemble File")
                        .modal(true)
                        .build();

                    file_dialog.open(
                        None::<&gtk::Window>,
                        None::<&gio::Cancellable>,
                        clone!(
                            #[weak]
                            obj,
                            #[weak]
                            file_row,
                            move |res| {
                                if let Ok(file) = res {
                                    if let Some(path) = file.path() {
                                        file_row.set_subtitle(&path.display().to_string());
                                        
                                        let service = obj.imp().distrobox_service.get().unwrap().clone();
                                        let task = service.do_assemble(&path.to_string_lossy());
                                        let dialog = obj.clone();
                                        task.connect_status_notify(clone!(
                                            #[weak]
                                            dialog,
                                            move |task| {
                                                if task.status() == "successful" {
                                                    dialog.close();
                                                }
                                            }
                                        ));
                                    }
                                }
                            }
                        ),
                    );
                }
            ));

            assemble_group.add(&file_row);
            assemble_page.append(&assemble_group);

            // Add a status label
            let status_label = gtk::Label::new(None);
            status_label.set_wrap(true);
            status_label.set_xalign(0.0);
            status_label.set_margin_top(12);
            status_label.set_margin_start(12);
            status_label.set_margin_end(12);
            status_label.add_css_class("dim-label");
            status_label.set_text("Select an assemble file to create a container. The file should be in YAML format.");
            assemble_page.append(&status_label);

            // Add create button for assemble file
            let create_btn = gtk::Button::with_label("Create");
            create_btn.set_halign(gtk::Align::Center);
            create_btn.add_css_class("suggested-action");
            create_btn.add_css_class("pill");
            create_btn.set_margin_top(12);
            create_btn.set_sensitive(false);

            // Enable button when file is selected
            file_row.connect_notify(Some("subtitle"), clone!(
                #[weak]
                create_btn,
                move |row, _| {
                    create_btn.set_sensitive(row.subtitle() != "No file selected");
                }
            ));

            // Handle create click
            let obj = self.obj();
            create_btn.connect_clicked(clone!(
                #[weak]
                obj,
                #[weak]
                file_row,
                move |_| {
                    if let Some(path) = file_row.subtitle() {
                        let service = obj.imp().distrobox_service.get().unwrap().clone();
                        let task = service.do_assemble(&path.to_string());
                        let dialog = obj.clone();
                        task.connect_status_notify(clone!(
                            #[weak]
                            dialog,
                            move |task| {
                                if task.status() == "successful" {
                                    dialog.close();
                                }
                            }
                        ));
                    }
                }
            ));

            assemble_page.append(&create_btn);

            // Create page for URL creation
            let url_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
            url_page.set_margin_start(12);
            url_page.set_margin_end(12);
            url_page.set_margin_top(12);
            url_page.set_margin_bottom(12);

            let url_group = adw::PreferencesGroup::new();
            url_group.set_title("From URL");
            url_group.set_description(Some("Create a container from a remote URL"));

            let url_row = adw::EntryRow::new();
            url_row.set_title("URL");
            url_row.set_text("https://example.com/container.yaml");

            let status_label = gtk::Label::new(None);
            status_label.set_wrap(true);
            status_label.set_xalign(0.0);
            status_label.set_margin_top(12);
            status_label.set_margin_start(12);
            status_label.set_margin_end(12);
            status_label.add_css_class("dim-label");
            status_label.set_text("Enter the URL of a remote assemble file to create a container.");

            url_group.add(&url_row);
            url_page.append(&url_group);
            url_page.append(&status_label);

            // Add create button for URL
            let create_btn = gtk::Button::with_label("Create");
            create_btn.set_halign(gtk::Align::Center);
            create_btn.add_css_class("suggested-action");
            create_btn.add_css_class("pill");
            create_btn.set_margin_top(12);
            create_btn.set_sensitive(false);

            // Enable button when URL is entered
            url_row.connect_changed(clone!(
                #[weak]
                create_btn,
                move |entry| {
                    create_btn.set_sensitive(!entry.text().is_empty());
                }
            ));

            // Handle create click
            let obj = self.obj();
            create_btn.connect_clicked(clone!(
                #[weak]
                obj,
                #[weak]
                url_row,
                move |_| {
                    let url = url_row.text();
                    let service = obj.imp().distrobox_service.get().unwrap().clone();
                    let task = service.do_assemble(&url);
                    let dialog = obj.clone();
                    task.connect_status_notify(clone!(
                        #[weak]
                        dialog,
                        move |task| {
                            if task.status() == "successful" {
                                dialog.close();
                            }
                        }
                    ));
                }
            ));

            url_page.append(&create_btn);

            // Add pages to view stack
            view_stack.add_titled(&gui_page, Some("create"), "Guided");
            view_stack.add_titled(&assemble_page, Some("assemble-file"), "From File");
            view_stack.add_titled(&url_page, Some("assemble-url"), "From URL");

            // Create a box to hold the view switcher and content
            let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            
            // Add inline view switcher
            let view_switcher = adw::InlineViewSwitcher::new();
            view_switcher.set_stack(Some(&view_stack));
            view_switcher.set_margin_start(12);
            view_switcher.set_margin_end(12);
            view_switcher.set_margin_top(12);
            view_switcher.set_margin_bottom(12);
            
            content_box.append(&view_switcher);
            content_box.append(&view_stack);
            
            toolbar_view.add_top_bar(&header);
            toolbar_view.set_content(Some(&content_box));

            self.obj().set_child(Some(&toolbar_view));
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CreateDistroboxDialog {
        const NAME: &'static str = "CreateDistroboxDialog";
        type Type = super::CreateDistroboxDialog;
        type ParentType = adw::Dialog;
    }

    impl WidgetImpl for CreateDistroboxDialog {}
    impl AdwDialogImpl for CreateDistroboxDialog {}
}

glib::wrapper! {
    pub struct CreateDistroboxDialog(ObjectSubclass<imp::CreateDistroboxDialog>)
        @extends adw::Dialog, gtk::Widget;
}
impl CreateDistroboxDialog {
    pub fn new(distrobox_service: DistroboxService) -> Self {
        let this: Self = glib::Object::builder().build();

        distrobox_service.connect_images_changed(clone!(
            #[weak]
            this,
            move |service| {
                let string_list = gtk::StringList::new(&[]);
                if let Resource::Loaded(images) = service.images() {
                    for image in images {
                        string_list.append(&image);
                    }
                } else {
                    dbg!("Loading images...");
                }
                this.imp().image_row.set_model(Some(&string_list));
            }
        ));
        distrobox_service.load_images();

        this.imp().distrobox_service.set(distrobox_service).unwrap();
        this
    }

    pub fn update_create_args(&self) -> Result<(), Error> {
        let imp = self.imp();
        let image = imp
            .image_row
            .selected_item()
            .unwrap()
            .downcast_ref::<gtk::StringObject>()
            .unwrap()
            .string();
        let volumes = imp
            .volume_rows
            .borrow()
            .iter()
            .filter_map(|entry| {
                if !entry.text().is_empty() {
                    Some(entry.text().to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let name = CreateArgName::new(&imp.name_row.text())?;

        let create_args = CreateArgs {
            name,
            image: image.to_string(),
            nvidia: imp.nvidia_row.is_active(),
            home_path: imp.home_row.subtitle().map(|s| s.to_string()).unwrap(), // TODO: handle None,
            init: imp.init_row.is_active(),
            volumes,
        };

        self.imp().current_create_args.replace(create_args);
        Ok(())
    }

    pub fn build_volumes_group(&self) -> adw::PreferencesGroup {
        let volumes_group = adw::PreferencesGroup::new();
        volumes_group.set_title("Volumes");
        volumes_group.set_description(Some("Specify volumes in the format 'dest_dir:source_dir'"));

        let add_volume_button = adw::ButtonRow::builder().title("Add Volume").build();
        add_volume_button.connect_activated(clone!(
            #[weak(rename_to=this)]
            self,
            #[weak]
            volumes_group,
            move |_| {
                let volume_row = adw::EntryRow::new();
                volume_row.set_title("Volume");

                let remove_button = gtk::Button::from_icon_name("user-trash-symbolic");
                remove_button.set_tooltip_text(Some("Remove Volume"));
                remove_button.add_css_class("flat");
                remove_button.set_valign(gtk::Align::Center);
                remove_button.add_css_class("destructive-action");
                remove_button.connect_clicked(clone!(
                    #[weak]
                    this,
                    #[weak]
                    volumes_group,
                    #[weak]
                    volume_row,
                    move |_| {
                        this.imp()
                            .volume_rows
                            .borrow_mut()
                            .retain(|row| row != &volume_row);
                        volumes_group.remove(&volume_row);
                    }
                ));
                volume_row.add_suffix(&remove_button);

                this.imp().volume_rows.borrow_mut().push(volume_row.clone());
                volumes_group.add(&volume_row);
            }
        ));

        volumes_group.add(&add_volume_button);

        volumes_group
    }

    fn update_errors(&self, res: &Result<(), distrobox::Error>) {
        let imp = self.imp();
        imp.name_row.remove_css_class("error");
        imp.name_row.set_tooltip_text(None);
        match res {
            Err(distrobox::Error::InvalidField(field, msg)) if field == "name" => {
                imp.name_row.add_css_class("error");
                imp.name_row.set_tooltip_text(Some(&msg));
            }
            _ => {}
        }
    }

    pub fn connect_create_requested<F: Fn(&Self, CreateArgs) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_local("create-requested", false, move |args| {
            let obj = args[0].get::<Self>().unwrap();
            f(&obj, obj.imp().current_create_args.borrow().clone());
            None
        })
    }
}
