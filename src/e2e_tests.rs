// End-to-end tests: boot the real application window on top of the
// predefined null command runners and null settings (simulating the first
// boot of the app) and assert on the text the user actually sees.

use std::time::Duration;

use adw::prelude::*;

use crate::application::DistroboxStoreTy;
use crate::application::register_app_style_providers;
use crate::fakers::Settings;
use crate::gtk_utils::extract_widget_text;
use crate::gtk_utils::save_screenshot_if_requested;
use crate::gtk_utils::test_utils::spin_main_context_until;
use crate::models::{DistroboxSource, RootStore, ViewType};
use crate::widgets::DistroShelfWindow;

/// Registers the app's resources (`distroshelf.gresource`, built by meson)
/// and installs the app-wide styling (CSS + icon resource path), so e2e
/// tests render with real icons — including the per-distro brand colors —
/// instead of the `image-missing` placeholder and default text color. This
/// mirrors production, where `main.rs` registers the same gresource and
/// `DistroShelfApplication::activate` calls `register_app_style_providers`.
fn register_app_resources() {
    let resource = gtk::gio::Resource::load(crate::config::GRESOURCE_PATH)
        .expect("distroshelf.gresource not found; build it with ninja");
    gtk::gio::resources_register(&resource);
    let display = gtk::gdk::Display::default().expect("e2e tests require a display");
    register_app_style_providers(&display);
}

/// Boots the app like `DistroShelfApplication::recreate_window` does, but
/// against the predefined null command runner of `store_ty` and first-boot
/// (schema default) null settings.
fn boot_first_start(store_ty: DistroboxStoreTy) -> (DistroShelfWindow, RootStore) {
    boot_first_start_with_host_path(store_ty, crate::application::NULL_HOST_DISTROBOX_PATH)
}

/// Like [`boot_first_start`], but the host distrobox is registered under
/// `host_path` instead of [`NULL_HOST_DISTROBOX_PATH`]. The integrated
/// terminal spawns its shell directly through vte (bypassing the command
/// runner), so tests that switch to the terminal tab must point that spawn
/// at a path guaranteed not to exist — otherwise they would enter a real
/// container.
fn boot_first_start_with_host_path(
    store_ty: DistroboxStoreTy,
    host_path: &str,
) -> (DistroShelfWindow, RootStore) {
    let _ = adw::init();

    register_app_resources();

    let runner = store_ty
        .null_command_runner_with_host_path(host_path)
        .expect("null store type must provide a command runner");
    let settings = Settings::new_null();
    let root_store = RootStore::new(runner, settings, store_ty.null_file_system());
    root_store.start_background_tasks();

    let window = DistroShelfWindow::new_unattached(root_store.clone());
    (window, root_store)
}

/// All user-visible strings of the currently shown view.
fn visible_view_strings(window: &DistroShelfWindow) -> Vec<String> {
    let visible = window
        .main_stack()
        .visible_child()
        .expect("view stack has a visible child");
    extract_widget_text(&visible)
        .entries
        .into_iter()
        .flat_map(|entry| entry.strings)
        .map(|string| string.value)
        .collect()
}

fn assert_visible_string(strings: &[String], expected: &str) {
    assert!(
        strings.iter().any(|s| s == expected),
        "expected {expected:?} among the visible strings: {strings:#?}"
    );
}

/// Buttons labeled `label` inside `root`, in widget-tree order.
fn buttons_labeled(root: &gtk::Widget, label: &str) -> Vec<gtk::Button> {
    fn walk(widget: &gtk::Widget, label: &str, out: &mut Vec<gtk::Button>) {
        if let Some(button) = widget.downcast_ref::<gtk::Button>()
            && button.label().is_some_and(|l| l == label)
        {
            out.push(button.clone());
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            walk(&c, label, out);
            child = c.next_sibling();
        }
    }
    let mut out = Vec::new();
    walk(root, label, &mut out);
    out
}

#[gtk::test]
fn test_first_boot_with_host_distrobox_shows_containers() {
    let (window, root_store) = boot_first_start(DistroboxStoreTy::NullHostWorking);

    // Wait for the containers to be rendered in the sidebar, not just
    // loaded in the model.
    spin_main_context_until(Duration::from_secs(5), || {
        visible_view_strings(&window).iter().any(|s| s == "Ubuntu")
    });

    assert!(
        root_store
            .distrobox_version()
            .data()
            .is_some_and(|exe| exe.is_some()),
        "host distrobox must be detected"
    );
    assert!(
        root_store.container_runtime().data().is_some(),
        "podman must be detected as container runtime"
    );
    assert_eq!(
        root_store.current_view(),
        ViewType::Main,
        "a working host distrobox must boot into the main view"
    );
    assert_eq!(
        window.main_stack().visible_child_name().as_deref(),
        Some("main")
    );

    let strings = visible_view_strings(&window);
    for container_name in ["Ubuntu", "Fedora", "Arch Linux", "Alpine"] {
        assert_visible_string(&strings, container_name);
    }

    let _ = save_screenshot_if_requested(&window, "e2e_first_boot_host_containers_main").unwrap();

    window.destroy();
}

