# DistroShelf Copilot Instructions

## Project Overview
DistroShelf is a Rust-based GTK4/Libadwaita GUI for managing [Distrobox](https://distrobox.it/) containers. Built with Meson, it provides container lifecycle management, package installation, and application export functionality.

## Architecture Overview

### State Management (`RootStore`)
**Central reactive store pattern** (`src/store/root_store.rs`):
- Single GObject holding all app state: containers, tasks, images, settings
- UI binds directly to `RootStore` properties via GObject data binding
- State updates trigger automatic UI refresh through property notifications
- Contains `Query<T>` instances for async data with built-in loading/error states
- Example: `containers_query: Query<Vec<Container>>` exposed as bindable property

### Command Execution Architecture
**ALL shell commands MUST use `CommandRunner` abstraction** (`src/fakers/command_runner.rs`):
```rust
// CORRECT - works in both native and Flatpak
let cmd = Command::new("distrobox-list");
let output = runner.output(cmd).await?;

// WRONG - breaks in Flatpak
let output = std::process::Command::new("distrobox-list").output()?;
```
**Why:** Flatpak apps cannot directly exec host commands. `FlatpakCommandRunner` automatically wraps commands with `flatpak-spawn --host` (see `src/backends/flatpak.rs::map_flatpak_spawn_host`).

**Implementations:**
- `RealCommandRunner`: Direct execution (native builds)
- `FlatpakCommandRunner`: Wraps with `flatpak-spawn --host` 
- `NullCommandRunner`: Returns mock responses for testing/previews

### GObject Subclassing Pattern
Standard gtk-rs pattern used throughout (`src/container.rs`, `src/window.rs`, etc.):
```rust
mod imp {
    #[derive(Properties)]
    #[properties(wrapper_type = super::MyWidget)]
    pub struct MyWidget {
        #[property(get, set)]
        name: RefCell<String>,
    }
}
glib::wrapper! {
    pub struct MyWidget(ObjectSubclass<imp::MyWidget>);
}
```

### Composite Template Pattern
UI widgets use GTK composite templates (`src/window.rs`, `src/welcome_view.rs`):
```rust
#[derive(gtk::CompositeTemplate)]
#[template(resource = "/com/ranfdev/DistroShelf/gtk/window.ui")]
pub struct DistroShelfWindow {
    #[template_child]
    pub sidebar_list_view: TemplateChild<gtk::ListView>,
}
// Connect callbacks in imp module:
#[gtk::template_callbacks]
impl WelcomeView {
    #[template_callback]
    fn continue_to_terminal_page(&self, _: &gtk::Button) { /* ... */ }
}
```
UI files in `data/gtk/*.ui` define structure; Rust handles logic.

## Key Patterns & Utilities

### `Query<T>` - Async Data Fetching
Wraps async operations with reactive state (`src/query/mod.rs`):
```rust
let query = Query::new("containers", || async { fetch_containers().await })
    .with_timeout(Duration::from_secs(5))
    .with_retry_strategy(|n| if n < 3 { Some(Duration::from_secs(n as u64)) } else { None });

query.refetch(); // Triggers fetch, updates is-loading/data/error properties
query.connect_success(|data| { /* UI update */ });
```
Properties: `is-loading`, `data`, `error`, `last-fetched-at`

### `DistroboxTask` - Long-Running Operations
Tracks command execution with output streaming (`src/distrobox_task.rs`):
```rust
let task = DistroboxTask::new("my-container", "Upgrade", |task| async move {
    let child = runner.spawn(Command::new("distrobox-upgrade"))?;
    task.handle_child_output(child).await?; // Streams output to task.output() TextBuffer
    Ok(())
});
// Status: "pending" -> "executing" -> "successful"/"failed"
// Displayed in TaskManagerDialog with live output
```

### `TypedListStore<T>` & List Reconciliation
Type-safe wrapper over `gio::ListStore` (`src/gtk_utils/typed_list_store.rs`):
```rust
let store = TypedListStore::<Container>::new();
for container in store.iter() { /* No downcasting needed */ }
```
Use `reconcile_list_by_key` to diff-update lists without full rebuild:
```rust
reconcile_list_by_key(&store, &new_containers, |c| c.name(), &["status", "image"]);
// Updates existing items, adds new, removes old - preserves object identity
```

### `glib::clone!` Macro
**Always use attribute syntax** for weak/strong references:
```rust
btn.connect_clicked(clone!(
    #[weak(rename_to=this)]
    self,
    #[strong]
    data,
    move |_| { this.do_something(&data); }
));
```

## Critical Integration Points

### Flatpak Detection
App automatically detects Flatpak environment and configures `CommandRunner`:
- Native: Uses `RealCommandRunner`
- Flatpak: Uses `FlatpakCommandRunner` (wraps all commands with `flatpak-spawn --host`)
- See `src/backends/flatpak.rs` and application initialization in `src/application.rs`

### Container Runtime Abstraction
`ContainerRuntime` trait (`src/backends/container_runtime.rs`) abstracts Podman/Docker:
- Auto-detects available runtime at startup
- Provides unified interface for images, events, container status
- `RootStore::container_runtime` is a `Query<Rc<dyn ContainerRuntime>>`

### Desktop File Parsing
Complex shell script in `src/backends/distrobox.rs` (`POSIX_FIND_AND_CONCAT_DESKTOP_FILES`) finds/encodes desktop files from containers for app export feature. Deserializes hex-encoded entries to avoid shell escaping issues.

## Key Files Reference
- `src/store/root_store.rs` - Central state store
- `src/backends/distrobox.rs` - Distrobox CLI wrapper (1300+ lines)
- `src/fakers/command_runner.rs` - Command execution abstraction
- `src/query/mod.rs` - Async query system
- `src/window.rs` - Main window with actions/UI binding
- `src/application.rs` - App initialization, store setup
- `data/gtk/*.ui` - GTK composite templates
