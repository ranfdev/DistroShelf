use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::gio::File;
use gtk::{gio, glib};
use tracing::error;

use crate::backends::{self, CreateArgName, CreateArgs, Error};
use crate::container::Container;
use crate::root_store::RootStore;
use crate::sidebar_row::SidebarRow;

use std::collections::HashSet;
use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};

use crate::{distro_icon, image_row_item};
use glib::clone;
use gtk::glib::{Properties, derived_properties};

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
        pub navigation_view: adw::NavigationView,
        pub toolbar_view: adw::ToolbarView,
        pub content: gtk::Box,
        pub name_row: adw::EntryRow,
        pub image_row: adw::ActionRow,
        pub images_model: gtk::StringList,
        pub selected_image: RefCell<String>,
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
        #[property(get, set=Self::set_clone_src, nullable)]
        pub clone_src: RefCell<Option<Container>>,
        // transient widget used to show the source container info when cloning
        pub clone_sidebar: RefCell<Option<SidebarRow>>,
        pub cloning_content: gtk::Box,
        pub view_switcher: adw::InlineViewSwitcher,
        pub clone_warning_banner: adw::Banner,
        pub downloaded_tags: RefCell<HashSet<String>>,
    }

    impl CreateDistroboxDialog {
        fn set_clone_src(&self, value: Option<Container>) {
            // store the value
            self.clone_src.replace(value.clone());

            if let Some(sidebar_row) = self.clone_sidebar.borrow_mut().take() {
                self.cloning_content.remove(&sidebar_row);
            }

            if let Some(container) = value {
                self.image_row.set_visible(false);
                self.cloning_content.set_visible(true);
                self.view_switcher.set_visible(false);
                let sidebar_row = SidebarRow::new(&container);
                // insert at the top of the cloning_content box
                self.cloning_content.append(&sidebar_row);
                self.clone_sidebar.replace(Some(sidebar_row));

                // Show warning if container is running
                if container.is_running() {
                    self.clone_warning_banner.set_revealed(true);
                } else {
                    self.clone_warning_banner.set_revealed(false);
                }
            } else {
                // no clone source, ensure image row is visible
                self.image_row.set_visible(true);
                self.cloning_content.set_visible(false);
                self.view_switcher.set_visible(true);
                self.clone_warning_banner.set_revealed(false);
            }
        }
    }

    #[derived_properties]
    impl ObjectImpl for CreateDistroboxDialog {
        fn constructed(&self) {
            self.obj().set_title("Create a Distrobox");
            self.obj().set_content_width(480);

            let navigation_view = &self.navigation_view;
            let toolbar_view = &self.toolbar_view;
            let header = adw::HeaderBar::new();

            // Create view switcher and stack
            let view_stack = adw::ViewStack::new();

            self.content.set_margin_start(12);
            self.content.set_margin_end(12);
            self.content.set_margin_top(12);
            self.content.set_margin_bottom(12);
            self.content.set_spacing(12);
            self.content.set_orientation(gtk::Orientation::Vertical);

            // Create cloning_content box with header and sidebar
            self.cloning_content
                .set_orientation(gtk::Orientation::Vertical);
            self.cloning_content.set_spacing(12);
            self.cloning_content.set_visible(false);

            // Create header box with "Cloning" label
            let cloning_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            cloning_header.set_homogeneous(false);

            let cloning_label = gtk::Label::new(Some("Cloning"));
            cloning_label.set_halign(gtk::Align::Start);
            cloning_label.add_css_class("title-3");

            cloning_header.set_hexpand(true);
            cloning_header.append(&cloning_label);

            self.cloning_content.append(&cloning_header);

            // Add warning banner for running containers
            self.clone_warning_banner
                .set_title("Cloning the container requires stopping it first");
            self.clone_warning_banner.set_revealed(false);
            self.cloning_content.append(&self.clone_warning_banner);

            self.content.append(&self.cloning_content);

            let preferences_group = adw::PreferencesGroup::new();
            preferences_group.set_title("Settings");
            self.name_row.set_title("Name");

            self.image_row.set_title("Base Image");
            self.image_row.set_subtitle("Select an image...");
            self.image_row.set_activatable(true);
            self.image_row
                .add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));

            let obj = self.obj().clone();
            self.image_row.connect_activated(clone!(
                #[weak]
                obj,
                move |_| {
                    let picker = obj.build_image_picker_view();
                    obj.imp().navigation_view.push(&picker);
                }
            ));

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
            self.content.append(&preferences_group);
            self.content.append(&volumes_group);

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
                            // If cloning from a source, delegate to clone_container, otherwise create normally
                            if let Some(src) = obj.clone_src() {
                                src.stop();
                                obj.root_store().clone_container(&src.name(), create_args);
                            } else {
                                obj.root_store().create_container(create_args);
                            }
                            obj.close();
                        }
                    });
                }
            ));
            create_btn.add_css_class("suggested-action");
            create_btn.add_css_class("pill");
            create_btn.set_margin_top(12);

            self.content.append(&create_btn);

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
            view_stack.add_titled(&self.content, Some("create"), "Guided");
            view_stack.add_titled(&assemble_page, Some("assemble-file"), "From File");
            view_stack.add_titled(&url_page, Some("assemble-url"), "From URL");

            // Create a box to hold the view switcher and content
            let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

            // Add inline view switcher
            self.view_switcher.set_stack(Some(&view_stack));
            self.view_switcher.set_margin_start(12);
            self.view_switcher.set_margin_end(12);
            self.view_switcher.set_margin_top(12);
            self.view_switcher.set_margin_bottom(12);

            content_box.append(&self.view_switcher);
            content_box.append(&view_stack);

            // Wrap content_box in a scrolled window
            let scrolled_window = gtk::ScrolledWindow::new();
            scrolled_window.set_propagate_natural_height(true);
            scrolled_window.set_child(Some(&content_box));

            toolbar_view.add_top_bar(&header);
            toolbar_view.set_vexpand(true);
            toolbar_view.set_content(Some(&scrolled_window));

            let page = adw::NavigationPage::new(toolbar_view, "main");
            navigation_view.add(&page);
            self.obj().set_child(Some(navigation_view));
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
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}
impl CreateDistroboxDialog {
    pub fn new(root_store: RootStore) -> Self {
        let this: Self = glib::Object::builder()
            .property("root-store", root_store)
            .build();

        this.root_store().images_query().connect_success(clone!(
            #[weak]
            this,
            move |images| {
                let string_list = &this.imp().images_model;
                string_list.splice(0, string_list.n_items(), &[]);
                let new_items: Vec<&str> = images.iter().map(|s| s.as_str()).collect();
                string_list.splice(0, 0, &new_items);
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

        let this_clone = this.clone();
        this.root_store()
            .downloaded_images_query()
            .connect_success(move |images| {
                *this_clone.imp().downloaded_tags.borrow_mut() = images.clone();
            });

        this.root_store().downloaded_images_query().refetch();

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

    pub fn build_image_picker_view(&self) -> adw::NavigationPage {
        let view = adw::ToolbarView::new();

        let header = adw::HeaderBar::new();
        view.add_top_bar(&header);

        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search image..."));
        search_entry.set_hexpand(true);

        header.set_title_widget(Some(&search_entry));

        let model = self.imp().images_model.clone();
        let expression = gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            None::<gtk::Expression>,
            "string",
        );
        let filter = gtk::StringFilter::builder()
            .expression(&expression)
            .match_mode(gtk::StringFilterMatchMode::Substring)
            .ignore_case(true)
            .build();

        search_entry
            .bind_property("text", &filter, "search")
            .sync_create()
            .build();

        let filter_model = gtk::FilterListModel::new(Some(model), Some(filter));
        let selection_model = gtk::SingleSelection::new(Some(filter_model.clone()));

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = image_row_item::ImageRowItem::new();
            item.set_child(Some(&row));
        });
        let obj = self.clone();
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let image = item
                .item()
                .and_downcast::<gtk::StringObject>()
                .unwrap()
                .string();
            let child = item.child();
            let child: &image_row_item::ImageRowItem = child.and_downcast_ref().unwrap();
            child.set_image(&image);

            let is_downloaded = obj.imp().downloaded_tags.borrow().contains(image.as_str());
            child.set_is_downloaded(is_downloaded);
        });

