use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::gio::File;
use gtk::{gio, glib};
use std::time::Duration;
use tracing::error;

use crate::backends::{self, CreateArgName, CreateArgs, Error};
use crate::dialogs::create_distrobox_helpers::split_repo_tag_digest;
use crate::fakers::Command;
use crate::i18n::gettext;
use crate::models::Container;
use crate::query::Query;
use crate::root_store::RootStore;
use crate::widgets::{ImageRowItem, SidebarRow};

use std::collections::HashSet;
use std::path::PathBuf;
use std::{cell::RefCell, rc::Rc};

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
        pub toast_overlay: adw::ToastOverlay,
        pub name_row: adw::EntryRow,
        pub image_row: adw::ActionRow,
        pub images_model: gtk::StringList,
        pub selected_image: RefCell<String>,
        pub prefill_query: RefCell<Option<Query<Option<String>>>>,
        pub ini_content_query: RefCell<Option<Query<String>>>,
        pub home_row_expander: adw::ExpanderRow,
        #[property(get, set, nullable)]
        pub home_folder: RefCell<Option<String>>,
        #[property(get, set, nullable)]
        pub assemble_file: RefCell<Option<String>>,
        #[property(get, set, nullable)]
        pub assemble_url: RefCell<Option<String>>,
        pub nvidia_row: adw::SwitchRow,
        pub init_row: adw::SwitchRow,
        pub hostname_row: adw::EntryRow,
        pub root_row: adw::SwitchRow,
        pub volume_rows: Rc<RefCell<Vec<adw::EntryRow>>>,
        pub scrolled_window: gtk::ScrolledWindow,
        #[property(get, set, nullable, construct_only)]
        pub clone_src: RefCell<Option<Container>>,
        pub view_switcher: adw::InlineViewSwitcher,
        pub downloaded_tags: RefCell<HashSet<String>>,
    }

    #[derived_properties]
    impl ObjectImpl for CreateDistroboxDialog {
        fn constructed(&self) {
            self.obj().set_title(&gettext("Create a Distrobox"));
            self.obj().set_content_width(480);

            let navigation_view = &self.navigation_view;
            let toolbar_view = &self.toolbar_view;
            let header = adw::HeaderBar::new();

            // Create view switcher and stack
            let view_stack = adw::ViewStack::new();

            let guided_page = self.obj().build_guided_page();
            let assemble_page = self.obj().build_assemble_from_file_page();
            let url_page = self.obj().build_assemble_from_url_page();

            // Add pages to view stack
            view_stack.add_titled(&guided_page, Some("create"), "Guided");
            view_stack.add_titled(&assemble_page, Some("assemble-file"), "From File");
            view_stack.add_titled(&url_page, Some("assemble-url"), "From URL");

            // Create a box to hold the view switcher and content
            let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

            // Add inline view switcher only if not cloning
            // When cloning from an existing container, we skip the view switcher and go directly to guided page
            if self.clone_src.borrow().is_none() {
                self.view_switcher.set_stack(Some(&view_stack));
                self.view_switcher.set_margin_start(12);
                self.view_switcher.set_margin_end(12);
                self.view_switcher.set_margin_top(12);
                self.view_switcher.set_margin_bottom(12);

                content_box.append(&self.view_switcher);
            }

            content_box.append(&view_stack);

            // Wrap content_box in a scrolled window
            let scrolled_window = gtk::ScrolledWindow::new();
            scrolled_window.set_propagate_natural_height(true);
            scrolled_window.set_child(Some(&content_box));

            // Wrap in toast overlay for showing notifications
            self.toast_overlay.set_child(Some(&scrolled_window));

            toolbar_view.add_top_bar(&header);
            toolbar_view.set_vexpand(true);
            toolbar_view.set_content(Some(&self.toast_overlay));

            let page = adw::NavigationPage::new(toolbar_view, "Create a Distrobox");
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
    pub fn new(root_store: RootStore, clone_src: Option<Container>) -> Self {
        let this: Self = glib::Object::builder()
            .property("root-store", root_store)
            .property("clone-src", clone_src)
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

    pub fn build_guided_page(&self) -> adw::NavigationPage {
        let imp = self.imp();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        let page = adw::NavigationPage::new(&content, &gettext("Guided"));

        // Create cloning_content box with header and sidebar
        // Only show cloning UI if we're cloning from an existing container
        if let Some(container) = self.clone_src() {
            imp.image_row.set_visible(false);

            let cloning_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
            content.append(&cloning_content);

            let cloning_header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            cloning_header.set_homogeneous(false);

            let cloning_label = gtk::Label::new(Some(&gettext("Cloning")));
            cloning_label.set_halign(gtk::Align::Start);
            cloning_label.add_css_class("title-3");

            cloning_header.set_hexpand(true);
            cloning_header.append(&cloning_label);
            cloning_content.append(&cloning_header);

            let sidebar_row = SidebarRow::new(&container);
            cloning_content.append(&sidebar_row);

            // Show warning if container is running
            if container.is_running() {
                let clone_warning_banner = adw::Banner::new(&gettext("Cloning the container requires stopping it first"));
                clone_warning_banner.set_revealed(true);
                cloning_content.append(&clone_warning_banner);
            }
        }

        let preferences_group = adw::PreferencesGroup::new();
        preferences_group.set_title(&gettext("Settings"));
        imp.name_row.set_title(&gettext("Name"));

        imp.image_row.set_title(&gettext("Base Image"));
        imp.image_row.set_subtitle(&gettext("Select an image..."));
        imp.image_row.set_activatable(true);
        imp.image_row
            .add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));

        imp.image_row.connect_activated(clone!(
            #[weak(rename_to=this)]
            self,
            move |_| {
                // Read the current subtitle and derive an initial search string
                let subtitle: String = this.imp().image_row.property("subtitle");
                let default_sub = gettext("Select an image...");

                let initial_search_repo: &str = split_repo_tag_digest(if subtitle == default_sub {
                    ""
                } else {
                    &subtitle
                })
                .0;
                // A repo is docker.io/library/xyz by default, we only want to search by 'xyz'
                let initial_search = initial_search_repo
                    .rsplit('/')
                    .next()
                    .unwrap_or(initial_search_repo);

                let picker = this.build_image_picker_view(Some(initial_search));
                this.imp().navigation_view.push(&picker);
            }
        ));

        let this = self.clone();
        let home_row = self.build_file_row(
            &gettext("Select Home Directory"),
            FileRowSelection::Folder,
            None, // No filter for folders
            move |path| {
                this.set_home_folder(Some(path.display().to_string()));
            },
        );
        imp.home_row_expander
            .set_title(&gettext("Custom Home Directory"));
        imp.home_row_expander.set_show_enable_switch(true);
        imp.home_row_expander.set_enable_expansion(false);
        imp.home_row_expander.add_row(&home_row);
        imp.home_row_expander
            .connect_enable_expansion_notify(clone!(
                #[weak(rename_to=this)]
                self,
                move |expander| {
                    if !expander.enables_expansion() {
                        this.set_home_folder(None::<&str>);
                    }
                    home_row.set_subtitle(this.home_folder().as_deref().unwrap_or(""));
                }
            ));

        imp.nvidia_row.set_title(&gettext("NVIDIA Support"));

        imp.init_row.set_title(&gettext("Init process"));

        imp.hostname_row.set_title(&gettext("Hostname"));

        imp.root_row.set_title(&gettext("Privileged"));

        preferences_group.add(&imp.name_row);
        preferences_group.add(&imp.image_row);
        preferences_group.add(&imp.home_row_expander);

        let advanced_group = adw::PreferencesGroup::new();
        advanced_group.set_title(&gettext("Advanced"));
        advanced_group.add(&imp.hostname_row);
        advanced_group.add(&imp.root_row);
        advanced_group.add(&imp.nvidia_row);
        advanced_group.add(&imp.init_row);

        let volumes_group = self.build_volumes_group();
        content.append(&preferences_group);
        content.append(&advanced_group);
        content.append(&volumes_group);

        let create_btn = self.build_create_btn();
        create_btn.set_sensitive(false);

        create_btn.connect_clicked(clone!(
            #[weak(rename_to=this)]
            self,
            move |_| {
                glib::MainContext::ref_thread_default().spawn_local(async move {
                    let res = this.extract_create_args().await;
                    this.update_errors(&res);
                    if let Ok(create_args) = res {
                        // If cloning from a source, delegate to clone_container, otherwise create normally
                        if let Some(src) = this.clone_src() {
                            src.stop();
                            this.root_store().clone_container(&src.name(), create_args);
                        } else {
                            this.root_store().create_container(create_args);
                        }
                        this.close();
                    }
                });
            }
        ));

        content.append(&create_btn);

        // Add name validation for Create button sensitivity and duplicate name check
        imp.name_row.connect_changed(clone!(
            #[weak]
            create_btn,
            #[weak(rename_to=this)]
            self,
            move |entry| {
                let text = entry.text();
                let is_valid = !text.is_empty() && backends::CreateArgName::new(&text).is_ok();
                
                // Check for duplicate container names
                let mut has_duplicate = false;
                if is_valid {
                    for container in this.root_store().containers().iter() {
                        if container.name() == text.as_str() {
                            has_duplicate = true;
                            break;
                        }
                    }
                }
                
                create_btn.set_sensitive(is_valid && !has_duplicate);
            }
        ));

        // Prefill wiring: debounce name changes to suggest an image when user hasn't interacted
        let prefill_query: Query<Option<String>> = Query::new(
            "prefill-suggestions".to_string(),
            clone!(
                #[weak(rename_to=this)]
                self,
                #[upgrade_or_panic]
                move || async move {
                    let imp = this.imp();
                    let text = imp.name_row.text().to_string();

                    // don't prefill if cloning from a source
                    if imp.clone_src.borrow().is_some() {
                        return Ok(None);
                    }

                    if text.is_empty() {
                        if imp.selected_image.borrow().is_empty() {
                            return Ok(Some(gettext("Select an image...")));
                        }
                        return Ok(None);
                    }

                    let candidates = imp
                        .images_model
                        .snapshot()
                        .into_iter()
                        .filter_map(|item| {
                            item.downcast::<gtk::StringObject>()
                                .ok()
                                .map(|sobj| sobj.string().to_string())
                        })
                        .collect::<Vec<_>>();

                    let (_filter, suggested_opt) =
                        crate::dialogs::create_distrobox_helpers::derive_image_prefill(
                            &text,
                            Some(&candidates),
                        );

                    Ok(suggested_opt)
                }
            ),
        );

        prefill_query.connect_success(clone!(
            #[weak(rename_to=this)]
            self,
            move |suggested_opt| {
                let imp = this.imp();
                if let Some(suggested) = suggested_opt.as_ref() {
                    // set subtitle as tentative prefill (do not overwrite confirmed selection)
                    if imp.selected_image.borrow().is_empty() {
                        imp.image_row.set_subtitle(suggested);
                    }
                }
            }
        ));

        prefill_query.set_refetch_strategy(Query::debounce(Duration::from_millis(300)));
        *imp.prefill_query.borrow_mut() = Some(prefill_query.clone());

        imp.name_row.connect_changed(move |_| {
            prefill_query.refetch();
        });

        page
    }
    pub fn build_assemble_from_file_page(&self) -> adw::NavigationPage {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);

        let page = adw::NavigationPage::new(&content, &gettext("Assemble from File"));

        let assemble_group = adw::PreferencesGroup::new();
        assemble_group.set_title(&gettext("Assemble from File"));
        assemble_group.set_description(Some(&gettext("Create a container from an assemble file")));

        let ini_filter = gtk::FileFilter::new();
        ini_filter.set_name(Some(&gettext("INI Files")));
        ini_filter.add_pattern("*.ini");

        let this = self.clone();
        let file_row = self.build_file_row(
            &gettext("Select Assemble File"),
            FileRowSelection::File,
            Some(&ini_filter),
            move |path| {
                this.set_assemble_file(Some(path.display().to_string()));
            },
        );
        assemble_group.add(&file_row);
        content.append(&assemble_group);

        let create_btn = self.build_create_btn();
        create_btn.set_sensitive(false);
        content.append(&create_btn);

        // Handle create click
        create_btn.connect_clicked(clone!(
            #[weak(rename_to=this)]
            self,
            move |_| {
                if let Some(path) = this.assemble_file() {
                    this.root_store().assemble_container(path.as_ref());
                    this.close();
                }
            }
        ));

        // Enable button when file is selected
        self.connect_assemble_file_notify(move |obj| {
            create_btn.set_sensitive(obj.assemble_file().is_some());
        });
        page
    }

    pub fn build_assemble_from_url_page(&self) -> adw::NavigationPage {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(12);
        content.set_margin_bottom(12);

        let page = adw::NavigationPage::new(&content, &gettext("Assemble from URL"));

        let url_group = adw::PreferencesGroup::new();
        url_group.set_title(&gettext("Assemble from URL"));
        url_group.set_description(Some(&gettext("Create a container from a remote URL")));

        let url_row = adw::EntryRow::new();
        url_row.set_title(&gettext("URL"));
        url_row.set_text("https://example.com/container.ini");

        url_group.add(&url_row);
        content.append(&url_group);

        // Create preview section with always-visible text view
        let preview_label = gtk::Label::new(Some(&gettext("Configuration Preview")));
        preview_label.set_halign(gtk::Align::Start);
        preview_label.add_css_class("heading");

        // Create TextView for content display
        let text_view = gtk::TextView::new();
        text_view.set_editable(false);
        text_view.set_cursor_visible(false);
        text_view.set_monospace(true);
        text_view.set_wrap_mode(gtk::WrapMode::None);

        // Wrap TextView in ScrolledWindow
        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_child(Some(&text_view));
        scrolled_window.set_min_content_height(200);
        scrolled_window.set_max_content_height(400);
        scrolled_window.set_vexpand(true);

        let preview_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        preview_box.append(&preview_label);
        preview_box.append(&scrolled_window);

        content.append(&preview_box);

        // Add create button for URL
        let create_btn = self.build_create_btn();
        create_btn.set_sensitive(false);
        content.append(&create_btn);

        // Create ini_content_query for downloading .ini file
        let ini_content_query: Query<String> = Query::new(
            "ini-content-download".to_string(),
            clone!(
                #[weak(rename_to=this)]
                self,
                #[upgrade_or_panic]
                move || async move {
                    if let Some(url_text) = this.assemble_url() {
                        if url_text.is_empty() {
                            return Err(anyhow::anyhow!("URL is empty"));
                        }
                        this.download_ini_file(&url_text).await
                    } else {
                        Err(anyhow::anyhow!("No URL provided"))
                    }
                }
            ),
        ).with_timeout(Duration::from_secs(10));

        // Wire ini_content_query success handler
        ini_content_query.connect_success(clone!(
            #[weak]
            text_view,
            #[weak]
            create_btn,
            move |content| {
                // Set content in TextView
                text_view.buffer().set_text(content);

                // Enable create button
                create_btn.set_sensitive(true);
            }
        ));

        // Wire ini_content_query loading handler
        ini_content_query.connect_loading(clone!(
            #[weak]
            create_btn,
            move |is_loading| {
                if is_loading {
                    create_btn.set_sensitive(false);
                }
            }
        ));

        // Wire ini_content_query error handler
        ini_content_query.connect_error(clone!(
            #[weak(rename_to=this)]
            self,
            #[weak]
            create_btn,
            #[weak]
            url_row,
            move |error| {
                create_btn.set_sensitive(false);
                url_row.add_css_class("error");

                let toast = adw::Toast::new(&format!("{}: {}", gettext("Download failed"), error));
                this.imp().toast_overlay.add_toast(toast);
            }
        ));

        ini_content_query.set_refetch_strategy(Query::debounce(Duration::from_millis(500)));
        *self.imp().ini_content_query.borrow_mut() = Some(ini_content_query.clone());

        url_row.connect_changed(clone!(
            #[weak(rename_to=this)]
            self,
            #[weak]
            create_btn,
            #[weak]
            text_view,
            #[strong]
            ini_content_query,
            move |entry| {
                this.set_assemble_url(Some(entry.text()));
                create_btn.set_sensitive(false);
                entry.remove_css_class("error");
                
                text_view.buffer().set_text("");
                
                // Debounced download (validation happens implicitly)
                ini_content_query.refetch();
            }
        ));

        // Handle create click
        create_btn.connect_clicked(clone!(
            #[weak(rename_to=this)]
            self,
            move |_| {
                this.root_store()
                    .assemble_container(this.assemble_url().as_ref().unwrap());
                this.close();
            }
        ));

        page
    }

    pub fn build_create_btn(&self) -> gtk::Button {
        let create_btn = gtk::Button::with_label(&gettext("Create"));
        create_btn.set_halign(gtk::Align::Center);
        create_btn.add_css_class("suggested-action");
        create_btn.add_css_class("pill");
        create_btn.set_margin_top(12);
        create_btn
    }

    pub fn build_file_row(
        &self,
        title: &str,
        selection: FileRowSelection,
        filter: Option<&gtk::FileFilter>,
        cb: impl Fn(PathBuf) + Clone + 'static,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle(&gettext("No file selected"));
        row.set_activatable(true);

        let file_icon = gtk::Image::from_icon_name("document-open-symbolic");
        row.add_suffix(&file_icon);

        let title = title.to_owned();
        let filter = filter.cloned(); // Clone the Option<&FileFilter> to Option<FileFilter>
        let dialog_cb = clone!(
            #[weak(rename_to=this)]
            self,
            #[strong]
            title,
            #[weak]
            row,
            move |res: Result<File, _>| {
                if let Ok(file) = res
                    && let Some(path) = file.path()
                {
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
        );
        row.connect_activated(move |_| {
            let file_dialog = gtk::FileDialog::builder().title(&title).modal(true).build();

            // Apply filter if provided
            if let Some(ref f) = filter {
                let filters = gio::ListStore::new::<gtk::FileFilter>();
                filters.append(f);
                file_dialog.set_filters(Some(&filters));
                file_dialog.set_default_filter(Some(f));
            }

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

    pub fn build_image_picker_view(&self, initial_search: Option<&str>) -> adw::NavigationPage {
        let view = adw::ToolbarView::new();

        let header = adw::HeaderBar::new();
        view.add_top_bar(&header);

        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some(&gettext("Search or enter custom image...")));
        search_entry.set_hexpand(true);
        if let Some(text) = initial_search {
            search_entry.set_text(text);
        }

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
            let row = ImageRowItem::new();
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
            let child: &ImageRowItem = child.and_downcast_ref().unwrap();
            child.set_image(&image);

            // TODO: Consider doing an availability check (remote / lazy-download)
            // to determine if an image is actually accessible, not just in the
            // downloaded tags set.
            let is_downloaded = obj.imp().downloaded_tags.borrow().contains(image.as_str());
            child.set_is_downloaded(is_downloaded);
        });

        let list_view = gtk::ListView::new(Some(selection_model.clone()), Some(factory));
        list_view.add_css_class("navigation-sidebar");
        list_view.set_single_click_activate(true);

        let custom_list = gtk::ListBox::new();
        custom_list.add_css_class("navigation-sidebar");
        custom_list.set_selection_mode(gtk::SelectionMode::None);

        let custom_row_item = ImageRowItem::new();
        custom_row_item.imp().icon.set_colored(false);

        custom_list.append(&custom_row_item);

        let custom_label = gtk::Label::new(Some(&gettext("Custom")));
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

        adw::NavigationPage::new(&view, "Select Image")
    }

    pub async fn extract_create_args(&self) -> Result<CreateArgs, Error> {
        let imp = self.imp();
        let image = {
            let sel = imp.selected_image.borrow();
            if sel.is_empty() {
                // fallback to the action row subtitle (tentative prefill)
                let subtitle: String = imp.image_row.property("subtitle");
                let default_sub = gettext("Select an image...");
                if subtitle.is_empty() || subtitle == default_sub {
                    String::new()
                } else {
                    subtitle
                }
            } else {
                sel.clone()
            }
        };
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
        let hostname = {
            let value = imp.hostname_row.text().trim().to_string();
            if value.is_empty() {
                None
            } else {
                Some(value)
            }
        };

        let create_args = CreateArgs {
            name,
            image: image.to_string(),
            nvidia: imp.nvidia_row.is_active(),
            home_path: self.home_folder(),
            init: imp.init_row.is_active(),
            hostname,
            root: imp.root_row.is_active(),
            volumes,
        };

        Ok(create_args)
    }

    pub fn build_volumes_group(&self) -> adw::PreferencesGroup {
        let volumes_group = adw::PreferencesGroup::new();
        volumes_group.set_title(&gettext("Volumes"));
        volumes_group.set_description(Some(&gettext(
            "Specify volumes in the format 'host_path:container_path'",
        )));

        let add_volume_button = adw::ButtonRow::builder()
            .title(&gettext("Add Volume"))
            .build();
        add_volume_button.connect_activated(clone!(
            #[weak(rename_to=this)]
            self,
            #[weak]
            volumes_group,
            move |_| {
                let volume_row = adw::EntryRow::new();
                volume_row.set_title(&gettext("Volume"));

                let remove_button = gtk::Button::from_icon_name("user-trash-symbolic");
                remove_button.set_tooltip_text(Some(&gettext("Remove Volume")));
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
                // Show toast for name validation error
                let toast = adw::Toast::new(msg);
                imp.toast_overlay.add_toast(toast);
            }
            Err(backends::Error::InvalidField(_, msg)) => {
                // Show toast for other field validation errors
                let toast = adw::Toast::new(msg);
                imp.toast_overlay.add_toast(toast);
            }
            _ => {}
        }
    }

    async fn download_ini_file(&self, url: &str) -> anyhow::Result<String> {
        // Download the .ini file content using curl
        // CRITICAL: Use self.root_store().command_runner() for Flatpak compatibility
        let command_runner = self.root_store().command_runner();
        let mut cmd = Command::new("curl");
        cmd.arg("-s"); // Silent
        cmd.arg("-f"); // Fail on HTTP errors
        cmd.arg("-L"); // Follow redirects
        cmd.arg("--connect-timeout");
        cmd.arg("10"); // 10 second connection timeout
        cmd.arg("--max-time");
        cmd.arg("30"); // 30 second max time
        cmd.arg(url);

        let output = command_runner.output(cmd).await?;
        
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to download file: HTTP error"));
        }

        let content = String::from_utf8(output.stdout)
            .map_err(|_| anyhow::anyhow!("Downloaded file is not valid UTF-8"))?;

        if content.is_empty() {
            return Err(anyhow::anyhow!("Downloaded file is empty"));
        }

        Ok(content)
    }
}
