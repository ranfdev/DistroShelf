# DistroShelf Copilot Instructions

## Project Overview
DistroShelf is a Rust-based GUI for managing Distrobox containers, built with **GTK4** and **Libadwaita**. It uses the **Meson** build system.

## Architecture & Patterns

### GObject Subclassing
The project heavily relies on standard Rust GObject subclassing.
- **Pattern:** Public struct wrapper + private `imp` module with the implementation struct.
- **Properties:** Use `#[derive(Properties)]` and `#[property(...)]` attributes in the `imp` struct.
- **Template Callbacks:** UI signals are often connected via `#[template_callback]` in the `imp` struct.

### State Management (`RootStore`)
- **Central Store:** `src/store/root_store.rs` (`RootStore`) is the central GObject that holds the application state (containers, tasks, settings).
- **Data Binding:** The UI binds directly to properties of `RootStore` or its children (e.g., `Container` objects).
- **Updates:** State changes are triggered by methods on `RootStore` which emit signals/notify properties.

### Command Execution (`CommandRunner`)
- **Mandatory Abstraction:** ALL terminal commands (distrobox, podman, etc.) MUST be executed via the `CommandRunner` trait (`src/fakers/command_runner.rs`).
- **Why:** This ensures compatibility with Flatpak (via `flatpak-spawn --host`) and enables testing via mocks.
- **Implementations:**
  - `RealCommandRunner`: Runs commands directly (for native builds).
  - `FlatpakCommandRunner`: Wraps commands with `flatpak-spawn --host` (for Flatpak builds).
  - `NullCommandRunner`: For testing/previews without actual Distrobox.

## Coding Conventions

### `glib::clone!` Macro
Use the attribute-based syntax for `glib::clone!`:
```rust
let label = gtk::Label::new("");
btn.connect_clicked(clone!(
    #[weak(rename_to=this)]
    self,
    #[weak]
    label,
    move |btn| {
        // ...
    }
));
```

### Async & Concurrency
- **UI Thread:** Use `glib::MainContext::default().spawn_local()` for async tasks that interact with the UI.
- **Data Fetching (`Query`):** Use `crate::query::Query` for async data fetching.
  - Wraps async operations with loading state, error handling, and caching.
  - Exposes GObject properties (`is-loading`, `data`, `error`) for easy UI binding.
- **Long-running Tasks (`DistroboxTask`):** Use `DistroboxTask` for operations like container creation or upgrades.
  - Tracks task status ("pending", "executing", "successful", "failed"), output logs, and errors.
  - Can be passed to `TaskManagerDialog` for visualization.
  - Use `handle_child_output` to stream command output to the task's log buffer.
- **Background Tasks:** Heavy operations run asynchronously using `async-process`, `futures`, and `async-channel`.

### Error Handling
- Use `anyhow` for application-level errors.
- Use `thiserror` for library/module-level errors.
- Log errors using `tracing::error!`.

## Build & Run Workflows

### Build System (Meson)
- **Setup:** `meson setup _build`
- **Compile:** `meson compile -C _build`
- **Run:** `_build/src/distroshelf`
- **Clean:** `meson compile -C _build --clean`

## Key Files
- `src/store/root_store.rs`: Main state container.
- `src/distrobox/mod.rs`: Distrobox CLI wrapper logic.
- `src/window.rs`: Main application window logic.
- `src/application.rs`: Application entry point and setup.
- `data/gtk/*.ui`: UI templates (composite templates).