        let list_view = gtk::ListView::new(Some(selection_model.clone()), Some(factory));
        list_view.add_css_class("navigation-sidebar");
        list_view.set_single_click_activate(true);

        let custom_list = gtk::ListBox::new();
        custom_list.add_css_class("navigation-sidebar");
        custom_list.set_selection_mode(gtk::SelectionMode::None);

        let custom_row_item = image_row_item::ImageRowItem::new();
        distro_icon::remove_color(&custom_row_item.imp().icon);

        custom_list.append(&custom_row_item);

        let custom_label = gtk::Label::new(Some("Custom"));
        custom_label.add_css_class("heading");
        custom_label.set_halign(gtk::Align::Start);
        custom_label.set_margin_start(12);
        custom_label.set_margin_top(12);

        // Create a box to hold both the list view and custom list
        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content_box.append(&list_view);
        content_box.append(&custom_label);
        content_box.append(&custom_list);

        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_child(Some(&content_box));
        scrolled_window.set_vexpand(true);

        view.set_content(Some(&scrolled_window));

        // Update custom row
        search_entry.connect_search_changed(clone!(
            #[weak]
            custom_row_item,
            #[weak]
            custom_list,
            move |entry| {
                let text = entry.text();
                if text.is_empty() {
                    custom_list.set_sensitive(false);
                } else {
                    custom_list.set_sensitive(true);
                    custom_row_item.set_image(&text);
                }
            }
        ));
        // Initial button state
        if search_entry.text().is_empty() {
            custom_list.set_sensitive(false);
        }

