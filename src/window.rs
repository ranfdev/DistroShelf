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

use crate::app_view_model::AppViewModel;
use crate::container::Container;
use crate::create_distrobox_dialog::CreateDistroboxDialog;
use crate::distrobox_service::DistroboxService;
use crate::distrobox_task::DistroboxTask;
use crate::exportable_apps_dialog::ExportableAppsDialog;
use crate::gtk_utils::reaction;
use crate::known_distros::PackageManager;
use crate::sidebar_row::SidebarRow;
use crate::tasks_button::TasksButton;
use crate::terminal_combo_row::TerminalComboRow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::clone;
use gtk::{gio, glib, pango};
use tracing::info;

mod imp {
    use std::cell::RefCell;

    use glib::{derived_properties, Properties};
    use gtk::gdk;

    use crate::{
        app_view_model::AppViewModel, distrobox_service::DistroboxService, resource::Resource,
        tagged_object::TaggedObject,
    };

    use super::*;

    #[derive(Default, gtk::CompositeTemplate, Properties)]
    #[properties(wrapper_type = super::DistrohomeWindow)]
    #[template(resource = "/com/ranfdev/DistroHome/window.ui")]
    pub struct DistrohomeWindow {
        #[property(get, set)]
        pub app_view_model: RefCell<AppViewModel>,
        #[property(get, set, nullable)]
        pub current_dialog: RefCell<Option<adw::Dialog>>,

