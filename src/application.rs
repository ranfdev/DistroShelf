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
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

use crate::DistroShelfWindow;
use crate::backends;
use crate::backends::DistroboxCommandRunnerResponse;
use crate::config;
use crate::fakers::{
    Command, CommandRunner, FileSystem, NullCommandRunnerBuilder, NullFileSystemBuilder, Settings,
};
use crate::models::known_distros;
use crate::root_store::RootStore;

#[derive(Debug, Copy, Clone, PartialEq, Eq, glib::Enum, Default)]
#[enum_type(name = "DistroboxStoreTy")]
pub enum DistroboxStoreTy {
    #[default]
    Real,
    NullHostWorking,
    NullEmpty,
    NullNoVersion,
    NullHostAndBundledWorking,
    /// No host distrobox, but a previously downloaded bundled distrobox
    /// is installed and working.
    NullBundledOnly,
}

/// Path reported for the host distrobox by the null command runners.
pub const NULL_HOST_DISTROBOX_PATH: &str = "/usr/bin/distrobox";

/// The GResource prefix where the app bundles its icons
/// (see `data/distroshelf.gresource.xml`).
pub const APP_ICONS_RESOURCE_PATH: &str = "/com/ranfdev/DistroShelf/icons";

/// Makes the icons bundled in the app's GResource (`data/icons`) resolvable
/// by the icon theme, matching the `scalable/actions` layout of the hicolor
/// theme. GTK only searches resources under the icon theme's registered
/// resource paths; without this, every bundled icon (distro logos, etc.)
/// falls back to the `image-missing` placeholder. Idempotent.
pub fn register_app_icon_resource_path(display: &gdk::Display) {
    let theme = gtk::IconTheme::for_display(display);
    if !theme.resource_path().iter().any(|p| p == APP_ICONS_RESOURCE_PATH) {
        theme.add_resource_path(APP_ICONS_RESOURCE_PATH);
    }
}

