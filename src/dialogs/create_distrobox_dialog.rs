use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::gio::File;
use gtk::{gio, glib};
use tracing::error;

use crate::distrobox::{self, CreateArgName, CreateArgs, Error};
use crate::root_store::RootStore;

use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};

use crate::distro_combo_row_item;
use glib::clone;
use gtk::glib::{derived_properties, Properties};

pub enum FileRowSelection {
    File,
    Folder,
}
mod imp {
    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type=super::CreateDistroboxDialog)]
    pub struct CreateDistroboxDialog {
        #[property(get, set)]
        pub root_store: RefCell<RootStore>,
        pub dialog: adw::Dialog,
        pub toolbar_view: adw::ToolbarView,
        pub content: gtk::Box,
        pub name_row: adw::EntryRow,
        pub image_row: adw::ComboRow,
        pub home_row_expander: adw::ExpanderRow,
        #[property(get, set, nullable)]
        pub home_folder: RefCell<Option<String>>,
        #[property(get, set, nullable)]
        pub assemble_file: RefCell<Option<String>>,
        #[property(get, set, nullable)]
        pub assemble_url: RefCell<Option<String>>,
        pub nvidia_row: adw::SwitchRow,
        pub init_row: adw::SwitchRow,
        pub volume_rows: Rc<RefCell<Vec<adw::EntryRow>>>,
        pub scrolled_window: gtk::ScrolledWindow,
    }

    #[derived_properties]
    impl ObjectImpl for CreateDistroboxDialog {
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

            let obj = self.obj().clone();
            let home_row = self.obj().build_file_row(
                "Select Home Directory",
                FileRowSelection::Folder,
                move |path| {
                    obj.set_home_folder(Some(path.display().to_string()));
                },
            );
            self.home_row_expander.set_title("Custom Home Directory");
            self.home_row_expander.set_show_enable_switch(true);
            self.home_row_expander.set_enable_expansion(false);
            self.home_row_expander.add_row(&home_row);
            let obj = self.obj().clone();
            self.home_row_expander
                .connect_enable_expansion_notify(clone!(
                    #[weak]
                    home_row,
                    move |expander| {
                        if !expander.enables_expansion() {
                            obj.set_home_folder(None::<&str>);
                        }
                        home_row.set_subtitle(obj.home_folder().as_deref().unwrap_or(""));
                    }
                ));

            self.nvidia_row.set_title("NVIDIA Support");

            self.init_row.set_title("Init process");

            preferences_group.add(&self.name_row);
            preferences_group.add(&self.image_row);
            preferences_group.add(&self.home_row_expander);
            preferences_group.add(&self.nvidia_row);
            preferences_group.add(&self.init_row);

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
                    glib::MainContext::ref_thread_default().spawn_local(async move {
                        let res = obj.extract_create_args().await;
                        obj.update_errors(&res);
                        if let Ok(create_args) = res {
                            obj.root_store().create_container(create_args);
                        }
                    });
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

            let obj = self.obj().clone();
            let file_row = self.obj().build_file_row(
                "Select Assemble File",
                FileRowSelection::File,
                move |path| {
                    obj.set_assemble_file(Some(path.display().to_string()));
                },
            );
            assemble_group.add(&file_row);
            assemble_page.append(&assemble_group);

            // Add create button for assemble file
            let create_btn = gtk::Button::with_label("Create");
            create_btn.set_halign(gtk::Align::Center);
            create_btn.add_css_class("suggested-action");
            create_btn.add_css_class("pill");
            create_btn.set_margin_top(12);
            create_btn.set_sensitive(false);
            assemble_page.append(&create_btn);

            // Handle create click
            let obj = self.obj();
            create_btn.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    if let Some(path) = obj.assemble_file() {
                        obj.root_store().assemble_container(path.as_ref());
                        obj.close();
                    }
                }
            ));

            // Enable button when file is selected
            self.obj().connect_assemble_file_notify(move |obj| {
                create_btn.set_sensitive(obj.assemble_file().is_some());
            });

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

            url_group.add(&url_row);
            url_page.append(&url_group);

            // Add create button for URL
            let create_btn = gtk::Button::with_label("Create");
            create_btn.set_halign(gtk::Align::Center);
            create_btn.add_css_class("suggested-action");
            create_btn.add_css_class("pill");
            create_btn.set_margin_top(12);
            create_btn.set_sensitive(false);
            url_page.append(&create_btn);

            // Enable button when URL is entered
            url_row.connect_changed(clone!(
                #[weak]
                obj,
                move |entry| {
                    obj.set_assemble_url(Some(entry.text()));
                }
            ));

            // Handle create click
            let obj = self.obj();
            create_btn.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    obj.root_store()
                        .assemble_container(&obj.assemble_url().as_ref().unwrap());
                    obj.close();
                }
            ));

            obj.connect_assemble_url_notify(move |obj| {
                create_btn.set_sensitive(obj.assemble_url().is_some());
            });

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

            // Wrap content_box in a scrolled window
            let scrolled_window = gtk::ScrolledWindow::new();
            scrolled_window.set_propagate_natural_height(true);
            scrolled_window.set_child(Some(&content_box));

            toolbar_view.add_top_bar(&header);
            toolbar_view.set_vexpand(true);
            toolbar_view.set_content(Some(&scrolled_window));

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
    pub fn new(root_store: RootStore) -> Self {
        let this: Self = glib::Object::builder()
            .property("root-store", root_store)
            .build();

        this.root_store()
            .images_query()
            .connect_success(clone!(
            #[weak]
            this,
            move |images| {
                let string_list = gtk::StringList::new(&[]);
                for image in images {
                    string_list.append(&image);
                }
                this.imp().image_row.set_model(Some(&string_list));
            }
        ));
        this.root_store().images_query().refetch();

        glib::MainContext::ref_thread_default().spawn_local(clone!(
            #[weak]
            this,
            async move {
                let is_nvidia = this.root_store().is_nvidia_host().await;
                this.imp().nvidia_row.set_active(is_nvidia);
            }
        ));

        this
    }

    pub fn build_file_row(
        &self,
        title: &str,
        selection: FileRowSelection,
        cb: impl Fn(PathBuf) + Clone + 'static,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle("No file selected");
        row.set_activatable(true);

        let file_icon = gtk::Image::from_icon_name("document-open-symbolic");
        row.add_suffix(&file_icon);

        let title = title.to_owned();
        let dialog_cb = clone!(
            #[weak(rename_to=this)]
            self,
            #[strong]
            title,
            #[weak]
            row,
            move |res: Result<File, _>| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        glib::MainContext::ref_thread_default().spawn_local(async move {
                            match this
                                .root_store()
                                .resolve_host_path(&path.display().to_string())
                                .await
                            {
                                Ok(resolved_path) => {
                                    row.set_subtitle(&resolved_path);
                                    cb(PathBuf::from(resolved_path));
                                }

                                Err(e) => {
                                    this.update_errors::<()>(&Err(Error::InvalidField(
                                        title.to_lowercase(),
                                        e.to_string(),
                                    )));
                                }
                            }
                        });
                    }
                }
            }
        );
        row.connect_activated(move |_| {
            let file_dialog = gtk::FileDialog::builder().title(&title).modal(true).build();
            let dialog_cb = dialog_cb.clone();
            match selection {
                FileRowSelection::File => {
                    file_dialog.open(None::<&gtk::Window>, None::<&gio::Cancellable>, dialog_cb);
                }
                FileRowSelection::Folder => {
                    file_dialog.select_folder(
                        None::<&gtk::Window>,
                        None::<&gio::Cancellable>,
                        dialog_cb,
                    );
                }
            }
        });
        row
    }

    pub async fn extract_create_args(&self) -> Result<CreateArgs, Error> {
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
                    match entry.text().parse::<distrobox::Volume>() {
                        Ok(volume) => Some(Ok(volume)),
                        Err(e) => Some(Err(e)),
                    }
                } else {
                    None
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        let name = CreateArgName::new(&imp.name_row.text())?;

        let create_args = CreateArgs {
            name,
            image: image.to_string(),
            nvidia: imp.nvidia_row.is_active(),
            home_path: self.home_folder(),
            init: imp.init_row.is_active(),
            volumes,
        };
        dbg!(&create_args);

        Ok(create_args)
    }

    pub fn build_volumes_group(&self) -> adw::PreferencesGroup {
        let volumes_group = adw::PreferencesGroup::new();
        volumes_group.set_title("Volumes");
        volumes_group.set_description(Some(
            "Specify volumes in the format 'host_path:container_path'",
        ));

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

    fn update_errors<T>(&self, res: &Result<T, distrobox::Error>) {
        let imp = self.imp();
        imp.name_row.remove_css_class("error");
        imp.name_row.set_tooltip_text(None);
        if let Err(e) = res {
            error!(error = %e, "CreateDistroboxDialog: update_errors");
        }
        match res {
            Err(distrobox::Error::InvalidField(field, msg)) if field == "name" => {
                imp.name_row.add_css_class("error");
                imp.name_row.set_tooltip_text(Some(msg));
            }
            _ => {}
        }
    }
}