        // Template widgets
        #[template_child]
        pub sidebar_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub create_distrobox_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub sidebar_bottom_slot: TemplateChild<adw::Bin>,
        #[template_child]
        pub sidebar_list_box: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub main_slot: TemplateChild<adw::Bin>,
        #[template_child]
        pub split_view: TemplateChild<adw::NavigationSplitView>,
        #[template_child]
        pub welcome_view: TemplateChild<crate::welcome_view::WelcomeView>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DistrohomeWindow {
        const NAME: &'static str = "DistrohomeWindow";
        type Type = super::DistrohomeWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("win.refresh", None, |win, _action, _target| {
                win.app_view_model()
                    .distrobox_service()
                    .load_container_infos();
            });
            klass.add_binding_action(gdk::Key::F5, gdk::ModifierType::empty(), "win.refresh");

            klass.install_action("win.upgrade-all", None, |win, _action, _target| {
                win.app_view_model().distrobox_service().do_upgrade_all();
            });

            klass.install_action("win.preferences", None, |win, _action, _target| {
                win.app_view_model()
                    .set_current_dialog(TaggedObject::new("preferences"));
            });

            klass.install_action("win.learn-more", None, |_win, _action, _target| {
                gtk::UriLauncher::new(&"https://distrobox.it").launch(
                    None::<&gtk::Window>,
                    None::<&gio::Cancellable>,
                    |res| {
                        if let Err(e) = res {
                            tracing::error!(error = %e, "Failed to open Distrobox website");
                        }
                    },
                );
            });

            klass.install_action("win.create-distrobox", None, |win, _action, _target| {
                win.app_view_model()
                    .set_current_dialog(TaggedObject::new("create-distrobox"));
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[derived_properties]
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
    pub fn new<P: IsA<gtk::Application>>(
        application: &P,
        distrobox_service: DistroboxService,
        view_model: AppViewModel,
    ) -> Self {
        let this: Self = glib::Object::builder()
            .property("application", application)
            .property("app-view-model", view_model)
            .build();

        let this_clone = this.clone();
        this.app_view_model()
            .connect_selected_container_notify(move |model| {
                if let Some(container) = model.selected_container() {
                    this_clone.build_main_content(&container);
                    this_clone.imp().split_view.set_show_content(true);
                }
            });
        let this_clone = this.clone();
        this.app_view_model()
            .connect_current_view_notify(move |model| {
                match model.current_view().tag().as_str() {
                    "welcome" => {
                        this_clone
                            .imp()
                            .welcome_view
                            .set_model(model.current_view().object().and_downcast_ref().unwrap());
                    }
                    _ => {}
                }
                this_clone
                    .imp()
                    .main_stack
                    .set_visible_child_name(&model.current_view().tag());
            });
        let this_clone = this.clone();
        this.app_view_model()
            .connect_current_dialog_notify(move |model| {
                if let Some(dialog) = this_clone.current_dialog() {
                    dialog.close();
                }
                let dialog: adw::Dialog = match dbg!(model.current_dialog().tag().as_str()) {
                    "exportable-apps" => ExportableAppsDialog::new(
                        model.current_dialog().object().and_downcast_ref().unwrap(),
                    )
                    .upcast(),
                    "create-distrobox" => {
                        CreateDistroboxDialog::new(this_clone.app_view_model()).upcast()
                    }
                    "task" => this_clone.build_task_dialog(
                        model.current_dialog().object().and_downcast_ref().unwrap(),
                    ),
                    "preferences" => this_clone.build_preferences_dialog(),
                    _ => {
                        panic!("invalid dialog tag");
                    }
                };
                this_clone.set_current_dialog(Some(&dialog));
                dialog.present(Some(&this_clone));
            });
        this.build_sidebar();
        this
    }

    fn build_sidebar(&self) {
        let imp = self.imp();
        let this = self.clone();

        imp.sidebar_list_box
            .bind_model(Some(&self.app_view_model().containers()), |obj| {
                let container = obj.downcast_ref().unwrap();
                SidebarRow::new(container).upcast()
            });

        let this = self.clone();
        self.app_view_model()
            .connect_current_sidebar_view_notify(move |_| {
                this.imp()
                    .sidebar_stack
                    .set_visible_child_name(&this.app_view_model().current_sidebar_view());
            });
        let this = self.clone();
        imp.sidebar_list_box.connect_row_activated(move |_, row| {
            let index = row.index();
            let item = this.app_view_model().containers().item(index as u32);
            let selected_container: &Container = item.and_downcast_ref().unwrap();
            this.app_view_model()
                .set_selected_container(Some(selected_container.clone()));
        });

        // Add tasks button to the bottom of the sidebar
        let tasks_button = TasksButton::new(self.app_view_model());
        tasks_button.add_css_class("flat");
        self.imp()
            .sidebar_bottom_slot
            .set_child(Some(&tasks_button));

        let tasks_button_clone = tasks_button.clone();
        self.app_view_model()
            .distrobox_service()
            .connect_tasks_changed(move |service| {
                let tasks = service.tasks();
                tasks_button_clone.update_tasks(tasks);
            });
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
            header_box.add_css_class(&distro.name());
            icon.set_icon_name(Some(&distro.name()));
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

        let status_child = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        status_child.set_valign(gtk::Align::Center);

        let stop_btn = gtk::Button::from_icon_name("media-playback-stop-symbolic");
        stop_btn.set_tooltip_text(Some("Stop"));
        let container_name = container.name().clone();
        stop_btn.connect_clicked(clone!(
            #[weak(rename_to=this)]
            self,
            move |_| {
                this.app_view_model()
                    .distrobox_service()
                    .do_stop(&container_name);
            }
        ));
        status_child.append(&stop_btn);

        let terminal_btn = gtk::Button::from_icon_name("terminal-symbolic");
        terminal_btn.set_tooltip_text(Some("Open Terminal"));
        terminal_btn.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            #[strong]
            container,
            move |_| {
                this.app_view_model()
                    .distrobox_service()
                    .do_spawn_terminal(&container.name());
            }
        ));
        status_child.append(&terminal_btn);

        status_row.add_suffix(&status_child);
        status_group.add(&status_row);

        reaction! {
            (container.status_detail(), container.status_tag()),
            move |(detail, tag): (String, String)| {
                let text = format!("{tag}: {detail}");
                status_row.set_subtitle(&text);
                stop_btn.set_visible(tag == "up");
            }
        };

        // Quick Actions Group
        let actions_group = adw::PreferencesGroup::new();
        actions_group.set_title("Quick Actions");

        let upgrade_row = Self::create_button_row(
            "Upgrade Container",
            "software-update-available-symbolic",
            "Update all packages",
        );
        actions_group.add(&upgrade_row);

        let apps_row = Self::create_button_row(
            "Applications",
            "view-list-bullet-symbolic",
            "Manage exportable applications",
        );
        actions_group.add(&apps_row);

        if let Some(distro) = container.distro() {
            let pm = distro.package_manager();
            if pm != PackageManager::Unknown {
                let install_package_row = Self::create_button_row(
                    &format!("Install {} Package", pm.installable_file().unwrap()),
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
                this.app_view_model().upgrade_container();
            }
        ));

        apps_row.connect_activated(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.app_view_model().view_exportable_apps();
            }
        ));

