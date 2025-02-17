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

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config::VERSION;
use crate::distrobox::{Distrobox, DistroboxCommandRunnerResponse};
use crate::root_store::RootStore;
use crate::DistroShelfWindow;

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
    use std::cell::RefCell;

    use glib::Properties;
    use gtk::gdk;

    use crate::{known_distros, root_store::RootStore};

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

    fn recreate_window(&self) -> adw::ApplicationWindow {
        let distrobox = match { self.imp().distrobox_store_ty.borrow().to_owned() } {
            DistroboxStoreTy::NullWorking => Distrobox::new_null_with_responses(
                &[
                    DistroboxCommandRunnerResponse::Version,
                    DistroboxCommandRunnerResponse::new_list_common_distros(),
                    DistroboxCommandRunnerResponse::new_common_images(),
                    DistroboxCommandRunnerResponse::new_common_exported_apps(),
                ],
                false,
            ),
            DistroboxStoreTy::NullEmpty => Distrobox::new_null_with_responses(
                &[
                    DistroboxCommandRunnerResponse::Version,
                    DistroboxCommandRunnerResponse::List(vec![]),
                    DistroboxCommandRunnerResponse::new_common_images(),
                ],
                false,
            ),
            DistroboxStoreTy::NullNoVersion => Distrobox::new_null_with_responses(
                &[DistroboxCommandRunnerResponse::NoVersion],
                false,
            ),
            _ => Distrobox::new(),
        };

        self.set_root_store(RootStore::new(distrobox));
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
        let about = adw::AboutDialog::builder()
            .application_name("distroshelf")
            .application_icon("com.ranfdev.DistroShelf")
            .developer_name("Lorenzo Miglietta")
            .version(VERSION)
            .developers(vec!["Lorenzo Miglietta"])
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(gettext("translator-credits"))
            .copyright("© 2024 Lorenzo Miglietta")
            .build();

        about.present(Some(&window));
    }
}
