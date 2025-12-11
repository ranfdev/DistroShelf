use anyhow::Context;
use futures::prelude::*;
use glib::Properties;
use glib::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::OnceCell;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use tracing::error;
use tracing::info;
use tracing::{debug, warn};

use crate::backends::Distrobox;
use crate::backends::Status;
use crate::backends::container_runtime::{ContainerRuntime, get_container_runtime};
use crate::backends::podman::PodmanEvent;
use crate::backends::supported_terminals::{Terminal, TerminalRepository};
use crate::backends::{self, CreateArgs};
use crate::fakers::{Command, CommandRunner, FdMode};
use crate::gtk_utils::{TypedListStore, reconcile_list_by_key};
use crate::models::Container;
use crate::models::DistroboxTask;
use crate::models::ViewType;
use crate::models::{DialogParams, DialogType};
use crate::query::{Query, RefetchStrategy};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Hash, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Image {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Names")]
    pub names: Option<Vec<String>>,
}

mod imp {
    use std::rc::Rc;

    use crate::{backends::container_runtime::ContainerRuntime, query::Query};

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::RootStore)]
    pub struct RootStore {
        pub distrobox: OnceCell<crate::backends::Distrobox>,
        pub terminal_repository: RefCell<TerminalRepository>,
        pub command_runner: OnceCell<CommandRunner>,
        pub container_runtime: Query<Rc<dyn ContainerRuntime>>,

        pub distrobox_version: Query<String>,
        pub images_query: Query<Vec<String>>,
        pub downloaded_images_query: Query<HashSet<String>>,
        pub containers_query: Query<Vec<Container>>,

        pub containers: TypedListStore<Container>,
        pub selected_container_model: OnceCell<gtk::SingleSelection>,

        pub tasks: TypedListStore<DistroboxTask>,
        #[property(get, set, nullable)]
        pub selected_task: RefCell<Option<DistroboxTask>>,

        #[property(get)]
        pub settings: gio::Settings,

        #[property(get, set, builder(ViewType::default()))]
        current_view: RefCell<ViewType>,
        #[property(get, set, builder(DialogType::default()))]
        current_dialog: RefCell<DialogType>,

        /// Parameters for the current dialog (not a GObject property)
        pub dialog_params: RefCell<DialogParams>,
    }

    impl Default for RootStore {
        fn default() -> Self {
            Self {
                containers: TypedListStore::new(),
                command_runner: OnceCell::new(),
                container_runtime: Query::new("container_runtime".into(), || async {
                    anyhow::bail!("Container runtime not initialized")
                }),
                terminal_repository: RefCell::new(TerminalRepository::new(
                    CommandRunner::new_null(),
                )),
                selected_container_model: OnceCell::new(),
                current_view: Default::default(),
                current_dialog: Default::default(),
                dialog_params: Default::default(),
                distrobox: Default::default(),
                distrobox_version: Query::new("distrobox_version".into(), || async {
                    Ok(String::new())
                }),
                images_query: Query::new("images".into(), || async { Ok(vec![]) }),
                downloaded_images_query: Query::new("downloaded_images".into(), || async {
                    Ok(HashSet::new())
                }),
                containers_query: Query::new("containers".into(), || async { Ok(vec![]) }),
                tasks: TypedListStore::new(),
                selected_task: Default::default(),
                settings: gio::Settings::new("com.ranfdev.DistroShelf"),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for RootStore {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Watch settings
            let settings = obj.settings();
            settings.connect_changed(
                Some("distrobox-executable"),
                glib::clone!(
                    #[weak]
                    obj,
                    move |settings, _key| {
                        let val = settings.string("distrobox-executable");
                        if val == "bundled" {
                            // Check if bundled version exists
                            let path = crate::distrobox_downloader::get_bundled_distrobox_path();
                            if !path.exists() {
                                obj.download_distrobox();
                            } else {
                                // Just refetch version to update UI
                                obj.distrobox_version().refetch();
                            }
                        } else {
                            obj.distrobox_version().refetch();
                        }
                    }
                ),
            );
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RootStore {
        const NAME: &'static str = "RootStore";
        type Type = super::RootStore;
    }
}

glib::wrapper! {
    pub struct RootStore(ObjectSubclass<imp::RootStore>);
}
impl RootStore {
    pub fn new(command_runner: CommandRunner) -> Self {
        let this: Self = glib::Object::builder().build();

        this.imp()
            .command_runner
            .set(command_runner.clone())
            .or(Err("command_runner already set"))
            .unwrap();

        this.imp()
            .terminal_repository
            .replace(TerminalRepository::new(command_runner.clone()));

        // Build a CmdFactory that will be injected into the Distrobox backend. The factory
        // is created here (root_store) so the distrobox module does not depend on `gio::Settings`.
        let this_clone = this.clone();
        let cmd_factory: crate::backends::distrobox::command::CmdFactory = Box::new(move || {
            let distrobox_executable_val = this_clone.settings().string("distrobox-executable");
            let selected_program: String = if distrobox_executable_val == "bundled" {
                crate::distrobox_downloader::get_bundled_distrobox_path()
                    .to_string_lossy()
                    .into_owned()
            } else {
                "distrobox".into()
            };
            crate::fakers::Command::new(selected_program.clone())
        });

        this.imp()
            .distrobox
            .set(Distrobox::new(command_runner.clone(), cmd_factory))
            .or(Err("distrobox already set"))
            .unwrap();

        // Initialize the SingleSelection model
        let selection = gtk::SingleSelection::new(Some(this.containers().inner().clone()));
        this.imp()
            .selected_container_model
            .set(selection)
            .or(Err("selected_container_model already set"))
            .unwrap();

        let this_clone = this.clone();
        this.imp().distrobox_version.set_fetcher(move || {
            let this_clone = this_clone.clone();
            async move {
                let distrobox = this_clone.distrobox();
                distrobox.version().map_err(|e| e.into()).await
            }
        });
        let this_clone = this.clone();
        this.distrobox_version().connect_error(move |_error| {
            this_clone.set_current_view(ViewType::Welcome);
        });
        this.distrobox_version().refetch();

        let this_clone = this.clone();
        this.imp().images_query.set_fetcher(move || {
            let this_clone = this_clone.clone();
            async move {
                let distrobox = this_clone.distrobox();
                distrobox.list_images().map_err(|e| e.into()).await
            }
        });

        let this_clone = this.clone();
        this.imp().container_runtime.set_fetcher(move || {
            let this_clone = this_clone.clone();
            async move {
                get_container_runtime(this_clone.command_runner())
                    .await
                    .ok_or_else(|| anyhow::anyhow!("No container runtime available"))
            }
        });
        this.container_runtime().refetch();

        let this_clone = this.clone();
        this.imp().downloaded_images_query.set_fetcher(move || {
            let this_clone = this_clone.clone();
            async move {
                this_clone
                    .container_runtime()
                    .data()
                    .ok_or_else(|| anyhow::anyhow!("No container runtime available"))?
                    .downloaded_images()
                    .await
            }
        });

        let this_clone = this.clone();
        this.imp().containers_query.set_fetcher(move || {
            let this_clone = this_clone.clone();
            async move {
                let containers = this_clone.distrobox().list().await?;
                let containers: Vec<_> = containers
                    .into_values()
                    .map(|v| Container::from_info(&this_clone, v))
                    .collect();
                Ok(containers)
            }
        });

        let this_clone = this.clone();
        this.containers_query().connect_success(move |containers| {
            let this = this_clone.clone();

            reconcile_list_by_key(
                this.containers(),
                &containers[..],
                |item| item.name(),
                &["name", "status-tag", "status-detail", "distro", "image"],
            );
        });

        if this.selected_terminal().is_none() {
            let this = this.clone();
            glib::MainContext::ref_thread_default().spawn_local(async move {
                let Some(default_terminal) = this.terminal_repository().default_terminal().await
                else {
                    return;
                };
                this.set_selected_terminal_name(&default_terminal.name);
            });
        }

        // this.load_containers();
        this.start_listening_podman_events();
        this
    }

    pub fn distrobox(&self) -> &crate::backends::Distrobox {
        self.imp().distrobox.get().unwrap()
    }

    pub fn distrobox_version(&self) -> Query<String> {
        self.imp().distrobox_version.clone()
    }

    pub fn container_runtime(&self) -> Query<Rc<dyn ContainerRuntime>> {
        self.imp().container_runtime.clone()
    }

    pub fn images_query(&self) -> Query<Vec<String>> {
        self.imp().images_query.clone()
    }

    pub fn downloaded_images_query(&self) -> Query<HashSet<String>> {
        self.imp().downloaded_images_query.clone()
    }

    pub fn containers_query(&self) -> Query<Vec<Container>> {
        self.imp().containers_query.clone()
    }

    pub fn command_runner(&self) -> CommandRunner {
        self.imp().command_runner.get().unwrap().clone()
    }

    pub fn terminal_repository(&self) -> TerminalRepository {
        self.imp().terminal_repository.borrow().clone()
    }

    pub fn containers(&self) -> &TypedListStore<Container> {
        &self.imp().containers
    }

    pub fn tasks(&self) -> &TypedListStore<DistroboxTask> {
        &self.imp().tasks
    }

    pub fn selected_container_model(&self) -> gtk::SingleSelection {
        self.imp().selected_container_model.get().unwrap().clone()
    }

    /// Get the currently selected container, if any
    pub fn selected_container(&self) -> Option<Container> {
        let model = self.selected_container_model();
        let position = model.selected();
        if position == gtk::INVALID_LIST_POSITION {
            None
        } else {
            model
                .selected_item()
                .and_then(|obj| obj.downcast::<Container>().ok())
        }
    }

    pub fn load_containers(&self) {
        self.containers_query().refetch_with(RefetchStrategy::Throttle {
            interval: Duration::from_secs(1),
            trailing: true,
        });
    }

    pub fn download_distrobox(&self) -> DistroboxTask {
        let task = crate::distrobox_downloader::download_distrobox(self);
        self.tasks().append(&task);
        self.set_selected_task(Some(task.clone()));
        task
    }

    /// Start listening to podman events and auto-refresh container list for distrobox events
    pub fn start_listening_podman_events(&self) {
        let this = self.clone();
        let command_runner = self.command_runner();

        glib::MainContext::ref_thread_default().spawn_local(async move {
            info!("Starting podman events listener");
            let podman = crate::backends::podman::Podman::new(Rc::new(command_runner.clone()));

            let stream = match podman.listen_events() {
                Ok(stream) => stream,
                Err(e) => {
                    warn!("Failed to start podman events listener: {}", e);
                    return;
                }
            };

            // Process events
            stream
                .for_each(|line_result| {
                    let this = this.clone();
                    async move {
                        match line_result {
                            Ok(line) => {
                                // Parse the JSON event
                                match serde_json::from_str::<PodmanEvent>(&line) {
                                    Ok(event) => {
                                        // Only refresh if this is a distrobox container event
                                        if event.is_container_event() && event.is_distrobox() {
                                            debug!(
                                                "Distrobox container event detected ({}), refreshing container list",
                                                event.status.as_deref().unwrap_or("unknown")
                                            );
                                            this.containers_query().refetch_if_stale(Duration::from_secs(1));
                                        }
                                    }
                                    Err(e) => {
                                        debug!("Failed to parse podman event JSON: {} - Line: {}", e, line);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error reading podman event: {}", e);
                            }
                        }
                    }
                })
                .await;

            warn!("Podman events listener stopped");
        });
    }

    pub fn selected_container_name(&self) -> Option<String> {
        self.selected_container().map(|c| c.name())
    }

    pub fn create_task<F, Fut>(&self, name: &str, action: &str, operation: F) -> DistroboxTask
    where
        F: FnOnce(DistroboxTask) -> Fut + 'static,
        Fut: std::future::Future<Output = Result<(), anyhow::Error>> + 'static,
    {
        let this = self.clone();
        info!("Creating new distrobox task");
        let name = name.to_string();
        let action = action.to_string();

        let task = DistroboxTask::new(&name, &action, move |task| async move {
            debug!("Starting task execution");
            let result = operation(task).await;
            if let Err(ref e) = result {
                error!(error = %e, "Task execution failed");
            }
            this.load_containers();
            result
        });

        self.tasks().append(&task);
        task
    }

    pub fn clear_ended_tasks(&self) {
        self.tasks().retain(|task| !task.ended());
    }

    pub fn create_container(&self, create_args: CreateArgs) {
        let this = self.clone();
        let name = create_args.name.to_string();
        let task = self.create_task(&name, "create", move |task| async move {
            task.set_description(
                "Creation requires downloading the container image, which may take some time...",
            );
            let child = this.distrobox().create(create_args).await?;
            task.handle_child_output(child).await
        });
        self.view_task(&task);
    }
    pub fn clone_container(&self, source_name: &str, create_args: CreateArgs) {
        let this = self.clone();
        let name = create_args.name.to_string();
        let source = source_name.to_string();
        let task = self.create_task(&name, "clone", move |task| {
            let this = this.clone();
            let create_args = create_args;
            let source = source.clone();
            async move {
                task.set_description("Cloning container (may take some time)...");
                let child = this.distrobox().clone_from(&source, create_args).await?;
                task.handle_child_output(child).await
            }
        });
        self.view_task(&task);
    }
    pub fn assemble_container(&self, file_path: &str) {
        let this = self.clone();
        let file_path_clone = file_path.to_string();
        let file_name = Path::new(file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(file_path);

        let task = self.create_task(file_name, "assemble", move |task| async move {
            let child = this.distrobox().assemble(&file_path_clone)?;
            task.handle_child_output(child).await
        });
        self.view_task(&task);
    }
    pub fn upgrade_all(&self) {
        for container in self.containers().iter() {
            container.upgrade();
        }
    }

    pub fn view_task(&self, task: &DistroboxTask) {
        self.set_selected_task(Some(task));
        self.set_current_dialog(DialogType::TaskManager);
    }
    pub fn view_exportable_apps(&self) {
        let this = self.clone();
        this.set_current_dialog(DialogType::ExportableApps);
    }

    /// Opens a dialog with the given parameters.
    /// The parameters are stored and can be retrieved via `dialog_params()`.
    pub fn open_dialog(&self, dialog_type: DialogType, params: DialogParams) {
        self.imp().dialog_params.replace(params);
        self.set_current_dialog(dialog_type);
    }

    /// Returns the current dialog parameters.
    pub fn dialog_params(&self) -> std::cell::Ref<'_, DialogParams> {
        self.imp().dialog_params.borrow()
    }

    /// Takes the current dialog parameters, replacing them with default.
    pub fn take_dialog_params(&self) -> DialogParams {
        self.imp().dialog_params.take()
    }

    pub async fn spawn_terminal_cmd(
        &self,
        name: String,
        cmd: &Command,
    ) -> Result<(), anyhow::Error> {
        let Some(supported_terminal) = self.selected_terminal() else {
            error!("No terminal selected when trying to spawn terminal");
            return Err(anyhow::anyhow!("No terminal selected"));
        };
        let mut spawn_cmd = Command::new(supported_terminal.program);
        spawn_cmd
            .args(supported_terminal.extra_args)
            .arg(supported_terminal.separator_arg)
            .arg(cmd.program.clone())
            .args(cmd.args.clone());

        debug!(?spawn_cmd, "Spawning terminal command");
        let mut child = self.command_runner().spawn(spawn_cmd)?;

        let this = self.clone();
        glib::MainContext::ref_thread_default().spawn_local(async move {
            this.reload_till_up(name, 5);
        });
        if !child.wait().await?.success() {
            return Err(anyhow::anyhow!("Failed to spawn terminal"));
        }
        Ok(())
    }
    pub fn selected_terminal(&self) -> Option<Terminal> {
        // Old version stored the program, such as "gnome-terminal", now we store the name "GNOME console".
        let name_or_program: String = self.settings().string("selected-terminal").into();

        let by_name = self
            .imp()
            .terminal_repository
            .borrow()
            .terminal_by_name(&name_or_program);

        if let Some(terminal) = by_name {
            Some(terminal)
        } else if let Some(terminal) = self
            .imp()
            .terminal_repository
            .borrow()
            .terminal_by_program(&name_or_program)
        {
            Some(terminal)
        } else {
            error!("Terminal not found: {}", name_or_program);
            None
        }
    }
    pub fn set_selected_terminal_name(&self, name: &str) {
        self.imp()
            .settings
            .set_string("selected-terminal", name)
            .expect("Failed to save setting");
    }

    pub async fn validate_terminal(&self) -> Result<(), anyhow::Error> {
        let Some(terminal) = self.selected_terminal() else {
            error!("No terminal selected for validation");
            return Err(anyhow::anyhow!("No terminal selected"));
        };
        info!(terminal = %terminal.program, "Validating terminal");

        // Try running a simple command to validate the terminal
        let mut cmd = Command::new(terminal.program.clone());
        cmd.args(terminal.extra_args.clone())
            .arg(terminal.separator_arg)
            .arg("echo")
            .arg("DistroShelf terminal validation");

        let mut child = match self.command_runner().spawn(cmd) {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                error!(terminal = %terminal.program, "Terminal program not found");
                return Err(anyhow::anyhow!(
                    "Terminal program '{}' not found. Please install it or choose a different terminal.",
                    &terminal.program
                ));
            }
            Err(e) => return Err(e.into()),
        };

        if !child.wait().await?.success() {
            error!(terminal = %terminal.program, "Terminal validation failed");
            return Err(anyhow::anyhow!(
                "Terminal validation failed. '{}' did not run successfully.",
                &terminal.program
            ));
        }

        Ok(())
    }
    fn reload_till_up(&self, name: String, times: usize) {
        let this = self.clone();
        glib::MainContext::ref_thread_default().spawn_local(async move {
            for i in 1..times {
                glib::timeout_future(Duration::from_millis(i as u64 * 300)).await;

                // refresh the status of the container
                let containers = match this.distrobox().list().await {
                    Ok(c) => c,
                    Err(e) => {
                        warn!(error = %e, "Failed to list containers while waiting for container to start");
                        continue;
                    }
                };
                let Some(container) = containers.get(&name) else {
                    debug!(name = %name, "Container not found while waiting for it to start");
                    continue;
                };

                // if the container is running, we finally update the UI
                if let Status::Up(_) = &container.status {
                    this.load_containers();
                    return;
                }
            }
        });
    }

    pub async fn run_to_string(&self, mut cmd: Command) -> Result<String, anyhow::Error> {
        cmd.stderr = FdMode::Pipe;
        cmd.stdout = FdMode::Pipe;
        let output = self.command_runner().output(cmd.clone()).await?;
        Ok(String::from_utf8(output.stdout).map_err(|e| {
            error!(cmd = %cmd, "Failed to parse command output");
            backends::Error::ParseOutput(e.to_string())
        })?)
    }

    pub async fn is_nvidia_host(&self) -> bool {
        // uses lspci to check if the host has an NVIDIA GPU
        debug!("Checking if host is NVIDIA");
        let cmd = Command::new("lspci");
        let output = glib::future_with_timeout(Duration::from_secs(2), async move {
            self.run_to_string(cmd).await.context("Calling lspci")
        })
        .await
        .context("timeout")
        .flatten();
        match output {
            Ok(output) => {
                let is_nvidia = output.contains("NVIDIA") || output.contains("nVidia");
                debug!(is_nvidia, "lspci ran successfully");
                is_nvidia
            }
            Err(e) => {
                warn!(?e, "Failed to check if host is NVIDIA");
                false // If we can't run lspci, we assume it's not NVIDIA
            }
        }
    }

    fn getfattr_cmd(path: &str) -> Command {
        Command::new_with_args(
            "getfattr",
            [
                "-n",
                "user.document-portal.host-path",
                "--only-values",
                path,
            ],
        )
    }

    pub async fn resolve_host_path(&self, path: &str) -> Result<String, backends::Error> {
        // The path could be a:
        // 1. Host path, already resolved to a real location, e.g., "/home/user/Documents/custom-home-folder".
        // 2. Path from a flatpak sandbox, e.g., "/run/user/1000/doc/abc123".
        // The user may not have the `getfattr`, but we still want to try using it,
        // because we don't have an exact way to know if the path is from a flatpak sandbox or not.
        // If the path is already a real host path, `getfattr` may return an empty output,
        // because it doesn't have the `user.document-portal.host-path` attribute set by the flatpak portal.

        debug!(?path, "Resolving host path");

        let cmd = Self::getfattr_cmd(path);
        let output = self
            .run_to_string(cmd)
            .await
            .map_err(|e| backends::Error::ResolveHostPath(e.to_string()));

        let is_from_sandbox = path.starts_with("/run/user");

        match output {
            Ok(resolved_path) => {
                debug!(?resolved_path, "Resolved host path");
                if resolved_path.is_empty() {
                    // If the output is empty, we assume the path is already a real host path.
                    return Ok(path.to_string());
                }
                Ok(resolved_path.trim().to_string())
            }
            Err(e) if !is_from_sandbox => {
                debug!(
                    ?e,
                    "Failed to execute getfattr, but path doesn't seem from a sandbox anyway"
                );
                Ok(path.trim().to_string())
            }
            Err(e) => {
                debug!(?e, "Failed to resolve host path using getfattr");
                Err(e)
            }
        }
    }
}

impl Default for RootStore {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::fakers::NullCommandRunnerBuilder;

    #[gtk::test]
    fn test_resolve_path() {
        // (input_path, getfattr_output, expected_resolved_path)
        let tests = [
            (
                "/run/user/1000/doc/abc123",
                Ok("/home/user/Documents/custom-home-folder"),
                Ok("/home/user/Documents/custom-home-folder"),
            ),
            // When getfattr returns empty for a non-sandbox path, we return the original path
            ("/home/user/Documents/custom-home-folder", Ok(""), {
                Ok("/home/user/Documents/custom-home-folder")
            }),
            // If the resolution fails and the path is from a sandbox, we expect an error
            ("/run/user/1000/doc/xyz456", Err(()), Err(())),
        ];

        for (input_path, getfattr_output, expected_resolved_path) in tests {
            let runner = NullCommandRunnerBuilder::new()
                .cmd_full(RootStore::getfattr_cmd(input_path), move || {
                    getfattr_output
                        .map(|s| s.to_string())
                        // we need to return a real io::Error here
                        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "Command not found"))
                })
                .build();
            let store = RootStore::new(runner);

            let resolved_path: Result<String, backends::Error> =
                smol::block_on(store.resolve_host_path(input_path));

            if let Ok(expected_resolved_path) = expected_resolved_path {
                assert_eq!(resolved_path.unwrap(), expected_resolved_path);
            } else {
                assert!(resolved_path.is_err());
            }
        }
    }
}
