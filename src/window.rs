/* window.rs
 *
 * Copyright 2024 Lorenzo Miglietta
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::container::Container;
use crate::create_distrobox_dialog::CreateDistroboxDialog;
use crate::distrobox::{self, CreateArgs, Distrobox, ExportableApp, Status};
use crate::distrobox_service::DistroboxService;
use crate::distrobox_task::DistroboxTask;
use crate::exportable_apps_dialog::ExportableAppsDialog;
use crate::known_distros::known_distro_by_image;
use crate::resource::{Resource, SharedResource};
use crate::sidebar_row::SidebarRow;
use crate::supported_terminals::SUPPORTED_TERMINALS;
use crate::tasks_button::TasksButton;
use crate::{known_distros, supported_terminals};
use adw::prelude::*;
use adw::subclass::{preferences_group, prelude::*};
use anyhow::Context;
use gtk::glib::clone;
use gtk::{gio, glib, pango};

mod imp {
    use std::{
        cell::{OnceCell, RefCell},
        collections::{HashMap, HashSet}, sync::Once,
    };

    use gtk::gdk;

    use crate::{distrobox_service::DistroboxService, resource::Resource};

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/ranfdev/DistroHome/window.ui")]
    pub struct DistrohomeWindow {
        // Template widgets
        #[template_child]
        pub sidebar_slot: TemplateChild<adw::Bin>,
        #[template_child]
        pub create_distrobox_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub sidebar_bottom_slot: TemplateChild<adw::Bin>,
        pub sidebar_list_box: gtk::ListBox,
        #[template_child]
        pub main_slot: TemplateChild<adw::Bin>,
        #[template_child]
        pub split_view: TemplateChild<adw::NavigationSplitView>,
        pub distrobox_service: OnceCell<DistroboxService>,
        pub selected_container: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DistrohomeWindow {
        const NAME: &'static str = "DistrohomeWindow";
        type Type = super::DistrohomeWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("win.refresh", None, |win, _action, _target| {
                dbg!("Refreshing containers...");
                win.distrobox_service().load_container_infos();
            });
            klass.add_binding_action(gdk::Key::F5, gdk::ModifierType::empty(), "win.refresh");

            klass.install_action("win.upgrade-all", None, |win, _action, _target| {
                if let Resource::Loaded(containers) = win.distrobox_service().containers() {
                    for container in containers.values() {
                        win.distrobox_service().do_upgrade(&container.name());
                    }
                }
            });

            klass.install_action("win.preferences", None, |win, _action, _target| {
                win.build_preferences_dialog();
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DistrohomeWindow {}
    impl WidgetImpl for DistrohomeWindow {}
    impl WindowImpl for DistrohomeWindow {}
    impl ApplicationWindowImpl for DistrohomeWindow {}
    impl AdwApplicationWindowImpl for DistrohomeWindow {}
}

glib::wrapper! {
    pub struct DistrohomeWindow(ObjectSubclass<imp::DistrohomeWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,        @implements gio::ActionGroup, gio::ActionMap;
}

impl DistrohomeWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P, distrobox_service: DistroboxService) -> Self {
        let this: Self = glib::Object::builder()
            .property("application", application)
            .build();

        this.imp().distrobox_service.set(distrobox_service);

        this.build_sidebar();

        let this_clone = this.clone();
        this.distrobox_service()
            .connect_containers_changed(move |service| {
                this_clone.fill_sidebar();
            });
        this.distrobox_service().load_container_infos();

        let this_clone = this.clone();
        this.distrobox_service()
            .connect_version_changed(move |service| match service.version() {
                Resource::Error(err, _) => {
                    this_clone.show_error_page(&err.to_string());
                }
                _ => {}
            });
        this.build_welcome_dialog();

        this
    }

    fn show_error_page(&self, error_message: &str) {
        let status_page = adw::StatusPage::builder()
            .title("Error")
            .description(error_message)
            .icon_name("dialog-error-symbolic")
            .build();

        self.imp().main_slot.set_child(Some(&status_page));
    }

    fn distrobox_service(&self) -> &DistroboxService {
        &self.imp().distrobox_service.get().unwrap()
    }

    fn selected_container_name(&self) -> Option<String> {
        self.imp().selected_container.borrow().clone()
    }

    fn selected_container(&self) -> Option<Container> {
        let name = self.selected_container_name()?;
        self.distrobox_service()
            .containers()
            .data()?
            .get(&name)
            .cloned()
    }

    fn build_sidebar(&self) {
        let imp = self.imp();

        let this = self.clone();
        imp.create_distrobox_btn.connect_clicked(move |_| {
            this.build_create_distrobox_dialog();
        });

        imp.sidebar_list_box.add_css_class("navigation-sidebar");
        imp.sidebar_list_box
            .set_selection_mode(gtk::SelectionMode::Single);

        let this = self.clone();

        imp.sidebar_list_box.connect_row_activated(move |_, _| {
            this.imp().split_view.set_show_content(true);
        });

        let this = self.clone();
        imp.sidebar_list_box
            .connect_row_selected(move |_list_box, row| {
                let Some(row) = row else {
                    return;
                };

                let child = row.child();
                let row: &SidebarRow = child.and_downcast_ref().unwrap();
                let containers = this.distrobox_service().containers();
                let Some(containers) = containers.data() else {
                    return;
                };
                if let Some(container) = containers.get(&row.name()) {
                    this.build_main_content(&container);
                }

                this.imp()
                    .selected_container
                    .replace(Some(row.name().clone()));
            });

        // Create a scrolled window for the sidebar
        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_width(200)
            .child(&imp.sidebar_list_box)
            .build();

        self.imp().sidebar_slot.set_child(Some(&scrolled_window));

        // Add tasks button to the bottom of the sidebar
        let tasks_button = TasksButton::new();
        tasks_button.add_css_class("flat");
        let this = self.clone();
        tasks_button.connect_task_clicked(move |button, task| {
            this.build_task_dialog(task);
        });
        self.imp()
            .sidebar_bottom_slot
            .set_child(Some(&tasks_button));

        let tasks_button_clone = tasks_button.clone();
        self.distrobox_service()
            .connect_tasks_changed(move |service| {
                let tasks = service.tasks();
                tasks_button_clone.update_tasks(&tasks[..]);
            });
    }

    fn fill_sidebar(&self) {
        let imp = self.imp();

        let containers = self.distrobox_service().containers();
        if let Resource::Loaded(containers) = containers {
            // remove previous rows
            while let Some(row) = imp.sidebar_list_box.first_child() {
                imp.sidebar_list_box.remove(&row);
            }

            let mut sorted_containers = containers.iter().collect::<Vec<_>>();
            sorted_containers.sort_by_key(|x| x.0);

            let selected_container = self.selected_container_name();

            for (name, info) in sorted_containers {
                // build the row
                let row = gtk::ListBoxRow::new();
                let sidebar_row = SidebarRow::new(&info);
                row.set_child(Some(&sidebar_row));
                imp.sidebar_list_box.append(&row);

                // select it if it was selected before
                if let Some(selected) = &selected_container {
                    if selected == name {
                        dbg!(selected);
                        imp.sidebar_list_box.select_row(Some(&row));
                    }
                }
            }

            // select the first is nothing was selected
            if selected_container.is_none() {
                if let Some(row) = imp.sidebar_list_box.first_child() {
                    let row = row.downcast_ref::<gtk::ListBoxRow>();
                    imp.sidebar_list_box.select_row(row);
                }
            }
        } else {
            dbg!("Loading containers...");
        }
    }

    pub fn build_container_header(&self, container: &Container) -> gtk::Box {
        // Create labels for the title and subtitle
        let title_label = gtk::Label::new(Some(&container.name()));
        title_label.set_xalign(0.0);
        title_label.add_css_class("title-1");

        let subtitle_label = gtk::Label::new(Some(&container.image()));
        subtitle_label.set_xalign(0.0);
        subtitle_label.set_ellipsize(pango::EllipsizeMode::End);
        subtitle_label.add_css_class("subtitle");

        // Create a vertical box to hold the title and subtitle
        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        text_box.append(&title_label);
        text_box.append(&subtitle_label);

        // Add the text box and status label to the header box
        let icon = gtk::Image::new();
        icon.add_css_class("header-icon");
        icon.set_icon_size(gtk::IconSize::Large);

        let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header_box.add_css_class("distro-header");

        header_box.append(&icon);
        header_box.append(&text_box);

        if let Some(distro) = container.distro() {
            header_box.add_css_class(distro.name);
            icon.set_icon_name(Some(&distro.name));
        }

        header_box
    }
    pub fn build_main_content(&self, container: &Container) {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let clamp = adw::Clamp::new();

        // Main container with scrolling
        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled_window.set_vexpand(true);

        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 24);
        main_box.set_margin_start(12);
        main_box.set_margin_end(12);
        main_box.set_margin_top(12);
        main_box.set_margin_bottom(24);

        // Status Group
        let status_group = adw::PreferencesGroup::new();
        status_group.set_title("Container Status");

        let status_row = adw::ActionRow::new();
        status_group.add_css_class("property");
        status_row.set_title("Status");
        status_row.set_subtitle(&container.status().to_string());

        let status_child = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        status_child.set_valign(gtk::Align::Center);

        let terminal_btn = gtk::Button::from_icon_name("terminal-symbolic");
        terminal_btn.set_tooltip_text(Some("Open Terminal"));
        terminal_btn.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            #[strong]
            container,
            move |_| {
                this.distrobox_service()
                    .do_spawn_terminal(&container.name());
            }
        ));

        if let Status::Up(_) = &container.status() {
            let stop_btn = gtk::Button::from_icon_name("media-playback-stop-symbolic");
            stop_btn.set_tooltip_text(Some("Stop"));

            status_child.append(&stop_btn);
            status_child.append(&terminal_btn);

            let container_name = container.name().clone();
            stop_btn.connect_clicked(clone!(
                #[weak(rename_to=this)]
                self,
                move |_| {
                    this.distrobox_service().do_stop(&container_name);
                }
            ));
        } else {
            status_child.append(&terminal_btn);
        }

        status_row.add_suffix(&status_child);
        status_group.add(&status_row);

        // Quick Actions Group
        let actions_group = adw::PreferencesGroup::new();
        actions_group.set_title("Quick Actions");

        let upgrade_row = Self::create_button_row(
            "Upgrade Container",
            "system-upgrade-symbolic",
            "Update all packages",
        );
        actions_group.add(&upgrade_row);

        let apps_row = Self::create_button_row(
            "Applications",
            "applications-all-symbolic",
            "Manage exportable applications",
        );
        actions_group.add(&apps_row);

        if let Some(pm) = container.distro().and_then(|distro| distro.package_manager) {
            let install_package_row = Self::create_button_row(
                &format!("Install {} Package", pm.installable_file()),
                "package-symbolic",
                "Install packages into container",
            );
            actions_group.add(&install_package_row);
            install_package_row.connect_activated(clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.build_install_package_dialog();
                }
            ));
        }

        let clone_row = Self::create_button_row(
            "Clone Container",
            "edit-copy-symbolic",
            "Create a copy of this container",
        );
        actions_group.add(&clone_row);

        // Danger Zone Group
        let danger_group = adw::PreferencesGroup::new();
        danger_group.set_title("Danger Zone");
        danger_group.add_css_class("danger-group");

        let delete_row = Self::create_button_row(
            "Delete Container",
            "user-trash-symbolic",
            "Permanently remove this container and all its data",
        );
        delete_row.add_css_class("error");

        danger_group.add(&delete_row);

        // Add all groups to main box
        main_box.append(&self.build_container_header(container));
        main_box.append(&status_group);
        main_box.append(&actions_group);
        main_box.append(&danger_group);

        upgrade_row.connect_activated(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                if let Some(ref name) = this.selected_container_name() {
                    this.build_upgrade_dialog(name);
                }
            }
        ));

        apps_row.connect_activated(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                if let Some(name) = this.selected_container_name() {
                    this.build_exportable_apps_dialog(&name);
                }
            }
        ));

        

        /*install_deb_row.connect_activated(clone!(@weak widget => move |_| {
            // TODO: Show file chooser
        }));

        clone_row.connect_activated(clone!(@weak widget => move |_| {
            // TODO: Show clone dialog
        }));
        */

        delete_row.connect_activated(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let dialog = adw::AlertDialog::builder()
                    .heading("Delete this container?")
                    .body(format!(
                        "{} will be deleted.\nThis action cannot be undone.",
                        this.selected_container_name().unwrap_or_default()
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
                        this,
                        move |dialog, _| {
                            if let Some(ref name) = this.selected_container_name() {
                                this.distrobox_service().do_delete(name);
                            }
                            dialog.close();
                        }
                    ),
                );

                dialog.present(Some(&this));
            }
        ));

        // Finish layout
        scrolled_window.set_child(Some(&main_box));
        clamp.set_child(Some(&scrolled_window));
        widget.append(&clamp);

        self.imp().main_slot.set_child(Some(&widget));
    }

    fn build_create_distrobox_dialog(&self) {
        let dialog = CreateDistroboxDialog::new(self.distrobox_service().clone());
        let this = self.clone();
        dialog.connect_create_requested(move |dialog, args| {
            let task = this.distrobox_service().do_create(args);
            let create_dialog = this.build_task_dialog(&task);
            task.connect_status_notify(move |task| {
                if task.status() == "successful" {
                    create_dialog.close();
                }
            });
            dialog.close();
        });
        dialog.present(Some(self));
    }

    fn build_install_package_dialog(&self) {
        if let Some(container) = self.selected_container() {
            if let Some(pm) = container.distro().and_then(|distro| distro.package_manager) {
                // Show file chooser and install package using the appropriate command
                let file_dialog = gtk::FileDialog::builder().title("Select Package").build();

                file_dialog.open(
                    Some(self),
                    None::<&gio::Cancellable>,
                    clone!(
                        #[weak(rename_to = this)]
                        self,
                        move |res| {
                            if let Ok(file) = res {
                                if let Some(path) = file.path() {
                                    dbg!(&path);
                                    this.distrobox_service()
                                        .do_install(&container.name(), &path);
                                }
                            }
                        }
                    ),
                );
            }
        }
    }

    fn build_task_dialog(&self, task: &DistroboxTask) -> adw::Dialog {
        let dialog = adw::Dialog::new();
        dialog.set_title(&format!("{}: {}", task.target(), task.name()));
        dialog.set_content_width(360);

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(
            &adw::HeaderBar::builder()
                .show_start_title_buttons(false)
                .show_end_title_buttons(false)
                .build(),
        );

        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_bottom(12);

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

        task.connect_status_notify(clone!(
            #[weak]
            status_label,
            move |task| {
                status_label.set_text(&format!("Status: {}", task.status()));
            }
        ));

        if task.is_failed() {
            if let Some(error) = task.take_error() {
                let error_label = gtk::Label::new(Some(&format!("Error: {}", error)));
                error_label.set_xalign(0.0);
                content.append(&error_label);
            }
        }

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
        let close_btn = gtk::Button::with_label("Hide");
        close_btn.connect_clicked(clone!(
            #[weak]
            dialog,
            move |_| {
                dialog.close();
            }
        ));
        close_btn.add_css_class("pill");

        let stop_btn = gtk::Button::with_label("Stop");
        stop_btn.connect_clicked(clone!(
            #[weak]
            task,
            move |_| {
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

        button_row.append(&close_btn);
        button_row.append(&stop_btn);
        content.append(&button_row);

        toolbar_view.set_content(Some(&content));

        dialog.set_child(Some(&toolbar_view));

        dialog.present(Some(self));
        dialog
    }

    fn build_welcome_dialog(&self) {
        let dialog = adw::Dialog::new();
        dialog.set_content_width(360);
        dialog.set_title("Setup");

        let toolbar_view = adw::ToolbarView::new();
        toolbar_view.add_top_bar(
            &adw::HeaderBar::builder()
            .build(),
        );

        let clamp = adw::Clamp::new();
        clamp.set_margin_top(12);
        clamp.set_margin_bottom(12);
        clamp.set_margin_start(12);
        clamp.set_margin_end(12);

        let carousel = adw::Carousel::new();
        carousel.set_vexpand(true);

        let indicator = adw::CarouselIndicatorDots::new();
        indicator.set_carousel(Some(&carousel));
        toolbar_view.add_bottom_bar(&indicator);

        let terminal_page = gtk::Box::new(gtk::Orientation::Vertical, 12);


        // Page 1: Welcome message
        if self.distrobox_service().version().error().is_some() {
            let welcome_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
            let welcome_label = gtk::Label::new(Some("Welcome to DistroHome!"));
            welcome_label.set_wrap(true);
            welcome_label.add_css_class("title-1");
            welcome_label.set_xalign(0.5);
            welcome_page.append(&welcome_label);

            let welcome_description = gtk::Label::new(Some("This application helps you manage your distroboxes easily."));
            welcome_description.set_wrap(true);
            welcome_description.set_xalign(0.5);
            welcome_page.append(&welcome_description);

            let link_button = gtk::LinkButton::with_label("https://distrobox.it/", "Learn more about Distrobox");
            link_button.set_halign(gtk::Align::Center);
            welcome_page.append(&link_button);

            let install_button = gtk::Button::with_label("Install Distrobox");
            install_button.set_halign(gtk::Align::Center);
            install_button.add_css_class("suggested-action");
            install_button.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            #[weak]
            carousel,
            #[weak]
            terminal_page,
            move |_| {
            // this.distrobox_service().install_distrobox();
            if this.distrobox_service().version().error().is_none() {
                carousel.scroll_to(&terminal_page, true);
            }
            }
            ));
            welcome_page.append(&install_button);

            carousel.append(&welcome_page);
        }

        // Page 2: Choose preferred terminal emulator
        let explanation_label = gtk::Label::new(Some("Please select your preferred terminal emulator. This will be used to open terminal sessions within your containers."));
        explanation_label.set_wrap(true);
        explanation_label.set_xalign(0.0);
        terminal_page.append(&explanation_label);

        let terminal_group = self.build_terminal_combo_row();
        terminal_page.append(&terminal_group);

        let done_button = gtk::Button::with_label("Done");
        done_button.set_halign(gtk::Align::Center);
        done_button.add_css_class("suggested-action");
        done_button.add_css_class("pill");
        
        // Enable/disable button based on terminal selection
        let terminal_valid = self.distrobox_service().selected_terminal().is_some();
        done_button.set_sensitive(terminal_valid);
        
        // Watch for terminal changes
        let done_button_clone = done_button.clone();
        self.distrobox_service().connect_selected_terminal_notify(clone!(
            #[weak]
            done_button_clone,
            move |service| {
                let valid = service.selected_terminal().is_some();
                done_button_clone.set_sensitive(valid);
            }
        ));

        done_button.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            #[weak]
            dialog,
            move |_| {
                if this.distrobox_service().selected_terminal().is_some() {
                    let this_clone = this.clone();
                    let dialog_clone = dialog.clone();
                    glib::MainContext::ref_thread_default().spawn_local(async move {
                        match this_clone.distrobox_service().validate_terminal().await {
                            Ok(_) => {
                                dialog_clone.close();
                            }
                            Err(err) => {
                                let error_dialog = adw::AlertDialog::builder()
                                    .heading("Terminal Validation Failed")
                                    .body(format!("Could not validate terminal: {}", err))
                                    .build();
                                error_dialog.add_response("ok", "OK");
                                error_dialog.present(Some(&this_clone));
                            }
                        }
                    });
                }
            }
        ));
        terminal_page.append(&done_button);

        carousel.append(&terminal_page);

        clamp.set_child(Some(&carousel));
        toolbar_view.set_content(Some(&clamp));
        dialog.set_child(Some(&toolbar_view));

        dialog.present(Some(self));
    }

    fn build_preferences_dialog(&self) {
        let dialog = adw::PreferencesDialog::new();
        dialog.set_title("Preferences");

        let page = adw::PreferencesPage::new();

        let preferences_group = adw::PreferencesGroup::new();
        preferences_group.set_title("General");

        let terminal_group = self.build_terminal_combo_row();
        page.add(&terminal_group);

        page.add(&preferences_group);
        dialog.add(&page);
        dialog.present(Some(self));
    }

    fn build_terminal_combo_row(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::new();
        group.set_title("Terminal Settings");

        let terminal_combo = adw::ComboRow::new();
        terminal_combo.set_title("Preferred Terminal");
        terminal_combo.set_use_subtitle(true);

        let terminals = SUPPORTED_TERMINALS
            .iter()
            .map(|x| x.name.as_ref())
            .collect::<Vec<&str>>();
        let selected_position = terminals.iter().position(|x| {
            Some(x)
                == self
                    .distrobox_service()
                    .selected_terminal()
                    .as_ref()
                    .map(|x| x.name.as_str())
                    .as_ref()
        });

        let terminal_list = gtk::StringList::new(&terminals);
        terminal_combo.set_model(Some(&terminal_list));
        if let Some(selected_position) = selected_position {
            terminal_combo.set_selected(selected_position as u32);
        }
        terminal_combo.connect_selected_item_notify(clone!(
            #[weak(rename_to = this)]
            self,
            #[weak]
            terminal_combo,
            move |combo| {
                let selected: gtk::StringObject = combo.selected_item().and_downcast().unwrap();
                supported_terminals::terminal_by_name(&selected.string()).map(|x| {
                    this.distrobox_service()
                        .set_selected_terminal_program(&x.program)
                });
            }
        ));

        group.add(&terminal_combo);
        group
    }

    fn build_exportable_apps_dialog(&self, box_name: &str) {
        ExportableAppsDialog::new(box_name, self.distrobox_service().clone()).present(Some(self));
    }

    fn build_upgrade_dialog(&self, box_name: &str) {
        let task = self.distrobox_service().do_upgrade(box_name);
        self.build_task_dialog(&task);
    }

    fn create_button_row(title: &str, icon_name: &str, subtitle: &str) -> adw::ActionRow {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle(subtitle);

        let icon = gtk::Image::from_icon_name(icon_name);
        row.add_prefix(&icon);

        row.set_activatable(true);

        row
    }
}