        clone_row.connect_activated(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let dialog = adw::Dialog::new();
                dialog.set_title("Clone Container");

                let toolbar_view = adw::ToolbarView::new();
                toolbar_view.add_top_bar(&adw::HeaderBar::new());

                let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
                content.set_margin_start(12);
                content.set_margin_end(12);
                content.set_margin_top(12);
                content.set_margin_bottom(12);

                let info_label =
                    gtk::Label::new(Some("Cloning a container may take several minutes."));
                info_label.add_css_class("dim-label");
                info_label.set_wrap(true);
                content.append(&info_label);

                let group = adw::PreferencesGroup::new();
                let entry = adw::EntryRow::builder().title("New container name").build();
                group.add(&entry);

                content.append(&group);

                let button_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                button_box.set_homogeneous(true);

                let cancel_btn = gtk::Button::with_label("Cancel");
                cancel_btn.add_css_class("pill");
                let clone_btn = gtk::Button::with_label("Clone");
                clone_btn.add_css_class("suggested-action");
                clone_btn.add_css_class("pill");

                button_box.append(&cancel_btn);
                button_box.append(&clone_btn);
                content.append(&button_box);

                toolbar_view.set_content(Some(&content));
                dialog.set_child(Some(&toolbar_view));

                cancel_btn.connect_clicked(clone!(
                    #[weak]
                    dialog,
                    move |_| {
                        dialog.close();
                    }
                ));

                clone_btn.connect_clicked(clone!(
                    #[weak(rename_to = this)]
                    this,
                    #[weak]
                    entry,
                    move |_| {
                        this.app_view_model().clone_container(&entry.text());
                    }
                ));

                dialog.present(Some(&this));
            }
        ));

        delete_row.connect_activated(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let dialog = adw::AlertDialog::builder()
                    .heading("Delete this container?")
                    .body(format!(
                        "{} will be deleted.\nThis action cannot be undone.",
                        this.app_view_model()
                            .selected_container_name()
                            .unwrap_or_default()
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
                            if let Some(ref name) = this.app_view_model().selected_container_name()
                            {
                                this.app_view_model().distrobox_service().do_delete(name);
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

    fn build_install_package_dialog(&self) {
        if let Some(container) = self.app_view_model().selected_container() {
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
                                info!(container = %container.name(), path = %path.display(), "Installing package into container");
                                this.app_view_model().distrobox_service()
                                    .do_install(&container.name(), &path);
                            }
                        }
                    }
                ),
            );
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
                tracing::error!(task = %task.name(), "Task failed: {}", error);
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
                tracing::warn!(task_id = %task.name(), "Stop requested but not implemented yet");
                // TODO: implement this
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

    fn build_preferences_dialog(&self) -> adw::Dialog {
        let dialog = adw::PreferencesDialog::new();
        dialog.set_title("Preferences");

        let page = adw::PreferencesPage::new();

        let preferences_group = adw::PreferencesGroup::new();
        preferences_group.set_title("General");

        let terminal_group = adw::PreferencesGroup::new();
        terminal_group.set_title("Terminal Settings");
        terminal_group.add(&TerminalComboRow::new_with_params(self.app_view_model()));
        page.add(&terminal_group);

        page.add(&preferences_group);
        dialog.add(&page);
        dialog.upcast()
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
