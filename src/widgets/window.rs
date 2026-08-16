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

use crate::dialogs::{
    CommandLogDialog, CreateDistroboxDialog, ExportableAppsDialog, PreferencesDialog,
    TaskManagerDialog,
};
use crate::i18n::{gettext, ngettext};
use crate::models::Container;
use crate::models::ContainerSortKey;
use crate::models::DialogParams;
use crate::models::DialogType;
use crate::root_store::RootStore;
use crate::widgets::{IntegratedTerminal, SidebarRow, TasksButton};
use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::Properties;
use glib::derived_properties;
use gtk::gio::ActionEntry;
use gtk::glib::clone;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate, Properties)]
    #[properties(wrapper_type = super::DistroShelfWindow)]
    #[template(file = "window.ui")]
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
        pub content_state_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub split_view: TemplateChild<adw::NavigationSplitView>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub migration_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub welcome_view: TemplateChild<crate::widgets::WelcomeView>,
        #[template_child]
        pub view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub view_switcher: TemplateChild<adw::ViewSwitcher>,
        #[template_child]
        pub view_switcher_bar: TemplateChild<adw::ViewSwitcherBar>,
        #[template_child]
        pub overview_bin: TemplateChild<adw::Bin>,
        #[template_child]
        pub terminal_bin: TemplateChild<adw::Bin>,
        pub terminals_by_container: RefCell<HashMap<String, IntegratedTerminal>>,
        pub sort_key_handler_id: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DistroShelfWindow {
        const NAME: &'static str = "DistroShelfWindow";
        type Type = super::DistroShelfWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
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
        this.setup();
        this
    }

    /// Builds the window without attaching it to a `GtkApplication`.
    /// Used by end-to-end tests, where no application main loop is running.
    #[cfg(test)]
    pub fn new_unattached(root_store: RootStore) -> Self {
        let this: Self = glib::Object::builder()
            .property("root-store", root_store)
            .build();
        this.setup();
        this
    }

    // End-to-end tests drive the window's widgets through these typed
    // accessors, so they never walk the widget tree; the tree itself stays
    // private to the window.
    /// The stack switching between the main and welcome views.
    #[cfg(test)]
    pub fn main_stack(&self) -> gtk::Stack {
        self.imp().main_stack.clone()
    }

    /// The stack switching between the overview and terminal pages.
    #[cfg(test)]
    pub fn view_stack(&self) -> adw::ViewStack {
        self.imp().view_stack.clone()
    }

    /// The vte terminal embedded in the terminal page, if a container
    /// is selected.
    #[cfg(test)]
    pub fn embedded_terminal(&self) -> Option<vte4::Terminal> {
        let terminal = self
            .imp()
            .terminal_bin
            .child()?
            .downcast::<IntegratedTerminal>()
            .ok()?;
        Some(terminal.terminal())
    }

    fn setup(&self) {
        let this = self.clone();

        // Restore window size from settings
        let settings = this.root_store().settings();
        let width = settings.int("window-width");
        let height = settings.int("window-height");
        this.set_default_size(width, height);

        this.setup_gactions();
        this.imp()
            .content_state_stack
            .set_visible_child_name("no_content");

        // Bind main-store changes for sidebar/model rebinding
        this.bind_main_store();
        let this_clone = this.clone();
        this.root_store()
            .connect_main_store_notify(move |_root_store| {
                this_clone.bind_main_store();
            });

        let this_clone = this.clone();
        this.root_store()
            .connect_current_view_notify(move |root_store| {
                this_clone
                    .imp()
                    .main_stack
                    .set_visible_child_name(root_store.current_view().as_str());
            });
        let this_clone = this.clone();
        this.root_store()
            .connect_current_dialog_notify(move |root_store| {
                if let Some(dialog) = this_clone.current_dialog()
                    && dialog.parent().is_some()
                {
                    dialog.close();
                }
                // Take dialog params (consumes them, resetting to default)
                let params = root_store.take_dialog_params();

                let dialog: adw::Dialog = match root_store.current_dialog() {
                    DialogType::ExportableApps => {
                        let Some(container) = this_clone.root_store().selected_container() else {
                            return;
                        };
                        ExportableAppsDialog::new(root_store, &container).upcast()
                    }
                    DialogType::CreateDistrobox => {
                        let dialog = CreateDistroboxDialog::new(
                            this_clone.root_store(),
                            params.clone_source,
                        );
                        dialog.upcast()
                    }
                    DialogType::TaskManager => TaskManagerDialog::new(root_store).upcast(),
                    DialogType::Preferences => {
                        PreferencesDialog::new(this_clone.root_store()).upcast()
                    }
                    DialogType::CommandLog => {
                        CommandLogDialog::new(this_clone.root_store()).upcast()
                    }
                    DialogType::None => return,
                };
                this_clone.set_current_dialog(Some(&dialog));
                dialog.present(Some(&this_clone));
            });
        this.build_sidebar();
        this.build_migration_banner();
        this.root_store().load_containers();

        // Refresh data when the window regains focus
        if let Some(m) = this.root_store().main_store() {
            m.containers_query()
                .refetch_on_focus(&this, Duration::from_secs(5));
        }
        this.root_store()
            .container_runtime()
            .refetch_on_focus(&this, Duration::from_secs(30));

        // Register terminal visibility callback once
        let this_clone = this.clone();
        this.imp()
            .view_stack
            .connect_visible_child_notify(move |stack| {
                if stack.visible_child_name().as_deref() == Some("terminal") {
                    if let Some(terminal) = this_clone
                        .imp()
                        .terminal_bin
                        .child()
                        .and_then(|w| w.downcast::<IntegratedTerminal>().ok())
                    {
                        terminal.spawn_terminal();
                        terminal.focus_terminal();
                    }
                } else {
                    this_clone.root_store().enable_shortcuts();
                }
            });

        // Save window size when closing
        let this_clone = this.clone();
        this.connect_close_request(move |_| {
            let (width, height) = this_clone.default_size();
            let settings = this_clone.root_store().settings();
            let _ = settings.set_int("window-width", width);
            let _ = settings.set_int("window-height", height);
            glib::Propagation::Proceed
        });
    }

    /// (Re-)binds sidebar, selection tracking, stale-banner, and focus refetch
    /// when `main_store` appears or changes.
    fn bind_main_store(&self) {
        let imp = self.imp();
        let root_store = self.root_store();

        let Some(main) = root_store.main_store() else {
            // No main store — clear the sidebar model
            imp.sidebar_list_view.set_model(None::<&gtk::NoSelection>);
            imp.content_state_stack.set_visible_child_name("no_content");
            return;
        };

        // Bind sidebar to MainStore's selection model
        let sel_model = main.selected_container_model();
        imp.sidebar_list_view.set_model(Some(&sel_model));

        let this_clone = self.clone();
        sel_model.connect_selected_item_notify(move |model| {
            if let Some(container) = model
                .selected_item()
                .and_then(|obj| obj.downcast::<Container>().ok())
            {
                this_clone.update_container(&container);
                this_clone
                    .imp()
                    .content_state_stack
                    .set_visible_child_name("content");
            } else {
                this_clone
                    .imp()
                    .content_state_stack
                    .set_visible_child_name("no_content");
            }
        });

        // Toggle sidebar stack based on container count
        let this_clone = self.clone();
        main.containers().inner().connect_items_changed(
            move |list, _position, _removed, _added| {
                let visible_child_name = if list.n_items() == 0 {
                    "no-distroboxes"
                } else {
                    "distroboxes"
                };
                this_clone
                    .imp()
                    .sidebar_stack
                    .set_visible_child_name(visible_child_name);
            },
        );
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
                    .set_current_dialog(DialogType::Preferences);
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
                    .set_current_dialog(DialogType::CreateDistrobox);
            }),
            a("command-log").activate(|this, _, _| {
                this.root_store().set_current_dialog(DialogType::CommandLog);
            }),
            a("clone-container").activate(|this, _, _| {
                if let Some(src) = this.root_store().selected_container() {
                    this.root_store().open_dialog(
                        DialogType::CreateDistrobox,
                        DialogParams::new().with_clone_source(src),
                    );
                }
            }),
            a("upgrade-container").activate(|this, _, _| {
                if let Some(container) = this.root_store().selected_container() {
                    let task = this.root_store().upgrade_container(&container);
                    this.root_store().view_task(&task);
                }
            }),
            a("view-exportable-apps").activate(|this, _, _| {
                this.root_store().view_exportable_apps();
            }),
            a("install-package").activate(|this, _, _| {
                this.build_install_package_dialog();
            }),
            a("stop-container").activate(|this, _, _| {
                if let Some(container) = this.root_store().selected_container() {
                    this.root_store().stop_container(&container);
                }
            }),
            a("delete-container").activate(|this, _, _| {
                this.build_delete_dialog();
            }),
            a("open-terminal").activate(|this, _, _| {
                this.open_terminal();
            }),
        ];
        self.add_action_entries(actions.into_iter().map(|entry| entry.build()));

        let sort_action = gio::SimpleAction::new_stateful(
            "sort-key",
            Some(glib::VariantTy::STRING),
            &ContainerSortKey::Name.to_str().to_variant(),
        );
        sort_action.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |action, param| {
                if let Some(v) = param {
                    let s: String = v.get().unwrap();
                    if let Some(key) = ContainerSortKey::from_str(&s) {
                        action.change_state(v);
                        if let Some(main) = this.root_store().main_store() {
                            main.set_containers_sort_key(key);
                        }
                    }
                }
            }
        ));
        self.add_action(&sort_action);

        // Watch sort key changes on active main_store
        let this_clone = self.clone();
        self.root_store()
            .connect_main_store_notify(move |root_store| {
                if let Some(main) = root_store.main_store() {
                    let this = this_clone.clone();
                    // Sync current value immediately (catches cases where
                    // the initial main-store notification was missed)
                    let current_key = main.containers_sort_key();
                    if let Some(action) = this.lookup_action("sort-key") {
                        action.change_state(&current_key.to_str().to_variant());
                    }
                    let handler_id = main.connect_containers_sort_key_notify(move |main| {
                        let sort_key = main.containers_sort_key();
                        if let Some(action) = this.lookup_action("sort-key") {
                            action.change_state(&sort_key.to_str().to_variant());
                        }
                    });
                    // Store handler so it lives as long as the window.
                    // The old handler is auto-disconnected when the old
                    // MainStore is dropped; we explicitly drop the stored
                    // handler ID so it isn't leaked.
                    this_clone
                        .imp()
                        .sort_key_handler_id
                        .replace(Some(handler_id));
                }
            });

        // Sync the current sort key from main_store immediately, in case
        // the initial main-store notification was emitted before this
        // handler was registered.
        if let Some(main) = self.root_store().main_store()
            && let Some(action) = self.lookup_action("sort-key")
        {
            action.change_state(&main.containers_sort_key().to_str().to_variant());
        }
    }

    fn build_sidebar(&self) {
        let imp = self.imp();

        // Create a factory for creating and binding sidebar rows
        let factory = gtk::SignalListItemFactory::new();

        factory.connect_setup(|_factory, item| {
            let list_item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let sidebar_row = SidebarRow::new(&Container::default());

            let click = gtk::GestureClick::new();
            click.connect_released(clone!(
                #[weak]
                list_item,
                #[weak]
                sidebar_row,
                move |_, _, _, _| {
                    let _ = sidebar_row.activate_action(
                        "list.activate-item",
                        Some(&glib::Variant::from(list_item.position())),
                    );
                }
            ));
            sidebar_row.add_controller(click);

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
        imp.sidebar_list_view.connect_activate(clone!(
            #[weak(rename_to = this)]
            self,
            move |_, _| {
                this.imp().split_view.set_show_content(true);
            }
        ));

        // Add tasks button and update button to the bottom of the sidebar
        let sidebar_bottom_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

        // "Update Distrobox" button
        let update_button = gtk::Button::builder()
            .label(gettext("Update Distrobox"))
            .build();
        update_button.add_css_class("suggested-action");
        update_button.add_css_class("pill");
        update_button.set_margin_start(12);
        update_button.set_margin_end(12);
        update_button.set_margin_top(12);
        update_button.set_visible(self.root_store().bundled_update_available());
        update_button.connect_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.root_store().download_distrobox();
                this.root_store()
                    .set_current_dialog(DialogType::TaskManager);
            }
        ));
        self.root_store()
            .connect_bundled_update_available_notify(clone!(
                #[weak]
                update_button,
                move |root_store| {
                    update_button.set_visible(root_store.bundled_update_available());
                }
            ));
        sidebar_bottom_box.append(&update_button);

        let tasks_button = TasksButton::new(&self.root_store());
        tasks_button.add_css_class("flat");
        sidebar_bottom_box.append(&tasks_button);

        self.imp()
            .sidebar_bottom_slot
            .set_child(Some(&sidebar_bottom_box));
    }

    pub fn add_toast(&self, toast: adw::Toast) {
        self.imp().toast_overlay.add_toast(toast);
    }

    fn build_migration_banner(&self) {
        let banner = self.imp().migration_banner.clone();

        let connect_stale = |banner: &adw::Banner, main: &crate::models::MainStore| {
            main.stale_containers()
                .inner()
                .connect_items_changed(clone!(
                    #[weak]
                    banner,
                    move |list, _position, _removed, _added| {
                        let count = list.n_items();
                        if count > 0 {
                            banner.set_title(&ngettext(
                                "Container needs migration",
                                "Containers need migration",
                                count,
                            ));
                        }
                        banner.set_revealed(count > 0);
                    }
                ));
        };

        // Wire up the current MainStore (connect_main_store_notify fires only
        // on changes, so the initial store is never caught by the callback below)
        if let Some(main) = self.root_store().main_store() {
            connect_stale(&banner, &main);
        }

        // Re-bind when main_store changes
        let banner_for_notify = banner.clone();
        self.root_store()
            .connect_main_store_notify(move |root_store| {
                if let Some(main) = root_store.main_store() {
                    connect_stale(&banner_for_notify, &main);
                } else {
                    banner_for_notify.set_revealed(false);
                }
            });

        banner.connect_button_clicked(clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                if let Some(task) = this.root_store().migrate_stale_containers() {
                    this.root_store().view_task(&task);
                }
            }
        ));
    }

    fn open_terminal(&self) {
        let root_store = self.root_store();
        if let Some(container) = root_store.selected_container() {
            let task = root_store.spawn_container_terminal(&container);
            task.connect_status_notify(clone!(
                #[weak(rename_to = this)]
                self,
                move |task| {
                    if task.error().is_some() {
                        let toast = adw::Toast::new(&gettext("Check your terminal settings."));
                        toast.set_button_label(Some(&gettext("Preferences")));
                        toast.connect_button_clicked(clone!(
                            #[weak]
                            this,
                            move |_| {
                                this.root_store()
                                    .set_current_dialog(DialogType::Preferences);
                            }
                        ));
                        this.add_toast(toast);
                    }
                }
            ));
        }
    }

    fn build_delete_dialog(&self) {
        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Delete this container?"))
            .body(format!(
                "{} {}",
                self.root_store()
                    .selected_container_name()
                    .unwrap_or_default(),
                gettext("will be deleted. This action cannot be undone.")
            ))
            .close_response("cancel")
            .default_response("cancel")
            .build();
        dialog.add_response("cancel", &gettext("Cancel"));
        dialog.add_response("delete", &gettext("Delete"));

        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.connect_response(
            Some("delete"),
            clone!(
                #[weak(rename_to = this)]
                self,
                move |dialog, _| {
                    if let Some(container) = this.root_store().selected_container() {
                        this.root_store().delete_container(&container);
                        if let Some(main) = this.root_store().main_store() {
                            main.selected_container_model()
                                .set_selected(gtk::INVALID_LIST_POSITION);
                        }
                    }
                    dialog.close();
                }
            ),
        );

        dialog.present(Some(self));
    }

    fn build_install_package_dialog(&self) {
        if let Some(container) = self.root_store().selected_container() {
            let root_store = self.root_store();
            let file_dialog = gtk::FileDialog::builder()
                .title(gettext("Select Package"))
                .build();

            file_dialog.open(
                Some(self),
                None::<&gio::Cancellable>,
                move |res| {
                    if let Ok(file) = res
                        && let Some(path) = file.path()
                    {
                        info!(container = %container.name(), path = %path.display(), "Installing package into container");
                        root_store.install_package(&container, &path);
                    }
                },
            );
        }
    }

    fn update_container(&self, container: &Container) {
        let imp = self.imp();

        let container_overview = crate::widgets::ContainerOverview::new(container);
        imp.overview_bin.set_child(Some(&container_overview));

        let container_name = container.name();
        let terminal = {
            let mut terminals_by_container = imp.terminals_by_container.borrow_mut();
            terminals_by_container
                .entry(container_name.clone())
                .or_insert_with(|| IntegratedTerminal::new(&container_name, &self.root_store()))
                .clone()
        };
        imp.terminal_bin.set_child(Some(&terminal));

        // Switch to overview page
        imp.view_stack.set_visible_child_name("overview");
    }
}