/// Installs the app-wide CSS on `display` — including the per-distro brand
/// colors that `DistroIcon` uses to tint the symbolic distro logos — and
/// registers the bundled icon resource path. Called by the application on
/// activation and by e2e tests, so tests render with the same styling as
/// the production app.
pub fn register_app_style_providers(display: &gdk::Display) {
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
    gtk::style_context_add_provider_for_display(
        display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    register_app_icon_resource_path(display);
}

impl DistroboxStoreTy {
    /// Predefined `CommandRunner` for the null store types, used by UI
    /// previews and end-to-end tests. Returns `None` for [`Self::Real`].
    pub fn null_command_runner(self) -> Option<CommandRunner> {
        self.null_command_runner_with_host_path(NULL_HOST_DISTROBOX_PATH)
    }

    /// Like [`null_command_runner`](Self::null_command_runner), but the host
    /// distrobox is registered under `host_path` instead of
    /// [`NULL_HOST_DISTROBOX_PATH`]. Tests use this to point the integrated
    /// terminal's shell spawn (which vte executes directly, outside the
    /// command runner) at a path guaranteed not to exist, so switching to
    /// the terminal tab never launches a real shell.
    pub fn null_command_runner_with_host_path(self, host_path: &str) -> Option<CommandRunner> {
        let mut builder = NullCommandRunnerBuilder::new();
        match self {
            DistroboxStoreTy::Real => return None,
            DistroboxStoreTy::NullHostWorking => {
                Self::add_host_distrobox_responses(
                    &mut builder,
                    host_path,
                    &[
                        DistroboxCommandRunnerResponse::Version,
                        DistroboxCommandRunnerResponse::new_list_common_distros(),
                        DistroboxCommandRunnerResponse::new_common_images(),
                        DistroboxCommandRunnerResponse::new_common_exported_apps(),
                    ],
                );
                Self::add_host_distrobox_path(&mut builder, host_path);
                Self::add_podman(&mut builder);
                Self::add_terminal(&mut builder);
                // Make unmatched commands (test -e, etc.) fail so bundled
                // distrobox does not appear to be installed.
                builder.fallback(ExitStatus::from_raw(1));
            }
            DistroboxStoreTy::NullEmpty => {
                Self::add_host_distrobox_responses(
                    &mut builder,
                    host_path,
                    &[
                        DistroboxCommandRunnerResponse::Version,
                        DistroboxCommandRunnerResponse::List(vec![]),
                        DistroboxCommandRunnerResponse::new_common_images(),
                    ],
                );
                Self::add_host_distrobox_path(&mut builder, host_path);
                Self::add_podman(&mut builder);
                Self::add_terminal(&mut builder);
                builder.fallback(ExitStatus::from_raw(1));
            }
            DistroboxStoreTy::NullNoVersion => {
                Self::add_host_distrobox_responses(
                    &mut builder,
                    host_path,
                    &[DistroboxCommandRunnerResponse::NoVersion],
                );
                // Unmatched commands (test -e, command -v, etc.) should fail
                // so that both host and bundled distrobox appear unavailable.
                builder.fallback(ExitStatus::from_raw(1));
            }
            DistroboxStoreTy::NullHostAndBundledWorking => {
                Self::add_host_distrobox_responses(
                    &mut builder,
                    host_path,
                    &[
                        DistroboxCommandRunnerResponse::Version,
                        DistroboxCommandRunnerResponse::new_list_common_distros(),
                        DistroboxCommandRunnerResponse::new_common_images(),
                        DistroboxCommandRunnerResponse::new_common_exported_apps(),
                    ],
                );
                Self::add_host_distrobox_path(&mut builder, host_path);
                Self::add_podman(&mut builder);
                Self::add_terminal(&mut builder);
                Self::add_installed_bundled_distrobox(&mut builder);
                builder.fallback(ExitStatus::from_raw(1));
            }
            DistroboxStoreTy::NullBundledOnly => {
                // No host distrobox: `distrobox version` and
                // `command -v distrobox` fail through the fallback.
                let bundled_path = crate::distrobox_downloader::get_bundled_distrobox_path();
                Self::add_distrobox_responses_as(
                    &mut builder,
                    &bundled_path.to_string_lossy(),
                    &[
                        DistroboxCommandRunnerResponse::new_list_common_distros(),
                        DistroboxCommandRunnerResponse::new_common_images(),
                        DistroboxCommandRunnerResponse::new_common_exported_apps(),
                    ],
                );
                Self::add_installed_bundled_distrobox(&mut builder);
                Self::add_podman(&mut builder);
                Self::add_terminal(&mut builder);
                builder.fallback(ExitStatus::from_raw(1));
            }
        }
        Some(builder.build())
    }

    pub fn null_file_system(self) -> FileSystem {
        match self {
            DistroboxStoreTy::Real => FileSystem::new_real(),
            DistroboxStoreTy::NullBundledOnly | DistroboxStoreTy::NullHostAndBundledWorking => {
                let mut builder = NullFileSystemBuilder::new();
                let bundled_path = crate::distrobox_downloader::get_bundled_distrobox_path();
                builder.file(&bundled_path, "fake distrobox script");
                builder.build()
            }
            _ => FileSystem::new_null(),
        }
    }

    /// Registers each distrobox response, rewriting the `distrobox` program
    /// to `program`. Non-distrobox helper commands included in a response
    /// (e.g. `printenv`) are registered unchanged.
    fn add_distrobox_responses_as(
        builder: &mut NullCommandRunnerBuilder,
        program: &str,
        responses: &[DistroboxCommandRunnerResponse],
    ) {
        Self::add_distrobox_responses_filtered(builder, program, responses, false);
    }

    /// Registers each distrobox response twice: under the bare `distrobox`
    /// program used before the executable is resolved, and under `host_path`
    /// used after the host path resolves.
    /// Program-independent helper commands are registered only once.
    fn add_host_distrobox_responses(
        builder: &mut NullCommandRunnerBuilder,
        host_path: &str,
        responses: &[DistroboxCommandRunnerResponse],
    ) {
        Self::add_distrobox_responses_filtered(builder, "distrobox", responses, false);
        Self::add_distrobox_responses_filtered(builder, host_path, responses, true);
    }

    fn add_distrobox_responses_filtered(
        builder: &mut NullCommandRunnerBuilder,
        program: &str,
        responses: &[DistroboxCommandRunnerResponse],
        only_distrobox_commands: bool,
    ) {
        for res in responses {
            for (mut cmd, out) in res.clone().into_commands() {
                if cmd.program == "distrobox" {
                    cmd.program = program.into();
                } else if only_distrobox_commands {
                    continue;
                }
                builder.cmd_full(cmd, move || out());
            }
        }
    }

    fn add_host_distrobox_path(builder: &mut NullCommandRunnerBuilder, host_path: &str) {
        let host_path = host_path.to_string();
        builder.cmd_full(
            Command::new_with_args("sh", ["-c", "command -v distrobox"]),
            move || Ok(host_path.clone()),
        );
    }

    /// Makes the bundled distrobox appear installed and working.
    fn add_installed_bundled_distrobox(builder: &mut NullCommandRunnerBuilder) {
        let bundled_path = crate::distrobox_downloader::get_bundled_distrobox_path();
        let mut version_cmd = Command::new(&bundled_path);
        version_cmd.arg("version");
        builder.cmd_full(version_cmd, || {
            Ok(format!(
                "distrobox: {}",
                crate::distrobox_downloader::DISTROBOX_VERSION
            ))
        });
    }

    fn add_podman(builder: &mut NullCommandRunnerBuilder) {
        builder.cmd_full(Command::new_with_args("podman", ["--version"]), || {
            Ok("podman version 4.9.3".to_string())
        });
    }

    /// Makes GNOME Console (kgx) the desktop default terminal and lets the
    /// terminal validation run succeed.
    fn add_terminal(builder: &mut NullCommandRunnerBuilder) {
        builder.cmd_full(
            Command::new_with_args(
                "gsettings",
                [
                    "get",
                    "org.gnome.desktop.default-applications.terminal",
                    "exec",
                ],
            ),
            || Ok("'kgx'".to_string()),
        );
        builder.cmd_full(
            Command::new_with_args("kgx", ["--", "echo", "DistroShelf terminal validation"]),
            || Ok(String::new()),
        );
    }
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
        }
    }

    impl ApplicationImpl for DistroShelfApplication {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let display = gdk::Display::default().expect("Could not connect to a display.");
            register_app_style_providers(&display);

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
        let command_runner = distrobox_store_ty.null_command_runner().unwrap_or_else(|| {
            let command_runner = CommandRunner::new_real();
            if Self::get_is_in_flatpak() {
                command_runner.map_cmd(backends::flatpak::map_flatpak_spawn_host)
            } else {
                command_runner
            }
        });

        command_runner.output_tracker().enable();

        // The single settings instance shared by the whole app. Null store
        // types simulate a first boot: they read and write in-memory settings
        // instead of the real GSettings schema.
        let settings = match distrobox_store_ty {
            DistroboxStoreTy::Real => Settings::new_real(),
            _ => Settings::new_null(),
        };

        let file_system = distrobox_store_ty.null_file_system();

        let root_store = RootStore::new(command_runner.clone(), settings, file_system);
        root_store.start_background_tasks();

        self.set_root_store(root_store);
        self.bind_shortcuts_model();
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

    fn bind_shortcuts_model(&self) {
        let shortcuts = self.root_store().shortcuts_model();
        let app = self.clone();
        shortcuts.connect_items_changed(move |_, _, _, _| {
            app.sync_shortcut_accels();
        });
        self.sync_shortcut_accels();
    }

    fn sync_shortcut_accels(&self) {
        let shortcuts = self.root_store().shortcuts_model();

        for action in self.root_store().shortcut_action_names() {
            self.set_accels_for_action(action, &[]);
        }

        for shortcut in shortcuts.iter::<gtk::Shortcut>() {
            let Ok(shortcut) = shortcut else {
                continue;
            };

            let Some(named_action) = shortcut.action().and_downcast::<gtk::NamedAction>() else {
                continue;
            };
            let Some(trigger) = shortcut.trigger() else {
                continue;
            };

            self.set_accels_for_action(&named_action.action_name(), &[&trigger.to_string()]);
        }
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
