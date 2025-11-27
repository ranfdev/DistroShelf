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
use crate::dialogs::{
    CreateDistroboxDialog, ExportableAppsDialog, PreferencesDialog, TaskManagerDialog,
};
use crate::root_store::RootStore;
use crate::sidebar_row::SidebarRow;
use crate::tagged_object::TaggedObject;
use crate::tasks_button::TasksButton;
use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::{Properties, derived_properties};
use gtk::gio::ActionEntry;
use gtk::glib::clone;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use tracing::info;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate, Properties)]
    #[properties(wrapper_type = super::DistroShelfWindow)]
    #[template(resource = "/com/ranfdev/DistroShelf/gtk/window.ui")]
    pub struct DistroShelfWindow {
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
        pub sidebar_list_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub content_page: TemplateChild<adw::NavigationPage>,
        #[template_child]
        pub split_view: TemplateChild<adw::NavigationSplitView>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub welcome_view: TemplateChild<crate::welcome_view::WelcomeView>,
        #[template_child]
        pub view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub view_switcher: TemplateChild<adw::ViewSwitcher>,
        #[template_child]
        pub view_switcher_bar: TemplateChild<adw::ViewSwitcherBar>,
        #[template_child]
        pub container_overview: TemplateChild<crate::widgets::ContainerOverview>,
        #[template_child]
        pub integrated_terminal: TemplateChild<crate::widgets::IntegratedTerminal>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DistroShelfWindow {
        const NAME: &'static str = "DistroShelfWindow";
        type Type = super::DistroShelfWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.add_binding_action(gdk::Key::F5, gdk::ModifierType::empty(), "win.refresh");
            // klass.add_binding_action(gdk::Key::N, gdk::ModifierType::CONTROL_MASK, "win.create-distrobox");
            klass.add_binding_action(
                gdk::Key::U,
                gdk::ModifierType::CONTROL_MASK,
                "win.upgrade-container",
            );
            klass.add_binding_action(
                gdk::Key::U,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK,
                "win.upgrade-all",
            );
            klass.add_binding_action(
                gdk::Key::I,
                gdk::ModifierType::CONTROL_MASK,
                "win.install-package",
            );
            klass.add_binding_action(
                gdk::Key::comma,
                gdk::ModifierType::CONTROL_MASK,
                "win.preferences",
            );
            klass.add_binding_action(
                gdk::Key::L,
                gdk::ModifierType::CONTROL_MASK,
                "win.command-log",
            );
            klass.add_binding_action(
                gdk::Key::T,
                gdk::ModifierType::CONTROL_MASK,
                "win.open-terminal",
            );
            klass.add_binding_action(
                gdk::Key::D,
                gdk::ModifierType::CONTROL_MASK,
                "win.clone-container",
            );
            klass.add_binding_action(
                gdk::Key::E,
                gdk::ModifierType::CONTROL_MASK,
                "win.view-exportable-apps",
            );
            klass.add_binding_action(
                gdk::Key::Delete,
                gdk::ModifierType::CONTROL_MASK,
                "win.delete-container",
            );
            klass.add_binding_action(
                gdk::Key::S,
                gdk::ModifierType::CONTROL_MASK,
                "win.stop-container",
            );
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[derived_properties]
    impl ObjectImpl for DistroShelfWindow {}
    impl WidgetImpl for DistroShelfWindow {}
    impl WindowImpl for DistroShelfWindow {}
    impl ApplicationWindowImpl for DistroShelfWindow {}
    impl AdwApplicationWindowImpl for DistroShelfWindow {}
}

glib::wrapper! {
    pub struct DistroShelfWindow(ObjectSubclass<imp::DistroShelfWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow, gtk::ShortcutManager, gtk::Root, gtk::Native,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}

impl DistroShelfWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P, root_store: RootStore) -> Self {
        let this: Self = glib::Object::builder()
            .property("application", application)
            .property("root-store", root_store)
            .build();

        // Restore window size from settings
        let settings = gio::Settings::new("com.ranfdev.DistroShelf");
        let width = settings.int("window-width");
        let height = settings.int("window-height");
        this.set_default_size(width, height);

        this.setup_gactions();
        let this_clone = this.clone();
        this.root_store()
            .selected_container_model()
            .connect_selected_notify(move |model| {
                if let Some(container) = model
                    .selected_item()
                    .and_then(|obj| obj.downcast::<Container>().ok())
                {
                    this_clone.update_container(&container);
                    this_clone.imp().split_view.set_show_content(true);
                } else {
                    this_clone
                        .imp()
                        .content_page
                        .set_child(None::<&gtk::Widget>);
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
                    if dialog.parent().is_some() {
                        dialog.close();
                    }
                }
                let dialog: adw::Dialog = match root_store.current_dialog().tag().as_str() {
                    "exportable-apps" => ExportableAppsDialog::new(
                        &this_clone.root_store().selected_container().unwrap(),
                    )
                    .upcast(),
                    "create-distrobox" => {
                        CreateDistroboxDialog::new(this_clone.root_store()).upcast()
                    }
                    "task-manager" => TaskManagerDialog::new(root_store).upcast(),
                    "preferences" => this_clone.build_preferences_dialog(),
                    "command-log" => this_clone.build_command_log_dialog(),
                    _ => return,
                };
                this_clone.set_current_dialog(Some(&dialog));
                dialog.present(Some(&this_clone));
            });
        this.build_sidebar();
        this.root_store().load_containers();

        // Save window size when closing
        let this_clone = this.clone();
        this.connect_close_request(move |_| {
            let (width, height) = this_clone.default_size();
            let settings = gio::Settings::new("com.ranfdev.DistroShelf");
            let _ = settings.set_int("window-width", width);
            let _ = settings.set_int("window-height", height);
            glib::Propagation::Proceed
        });

        this
    }

    fn setup_gactions(&self) {
        let a = ActionEntry::builder;

        let actions = [
            a("refresh").activate(move |this: &DistroShelfWindow, _, _| {
                this.root_store().load_containers();
            }),
            a("upgrade-all").activate(move |this: &DistroShelfWindow, _, _| {
                this.root_store().upgrade_all();
            }),
            a("preferences").activate(|this, _, _| {
                this.root_store()
                    .set_current_dialog(TaggedObject::new("preferences"));
            }),
            a("learn-more").activate(|_, _, _| {
                gtk::UriLauncher::new("https://distrobox.it").launch(
                    None::<&gtk::Window>,
                    None::<&gio::Cancellable>,
                    |res| {
                        if let Err(e) = res {
                            tracing::error!(error = %e, "Failed to open Distrobox website");
                        }
                    },
                );
            }),
            a("create-distrobox").activate(|this, _, _| {
                this.root_store()
                    .set_current_dialog(TaggedObject::new("create-distrobox"));
            }),
            a("command-log").activate(|this, _, _| {
                this.root_store()
                    .set_current_dialog(TaggedObject::new("command-log"));
            }),
            a("clone-container").activate(|this, _, _| {
                let dialog = CreateDistroboxDialog::new(this.root_store());
                if let Some(src) = this.root_store().selected_container() {
                    dialog.set_clone_src(Some(src));
                }
                dialog.present(Some(this));
            }),
            a("upgrade-container").activate(|this, _, _| {
                let task = this.root_store().selected_container().unwrap().upgrade();
                this.root_store().view_task(&task);
            }),
            a("view-exportable-apps").activate(|this, _, _| {
                this.root_store().view_exportable_apps();
            }),
            a("install-package").activate(|this, _, _| {
                this.build_install_package_dialog();
            }),
            a("stop-container").activate(|this, _, _| {
                this.root_store().selected_container().unwrap().stop();
            }),
            a("delete-container").activate(|this, _, _| {
                this.build_delete_dialog();
            }),
            a("open-terminal").activate(|this, _, _| {
                this.open_terminal();
            }),
        ];
        self.add_action_entries(actions.into_iter().map(|entry| entry.build()));
    }
    fn build_sidebar(&self) {
        let imp = self.imp();

        let selection_model = self.root_store().selected_container_model();

        // Create a factory for creating and binding sidebar rows
        let factory = gtk::SignalListItemFactory::new();

        factory.connect_setup(|_factory, item| {
            let list_item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let sidebar_row = SidebarRow::new(&Container::default());
            list_item.set_child(Some(&sidebar_row));
        });

        factory.connect_bind(|_factory, item| {
            let list_item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let container = list_item
                .item()
                .and_then(|obj| obj.downcast::<Container>().ok())
                .unwrap();
            let sidebar_row = list_item
                .child()
                .and_then(|child| child.downcast::<SidebarRow>().ok())
                .unwrap();
            sidebar_row.set_container(&container);
        });

        imp.sidebar_list_view.set_factory(Some(&factory));
        imp.sidebar_list_view.set_model(Some(&selection_model));
        let this = self.clone();
        self.root_store()
            .containers()
            .inner()
            .connect_items_changed(move |list, _position, _removed, _added| {
                let visible_child_name = if list.n_items() == 0 {
                    "no-distroboxes"
                } else {
                    "distroboxes"
                };
                this.imp()
                    .sidebar_stack
                    .set_visible_child_name(visible_child_name);
            });

        // Add tasks button to the bottom of the sidebar
        let tasks_button = TasksButton::new(&self.root_store());
        tasks_button.add_css_class("flat");
        self.imp()
            .sidebar_bottom_slot
            .set_child(Some(&tasks_button));
    }

    pub fn add_toast(&self, toast: adw::Toast) {
        self.imp().toast_overlay.add_toast(toast);
    }
    
    fn open_terminal(&self) {
        let task = self
            .root_store()
            .selected_container()
            .unwrap()
            .spawn_terminal();
        let this = self.clone();
        task.connect_status_notify(move |task| {
            if let Some(_) = &*task.error() {
                let toast = adw::Toast::new("Check your terminal settings.");
                toast.set_button_label(Some("Preferences"));
                toast.connect_button_clicked(clone!(
                    #[weak]
                    this,
                    move |_| {
                        this.root_store()
                            .set_current_dialog(TaggedObject::new("preferences"));
                    }
                ));
                this.add_toast(toast);
            }
        });
    }

    fn build_delete_dialog(&self) {
        let dialog = adw::AlertDialog::builder()
            .heading("Delete this container?")
            .body(format!(
                "{} will be deleted.\nThis action cannot be undone.",
                self.root_store()
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
                self,
                move |dialog, _| {
                    this.root_store().selected_container().unwrap().delete();
                    this.root_store()
                        .selected_container_model()
                        .set_selected(gtk::INVALID_LIST_POSITION);
                    dialog.close();
                }
            ),
        );

        dialog.present(Some(self));
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
        PreferencesDialog::new(self.root_store()).upcast()
    }

    fn build_command_log_dialog(&self) -> adw::Dialog {
        let dialog = adw::Dialog::new();
        dialog.set_title("Command Log");
        dialog.set_content_width(800);
        dialog.set_content_height(600);

        // Create toolbar view
        let toolbar_view = adw::ToolbarView::new();

        // Create header bar
        let header_bar = adw::HeaderBar::new();
        header_bar.set_title_widget(Some(&adw::WindowTitle::new("Command Log", "")));

        toolbar_view.add_top_bar(&header_bar);

        let toast_overlay = adw::ToastOverlay::new();

        // Create main content
        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

        // Create scrolled window for command list
        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled_window.set_vexpand(true);

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content_box.set_margin_start(12);
        content_box.set_margin_end(12);
        content_box.set_margin_top(12);
        content_box.set_margin_bottom(12);

        let description = gtk::Label::new(Some(
            "Executing a single task (eg: listing applications) may require multiple chains of commands. \
            For debugging purposes, this log shows all commands executed by the application.",
        ));

        description.set_wrap(true);
        description.set_xalign(0.0);
        description.add_css_class("dim-label");

        content_box.append(&description);

        // Create list box to show all commands
        let list_box = gtk::ListBox::new();
        list_box.set_selection_mode(gtk::SelectionMode::None);

        // Get command events from output tracker
        let command_events = self.root_store().command_runner().output_tracker().items();

        for event in command_events {
            match event {
                crate::fakers::CommandRunnerEvent::Spawned(id, command) => {
                    let row = gtk::ListBoxRow::new();
                    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    row_box.set_margin_start(6);
                    row_box.set_margin_end(6);
                    row_box.set_margin_top(3);
                    row_box.set_margin_bottom(3);

                    let status_icon = gtk::Image::from_icon_name("media-playback-start-symbolic");
                    status_icon.add_css_class("spawned");
                    status_icon.set_pixel_size(12);

                    let label_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
                    let title_label = gtk::Label::new(Some(&format!("Spawned [{}]", id)));
                    title_label.set_xalign(0.0);
                    title_label.add_css_class("caption");

                    let subtitle_label = gtk::Label::new(Some(&command.to_string()));
                    subtitle_label.set_xalign(0.0);
                    subtitle_label.add_css_class("caption");
                    subtitle_label.add_css_class("dim-label");
                    subtitle_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

                    label_box.append(&title_label);
                    label_box.append(&subtitle_label);

                    row_box.append(&status_icon);
                    row_box.append(&label_box);
                    row.set_child(Some(&row_box));

                    // Add click handler to copy command to clipboard
                    let command_str = command.to_string();
                    let gesture = gtk::GestureClick::new();
                    gesture.connect_pressed(clone!(
                        #[weak]
                        toast_overlay,
                        move |_, _, _, _| {
                            if let Some(display) = gdk::Display::default() {
                                let clipboard = display.clipboard();
                                clipboard.set_text(&command_str);
                                toast_overlay
                                    .add_toast(adw::Toast::new("Command copied to clipboard"));
                            }
                        }
                    ));
                    row.add_controller(gesture);

                    list_box.append(&row);
                }
                crate::fakers::CommandRunnerEvent::Started(id, command) => {
                    let row = gtk::ListBoxRow::new();
                    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    row_box.set_margin_start(6);
                    row_box.set_margin_end(6);
                    row_box.set_margin_top(3);
                    row_box.set_margin_bottom(3);

                    let status_icon = gtk::Image::from_icon_name("system-run-symbolic");
                    status_icon.add_css_class("started");
                    status_icon.set_pixel_size(12);

                    let label_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
                    let title_label = gtk::Label::new(Some(&format!("Started [{}]", id)));
                    title_label.set_xalign(0.0);
                    title_label.add_css_class("caption");

                    let subtitle_label = gtk::Label::new(Some(&command.to_string()));
                    subtitle_label.set_xalign(0.0);
                    subtitle_label.add_css_class("caption");
                    subtitle_label.add_css_class("dim-label");
                    subtitle_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

                    label_box.append(&title_label);
                    label_box.append(&subtitle_label);

                    row_box.append(&status_icon);
                    row_box.append(&label_box);
                    row.set_child(Some(&row_box));

                    // Add click handler to copy command to clipboard
                    let command_str = command.to_string();
                    let gesture = gtk::GestureClick::new();
                    gesture.connect_pressed(clone!(
                        #[weak]
                        toast_overlay,
                        move |_, _, _, _| {
                            if let Some(display) = gdk::Display::default() {
                                let clipboard = display.clipboard();
                                clipboard.set_text(&command_str);
                                toast_overlay
                                    .add_toast(adw::Toast::new("Command copied to clipboard"));
                            }
                        }
                    ));
                    row.add_controller(gesture);

                    list_box.append(&row);
                }
                crate::fakers::CommandRunnerEvent::Output(id, result) => {
                    let row = gtk::ListBoxRow::new();
                    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    row_box.set_margin_start(6);
                    row_box.set_margin_end(6);
                    row_box.set_margin_top(3);
                    row_box.set_margin_bottom(3);

                    let (title, icon, css_class) = match result {
                        Ok(_) => (
                            format!("Completed [{}]", id),
                            "object-select-symbolic",
                            "success",
                        ),
                        Err(_) => (format!("Failed [{}]", id), "dialog-error-symbolic", "error"),
                    };

                    let status_icon = gtk::Image::from_icon_name(icon);
                    status_icon.add_css_class(css_class);
                    status_icon.set_pixel_size(12);

                    let title_label = gtk::Label::new(Some(&title));
                    title_label.set_xalign(0.0);
                    title_label.add_css_class("caption");

                    row_box.append(&status_icon);
                    row_box.append(&title_label);
                    row.set_child(Some(&row_box));

                    list_box.append(&row);
                }
            }
        }

        content_box.append(&list_box);
        scrolled_window.set_child(Some(&content_box));
        main_box.append(&scrolled_window);

        toast_overlay.set_child(Some(&main_box));
        toolbar_view.set_content(Some(&toast_overlay));
        dialog.set_child(Some(&toolbar_view));

        dialog.upcast()
    }

    fn update_container(&self, container: &Container) {
        let imp = self.imp();
        
        // Set container on the template widgets
        imp.container_overview.set_container(container);
        imp.integrated_terminal.set_container(container);
        
        // Spawn terminal when view becomes visible
        imp.view_stack.connect_visible_child_notify(clone!(
            #[weak(rename_to = terminal)]
            imp.integrated_terminal,
            move |stack| {
                if stack.visible_child_name().as_deref() == Some("terminal") {
                    terminal.spawn_terminal();
                }
            }
        ));
    }
}
