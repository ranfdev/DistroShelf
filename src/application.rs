/* application.rs
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

use std::path::Path;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::Properties;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;

use crate::DistroShelfWindow;
use crate::backends;
use crate::backends::{Distrobox, DistroboxCommandRunnerResponse};
use crate::config;
use crate::fakers::CommandRunner;
use crate::models::known_distros;
use crate::root_store::RootStore;

#[derive(Debug, Copy, Clone, PartialEq, Eq, glib::Enum, Default)]
#[enum_type(name = "DistroboxStoreTy")]
pub enum DistroboxStoreTy {
    #[default]
    Real,
    NullWorking,
    NullEmpty,
    NullNoVersion,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties)]
    #[properties(wrapper_type = super::DistroShelfApplication)]
    pub struct DistroShelfApplication {
        #[property(get, set = Self::set_distrobox_store_ty, builder(DistroboxStoreTy::Real))]
        pub distrobox_store_ty: RefCell<DistroboxStoreTy>,

        #[property(get, set)]
        pub root_store: RefCell<RootStore>,
    }

    impl DistroShelfApplication {
        fn set_distrobox_store_ty(&self, value: DistroboxStoreTy) {
            self.distrobox_store_ty.replace(value);
            if let Some(w) = self.obj().active_window() {
                w.close();
            }
            let w = self.obj().recreate_window();
            w.present();
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DistroShelfApplication {
        const NAME: &'static str = "DistroShelfApplication";
        type Type = super::DistroShelfApplication;
        type ParentType = adw::Application;
    }

    #[glib::derived_properties]
    impl ObjectImpl for DistroShelfApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<primary>q"]);
            obj.set_accels_for_action("app.shortcuts", &["<primary>question"]);
            obj.set_accels_for_action("win.refresh", &["F5"]);
            obj.set_accels_for_action("win.upgrade-container", &["<primary>u"]);
            obj.set_accels_for_action("win.upgrade-all", &["<primary><shift>u"]);
            obj.set_accels_for_action("win.install-package", &["<primary>i"]);
            obj.set_accels_for_action("win.preferences", &["<primary>comma"]);
            obj.set_accels_for_action("win.open-terminal", &["<primary>period"]);
            obj.set_accels_for_action("win.view-exportable-apps", &["<primary>e"]);
            obj.set_accels_for_action("win.delete-container", &["<primary>Delete"]);
            obj.set_accels_for_action("win.stop-container", &["<primary>s"]);
        }
    }

    impl ApplicationImpl for DistroShelfApplication {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let provider = gtk::CssProvider::new();
            let known_distro_colors = &known_distros::generate_css();
            provider.load_from_string(&format!("
                {known_distro_colors}
                .distro-color-fg {{
                    color: var(--distro-color);
                }}
                .distro-color-bg {{
                    background-color: var(--distro-color);
                }}
                
                .distro-header {{
                    background-color: color-mix(in xyz, var(--distro-color), var(--window-bg-color) 80%);
                    border-radius: 12px;
                    padding: 12px;
                }}

                .distro-row-item {{
                    padding: 6px;
                    border-radius: 6px;
                }}

                .output {{
                    border-radius: 12px;
                    border: 1px solid @borders;
                }}
                .tasks-popover row {{
                    padding: 6px 12px;
                    border-radius: 6px;
                }}
                .combo popover label {{
                    min-width: 300px;
                }}
                
                .status-dot {{
                    border-radius: 9999px;
                    background-color: @error_color;
                }}
                .status-dot.up {{
                    background-color: @success_color;
                }}
                .status-dot.exited, .status-dot.created {{
                    background-color: alpha(@borders, 0.5);
                }}

                button.xs {{
                    font-size: 0.8em;
                    padding: 0.2em 0.2em;
                }}

                @keyframes pop-warning {{
                    0% {{
                        transform: scale(1.0);
                    }}
                    30% {{
                        transform: scale(1.2) rotateZ(20deg);
                    }}
                    70% {{
                        transform: scale(1.4) rotateZ(-15deg);
                    }}
                }}

                .task-warning {{
                    animation: pop-warning 1s;
                    animation-iteration-count: 3;
                }}

                .task-output-terminal {{
                    padding: 8px;
                    /* A larger radius isn't rendered properly in VTE, because the background of therminal is drawn over it */
                    border-radius: 4px;
                    border: 2px solid @borders;
                }}
            "));
            // We give the CssProvided to the default screen so the CSS rules we added
            // can be applied to our window.
            gtk::style_context_add_provider_for_display(
                &gdk::Display::default().expect("Could not connect to a display."),
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );

            let this = self.obj().clone();
            // Get the current window or create one if necessary
            let window = this
                .active_window()
                .unwrap_or_else(|| this.recreate_window().upcast());

            // Ask the window manager/compositor to present the window
            window.present();
        }
    }

    impl GtkApplicationImpl for DistroShelfApplication {}
    impl AdwApplicationImpl for DistroShelfApplication {}
}

glib::wrapper! {
    pub struct DistroShelfApplication(ObjectSubclass<imp::DistroShelfApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl DistroShelfApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .build()
    }

    fn get_is_in_flatpak() -> bool {
        let fp_env = std::env::var("FLATPAK_ID").is_ok();
        if fp_env {
            return true;
        }

        Path::new("/.flatpak-info").exists()
    }

    fn recreate_window(&self) -> adw::ApplicationWindow {
        let distrobox_store_ty = self.imp().distrobox_store_ty.borrow().to_owned();
        let command_runner = match distrobox_store_ty {
            DistroboxStoreTy::NullWorking => Distrobox::null_command_runner(&[
                DistroboxCommandRunnerResponse::Version,
                DistroboxCommandRunnerResponse::new_list_common_distros(),
                DistroboxCommandRunnerResponse::new_common_images(),
                DistroboxCommandRunnerResponse::new_common_exported_apps(),
            ]),
            DistroboxStoreTy::NullEmpty => Distrobox::null_command_runner(&[
                DistroboxCommandRunnerResponse::Version,
                DistroboxCommandRunnerResponse::List(vec![]),
                DistroboxCommandRunnerResponse::new_common_images(),
            ]),
            DistroboxStoreTy::NullNoVersion => {
                Distrobox::null_command_runner(&[DistroboxCommandRunnerResponse::NoVersion])
            }
            _ => {
                let command_runner = CommandRunner::new_real();
                if Self::get_is_in_flatpak() {
                    command_runner.map_cmd(backends::flatpak::map_flatpak_spawn_host)
                } else {
                    command_runner
                }
            }
        };

        command_runner.output_tracker().enable();

        let root_store = RootStore::new(command_runner.clone());
        root_store.start_background_tasks();

        self.set_root_store(root_store);
        let window =
            DistroShelfWindow::new(self.upcast_ref::<adw::Application>(), self.root_store());
        window.upcast()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        self.add_action_entries([quit_action, about_action]);
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about =
            adw::AboutDialog::from_appdata(&format!("{}/metainfo.xml", config::PATH_ID), None);
        about.set_developers(&["Lorenzo Miglietta"]);
        about.set_copyright(
            "© 2024 Lorenzo Miglietta.\nAll brand icons are trademarks of their respective owners",
        );
        about.add_link("Donate", "https://github.com/sponsors/ranfdev");
        about.present(Some(&window));
    }
}