        let handle_image_selected = clone!(
            #[weak(rename_to=this)]
            self,
            move |image: &str| {
                this.imp().selected_image.replace(image.to_string());
                this.imp().image_row.set_subtitle(image);
                this.imp().navigation_view.pop();
            }
        );

        // Handle Enter key on search_entry to select first filtered image
        search_entry.connect_activate(clone!(
            #[strong]
            handle_image_selected,
            #[weak]
            list_view,
            #[weak]
            selection_model,
            move |entry| {
                let text = entry.text();
                if text.is_empty() {
                    return;
                }
                if selection_model.n_items() > 0 {
                    list_view
                        .activate_action("list.activate-item", Some(&glib::Variant::from(0u32)))
                        .unwrap();
                } else {
                    handle_image_selected(&entry.text());
                }
            }
        ));

        // Handle Escape key to close the image selector
        let escape_key_controller = gtk::EventControllerKey::new();
        escape_key_controller.connect_key_pressed(clone!(
            #[weak(rename_to=this)]
            self,
            #[upgrade_or]
            glib::signal::Propagation::Stop,
            move |_, key, _, _| {
                match key {
                    gtk::gdk::Key::Escape => {
                        this.imp().navigation_view.pop();
                        glib::signal::Propagation::Stop
                    }
                    _ => glib::signal::Propagation::Proceed,
                }
            }
        ));
        search_entry.add_controller(escape_key_controller);

        // Handle custom image selection
        custom_list.connect_row_activated(clone!(
            #[weak]
            search_entry,
            #[strong]
            handle_image_selected,
            move |_, _| {
                handle_image_selected(&search_entry.text());
            }
        ));

        // Handle selection
        list_view.connect_activate(clone!(move |list_view, position| {
            let model = list_view.model().unwrap(); // SingleSelection
            let item = model
                .item(position)
                .unwrap()
                .downcast::<gtk::StringObject>()
                .unwrap();
            let image = item.string();

            handle_image_selected(&image);
        }));

        adw::NavigationPage::new(&view, "image-picker")
    }

    pub async fn extract_create_args(&self) -> Result<CreateArgs, Error> {
        let imp = self.imp();
        let image = imp.selected_image.borrow().clone();
        if image.is_empty() && imp.clone_src.borrow().is_none() {
            return Err(Error::InvalidField(
                "image".into(),
                "No image selected".into(),
            ));
        }
        let volumes = imp
            .volume_rows
            .borrow()
            .iter()
            .filter_map(|entry| {
                if !entry.text().is_empty() {
                    match entry.text().parse::<backends::Volume>() {
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

    fn update_errors<T>(&self, res: &Result<T, backends::Error>) {
        let imp = self.imp();
        imp.name_row.remove_css_class("error");
        imp.name_row.set_tooltip_text(None);
        if let Err(e) = res {
            error!(error = %e, "CreateDistroboxDialog: update_errors");
        }
        match res {
            Err(backends::Error::InvalidField(field, msg)) if field == "name" => {
                imp.name_row.add_css_class("error");
                imp.name_row.set_tooltip_text(Some(msg));
            }
            _ => {}
        }
    }
}