#[gtk::test]
fn test_first_boot_without_host_distrobox_shows_welcome_requirements() {
    let (window, root_store) = boot_first_start(DistroboxStoreTy::NullNoVersion);

    spin_main_context_until(Duration::from_secs(5), || {
        root_store.current_view() == ViewType::Welcome
            && visible_view_strings(&window)
                .iter()
                .any(|s| s.starts_with("Not found"))
    });

    assert!(
        root_store
            .distrobox_version()
            .data()
            .is_some_and(|exe| exe.is_none()),
        "no distrobox must be detected on first boot without host distrobox"
    );
    assert_eq!(
        root_store.current_view(),
        ViewType::Welcome,
        "a missing distrobox must boot into the welcome view"
    );
    assert_eq!(
        window.main_stack().visible_child_name().as_deref(),
        Some("welcome")
    );

    let strings = visible_view_strings(&window);
    assert_visible_string(&strings, "System Requirements");
    assert_visible_string(&strings, "Distrobox");
    assert_visible_string(
        &strings,
        "Not found - Install from system or use bundled version",
    );
    assert_visible_string(&strings, "Container Runtime");
    assert_visible_string(&strings, "Not found - Please install Podman or Docker");

    let _ = save_screenshot_if_requested(&window, "e2e_first_boot_without_host_welcome_requirements")
        .unwrap();

    window.destroy();
}

#[gtk::test]
fn test_choosing_bundled_distrobox_shows_main_view() {
    let (window, root_store) = boot_first_start(DistroboxStoreTy::NullBundledOnly);

    // First boot without a host distrobox lands on the welcome view with
    // the distrobox requirement marked missing.
    spin_main_context_until(Duration::from_secs(5), || {
        root_store.current_view() == ViewType::Welcome
            && visible_view_strings(&window)
                .iter()
                .any(|s| s == "Not found - Install from system or use bundled version")
    });
    assert_eq!(
        window.main_stack().visible_child_name().as_deref(),
        Some("welcome")
    );

    // The user chooses the bundled distrobox. Both the welcome view's
    // "Use Bundled Distrobox" action and the preferences radio end up in
    // this call once the bundled executable is installed.
    root_store.set_distrobox_source(DistroboxSource::Bundled);

    let welcome_view = window
        .main_stack()
        .child_by_name("welcome")
        .expect("welcome page must exist");
    let continue_buttons = buttons_labeled(&welcome_view, "Continue");
    assert_eq!(
        continue_buttons.len(),
        2,
        "welcome must have the requirements and terminal Continue buttons"
    );

    // Choosing the bundled distrobox satisfies the requirements and
    // enables the Continue button.
    spin_main_context_until(Duration::from_secs(5), || {
        continue_buttons[0].is_sensitive()
    });
    assert!(
        continue_buttons[0].is_sensitive(),
        "Continue must become sensitive after choosing the bundled distrobox"
    );
    assert!(
        root_store
            .distrobox_version()
            .data()
            .flatten()
            .is_some_and(|exe| exe.is_bundled()),
        "the resolved distrobox executable must be the bundled one"
    );
    let _ = save_screenshot_if_requested(&window, "e2e_choosing_bundled_distrobox_welcome").unwrap();
    continue_buttons[0].emit_clicked();

    // First boot preselects the desktop default terminal; continuing
    // validates it and enters the main view.
    spin_main_context_until(Duration::from_secs(5), || {
        root_store.selected_terminal().is_some()
    });
    continue_buttons[1].emit_clicked();

    spin_main_context_until(Duration::from_secs(5), || {
        root_store.current_view() == ViewType::Main
    });
    assert_eq!(
        root_store.current_view(),
        ViewType::Main,
        "choosing the bundled distrobox and continuing must show the main view"
    );
    assert_eq!(
        window.main_stack().visible_child_name().as_deref(),
        Some("main")
    );

    // The containers served by the bundled distrobox are visible.
    spin_main_context_until(Duration::from_secs(5), || {
        visible_view_strings(&window).iter().any(|s| s == "Ubuntu")
    });
    let strings = visible_view_strings(&window);
    for container_name in ["Ubuntu", "Fedora"] {
        assert_visible_string(&strings, container_name);
    }

    let _ = save_screenshot_if_requested(&window, "e2e_choosing_bundled_distrobox_main").unwrap();

    window.destroy();
}

#[gtk::test]
fn test_switching_to_terminal_tab_focuses_terminal() {
    // Point the host distrobox at a path that cannot exist: the terminal
    // tab spawns its shell directly through vte (bypassing the command
    // runner), and the test must not enter a real container.
    let (window, root_store) = boot_first_start_with_host_path(
        DistroboxStoreTy::NullHostWorking,
        "/nonexistent/distroshelf-test-distrobox",
    );

    spin_main_context_until(Duration::from_secs(5), || {
        visible_view_strings(&window).iter().any(|s| s == "Ubuntu")
    });

    // Select a container so the overview/terminal pages are built.
    let main = root_store.main_store().expect("main store must exist");
    main.selected_container_model().set_selected(0);
    spin_main_context_until(Duration::from_secs(5), || {
        root_store.selected_container().is_some()
    });

    // Present the window so the terminal page is mapped (needed for the
    // focus grab to take effect). We assert on `is_focus()` — the terminal
    // being the focus widget within the toplevel — rather than
    // `has_focus()`, which additionally requires the window manager to
    // grant the toplevel the global input focus and would flake headless.
    window.present();
    spin_main_context_until(Duration::from_secs(5), || window.is_mapped());

    let view_stack = window.view_stack();
    view_stack.set_visible_child_name("terminal");
    let terminal = window
        .embedded_terminal()
        .expect("a selected container must provide an embedded terminal");
    spin_main_context_until(Duration::from_secs(5), || terminal.is_focus());
    assert!(
        terminal.is_focus(),
        "switching to the terminal tab must focus the embedded terminal"
    );

    // Switching away and back re-focuses it.
    view_stack.set_visible_child_name("overview");
    view_stack.set_visible_child_name("terminal");
    spin_main_context_until(Duration::from_secs(5), || terminal.is_focus());
    assert!(
        terminal.is_focus(),
        "switching back to the terminal tab must re-focus the embedded terminal"
    );

    window.destroy();
}
