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

use crate::container::Container;
use crate::create_distrobox_dialog::CreateDistroboxDialog;
use crate::exportable_apps_dialog::ExportableAppsDialog;
use crate::gtk_utils::reaction;
use crate::known_distros::PackageManager;
use crate::root_store::RootStore;
use crate::sidebar_row::SidebarRow;
use crate::tagged_object::TaggedObject;
use crate::task_manager_dialog::TaskManagerDialog;
use crate::tasks_button::TasksButton;
use crate::terminal_combo_row::TerminalComboRow;
use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::{derived_properties, Properties};
use gtk::gdk;
use gtk::glib::clone;
use gtk::{gio, glib, pango};
use std::cell::RefCell;
use tracing::info;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate, Properties)]
    #[properties(wrapper_type = super::DistrohomeWindow)]
    #[template(resource = "/com/ranfdev/DistroHome/window.ui")]
    pub struct DistrohomeWindow {
        #[property(get, set, construct)]
        pub root_store: RefCell<RootStore>,
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
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
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
                win.root_store().load_containers();
            });
            klass.add_binding_action(gdk::Key::F5, gdk::ModifierType::empty(), "win.refresh");

            klass.install_action("win.upgrade-all", None, |win, _action, _target| {
                win.root_store().upgrade_all();
            });

            klass.install_action("win.preferences", None, |win, _action, _target| {
                win.root_store()
                    .set_current_dialog(TaggedObject::new("preferences"));
            });

            klass.install_action("win.learn-more", None, |_win, _action, _target| {
                gtk::UriLauncher::new("https://distrobox.it").launch(
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
                win.root_store()
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
    pub fn new<P: IsA<gtk::Application>>(application: &P, root_store: RootStore) -> Self {
        let this: Self = glib::Object::builder()
            .property("application", application)
            .property("root-store", root_store)
            .build();

        let this_clone = this.clone();
        this.root_store()
            .connect_selected_container_notify(move |root_store| {
                if let Some(container) = root_store.selected_container() {
                    this_clone.build_main_content(&container);
                    this_clone.imp().split_view.set_show_content(true);
                }
            });
        let this_clone = this.clone();
        this.root_store()
            .connect_current_view_notify(move |root_store| {
                this_clone
                    .imp()
                    .main_stack
                    .set_visible_child_name(&root_store.current_view().tag());
            });
        let this_clone = this.clone();
        this.root_store()
            .connect_current_dialog_notify(move |root_store| {
                if let Some(dialog) = this_clone.current_dialog() {
                    dialog.close();
                }
                let dialog: adw::Dialog = match dbg!(root_store.current_dialog().tag().as_str()) {
                    "exportable-apps" => ExportableAppsDialog::new(
                        &this_clone.root_store().selected_container().unwrap(),
                    )
                    .upcast(),
                    "create-distrobox" => {
                        CreateDistroboxDialog::new(this_clone.root_store()).upcast()
                    }
                    "task-manager" => TaskManagerDialog::new(root_store).upcast(),
                    "preferences" => this_clone.build_preferences_dialog(),
                    _ => return,
                };
                this_clone.set_current_dialog(Some(&dialog));
                dialog.present(Some(&this_clone));
            });
        this.build_sidebar();
        this.root_store().load_containers();
        this
    }

    fn build_sidebar(&self) {
        let imp = self.imp();

        imp.sidebar_list_box
            .bind_model(Some(&self.root_store().containers()), |obj| {
                let container = obj.downcast_ref().unwrap();
                SidebarRow::new(container).upcast()
            });

        let this = self.clone();
        self.root_store().containers().connect_items_changed(
            move |list, _position, _removed, _added| {
                let visible_child_name = if list.n_items() == 0 {
                    "no-distroboxes"
                } else {
                    "distroboxes"
                };
                this.imp()
                    .sidebar_stack
                    .set_visible_child_name(visible_child_name);
            },
        );
        let this = self.clone();
        imp.sidebar_list_box.connect_row_activated(move |_, row| {
            let index = row.index();
            let item = this.root_store().containers().item(index as u32);
            let selected_container: &Container = item.and_downcast_ref().unwrap();
            this.root_store()
                .set_selected_container(Some(selected_container.clone()));
        });

        // Add tasks button to the bottom of the sidebar
        let tasks_button = TasksButton::new(&self.root_store());
        tasks_button.add_css_class("flat");
        self.imp()
            .sidebar_bottom_slot
            .set_child(Some(&tasks_button));
    }

    pub fn build_container_header(&self, container: &Container) -> gtk::Box {
        // Create labels for the title and subtitle
        let title_label = gtk::Label::new(Some(&container.name()));
        title_label.set_xalign(0.0);
        title_label.add_css_class("title-1");

        // Create image URL label
        let subtitle_label = gtk::Label::new(Some(&container.image()));
        subtitle_label.set_xalign(0.0);
        subtitle_label.set_ellipsize(pango::EllipsizeMode::Middle);
        subtitle_label.add_css_class("subtitle");

        // Create a copy button next to the image URL label
        let copy_btn = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy_btn.set_tooltip_text(Some("Copy image URL"));
        copy_btn.add_css_class("flat");
        copy_btn.add_css_class("xs");
        // Capture a clone of the image URL for the closure
        let image_url = container.image().to_string();
        let toast_overlay = self.imp().toast_overlay.clone();
        copy_btn.connect_clicked(move |_| {
            if let Some(display) = gdk::Display::default() {
                let clipboard = display.primary_clipboard();
                clipboard.set_text(&image_url);
                toast_overlay.add_toast(adw::Toast::new("Image URL copied"));
            }
        });

        // Create a horizontal box that contains the subtitle label and copy button
        let subtitle_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        subtitle_box.append(&subtitle_label);
        subtitle_box.append(&copy_btn);

        // Create a vertical box and add the title and the new subtitle_box
        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        text_box.append(&title_label);
        text_box.append(&subtitle_box);

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
        stop_btn.connect_clicked(clone!(
            #[weak(rename_to=this)]
            self,
            move |_| {
                this.root_store().selected_container().unwrap().stop();
            }
        ));
        status_child.append(&stop_btn);

        let terminal_btn = gtk::Button::from_icon_name("terminal-symbolic");
        terminal_btn.set_tooltip_text(Some("Open Terminal"));
        terminal_btn.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.root_store()
                    .selected_container()
                    .unwrap()
                    .spawn_terminal();
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
                let task = this.root_store().selected_container().unwrap().upgrade();
                this.root_store().view_task(&task);
            }
        ));

        apps_row.connect_activated(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.root_store().view_exportable_apps();
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
                        this.root_store()
                            .selected_container()
                            .unwrap()
                            .clone_to(&entry.text());
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
                        this.root_store()
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
                            this.root_store().selected_container().unwrap().delete();
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
        if let Some(container) = self.root_store().selected_container() {
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
                                this.root_store().selected_container().unwrap()
                                    .install(&path);
                            }
                        }
                    }
                ),
            );
        }
    }

    fn build_preferences_dialog(&self) -> adw::Dialog {
        let dialog = adw::PreferencesDialog::new();
        dialog.set_title("Preferences");

        let page = adw::PreferencesPage::new();

        let preferences_group = adw::PreferencesGroup::new();
        preferences_group.set_title("General");

        let terminal_group = adw::PreferencesGroup::new();
        terminal_group.set_title("Terminal Settings");
        terminal_group.add(&TerminalComboRow::new_with_params(self.root_store()));
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
