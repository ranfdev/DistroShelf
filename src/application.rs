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
use crate::DistrohomeWindow;

mod imp {
    use gtk::gdk;

    use crate::{distrobox_service::DistroboxService, known_distros};

    use super::*;

    #[derive(Debug, Default)]
    pub struct DistrohomeApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for DistrohomeApplication {
        const NAME: &'static str = "DistrohomeApplication";
        type Type = super::DistrohomeApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for DistrohomeApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<primary>q"]);
        }
    }

    impl ApplicationImpl for DistrohomeApplication {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.obj();

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
                .status-dot.inactive {{
                    background-color: alpha(@borders, 0.5);
                }}
                .status-dot.active {{
                    background-color: @success_color;
                }}
                .status-dot.error {{
                    background-color: @error_color;
                }}
            "));
            // We give the CssProvided to the default screen so the CSS rules we added
            // can be applied to our window.
            gtk::style_context_add_provider_for_display(
                &gdk::Display::default().expect("Could not connect to a display."),
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );

            // Get the current window or create one if necessary
            let window = application.active_window().unwrap_or_else(|| {
                let window = DistrohomeWindow::new(&*application, DistroboxService::new());

                window.upcast()
            });

            // Ask the window manager/compositor to present the window
            window.present();
        }
    }

    impl GtkApplicationImpl for DistrohomeApplication {}
    impl AdwApplicationImpl for DistrohomeApplication {}
}

glib::wrapper! {
    pub struct DistrohomeApplication(ObjectSubclass<imp::DistrohomeApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl DistrohomeApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .build()
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
            .application_name("distrohome")
            .application_icon("com.ranfdev.DistroHome")
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
