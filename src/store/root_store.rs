// You can copy/paste this file every time you need a simple GObject
// to hold some data

use futures::prelude::*;
use glib::subclass::prelude::*;
use glib::Properties;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::time::Duration;
use tracing::debug;
use tracing::error;
use tracing::info;

use crate::container::Container;
use crate::distrobox::wrap_flatpak_cmd;
use crate::distrobox::Command;
use crate::distrobox::CreateArgName;
use crate::distrobox::CreateArgs;
use crate::distrobox::DesktopEntry;
use crate::distrobox::Distrobox;
use crate::distrobox::ExportableApp;
use crate::distrobox::Status;
use crate::distrobox_task::DistroboxTask;
use crate::gtk_utils::reconcile_list_by_key;
use crate::remote_resource::RemoteResource;
use crate::supported_terminals::{Terminal, TerminalRepository};
use crate::tagged_object::TaggedObject;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Serializable commands that can be executed by the application
///
/// These commands represent all operations that can be performed in the DistroShelf app,
/// and can be serialized to/from JSON for storage or IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppCommand {
    // Container management
    CreateContainer {
        image: String,
        init: bool,
        nvidia: bool,
        name: String,
        home_path: Option<String>,
        volumes: Vec<String>,
    },
    AssembleContainer {
        file_path: String,
    },
    AssembleContainerFromUrl {
        url: String,
    },
    DeleteContainer {
        container_name: String,
    },
    CloneContainer {
        source_name: String,
        target_name: String,
    },
    StopContainer {
        name: String,
    },
    StopAllContainers,
    UpgradeContainer {
        name: String,
    },
    UpgradeAllContainers,
    SpawnTerminal {
        name: String,
    },

    LaunchApp {
        container_name: String,
        app: ExportableApp,
    },
    ExportApp {
        container_name: String,
        desktop_file_path: String,
    },
    UnexportApp {
        container_name: String,
        desktop_file_path: String,
    },

    InstallPackage {
        container_name: String,
        package_path: PathBuf,
    },

    // UI operations
    RequestConfirmation {
        message: String,
        title: String,
        command: Box<AppCommand>,
    },
    ViewTask {
        task_id: String,
    },
    ViewExportableAppsDialog {
        container_name: String,
    },
    ViewInstallPackageDialog {
        container_name: String,
    },
    ViewCloneContainerDialog {
        container_name: String,
    },
    ClearEndedTasks,

    // Terminal settings
    SetSelectedTerminal {
        name: String,
    },
    ValidateTerminal,

    // Container list management
    ReloadContainers,
}

impl AppCommand {
    /// Serialize this command to a JSON string
    pub fn serialize(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize a command from a JSON string
    pub fn deserialize(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

mod imp {
    use std::cell::OnceCell;

    use crate::remote_resource::RemoteResource;

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::RootStore)]
    pub struct RootStore {
        pub distrobox: OnceCell<crate::distrobox::Distrobox>,
        pub terminal_repository: TerminalRepository,

        #[property(get, set)]
        pub distrobox_version: RefCell<RemoteResource>,

        #[property(get, set)]
        pub images: RefCell<RemoteResource>,

        #[property(get)]
        containers: gio::ListStore,
        #[property(get, set, nullable)]
        selected_container: RefCell<Option<crate::container::Container>>,

        #[property(get)]
        pub tasks: gio::ListStore,
        #[property(get, set, nullable)]
        pub selected_task: RefCell<Option<DistroboxTask>>,

        #[property(get)]
        pub settings: gio::Settings,

        #[property(get, set)]
        current_view: RefCell<TaggedObject>,
        #[property(get, set)]
        current_dialog: RefCell<TaggedObject>,
    }

    impl Default for RootStore {
        fn default() -> Self {
            Self {
                containers: gio::ListStore::new::<crate::container::Container>(),
                terminal_repository: Default::default(),
                selected_container: Default::default(),
                current_view: Default::default(),
                current_dialog: Default::default(),
                distrobox: Default::default(),
                distrobox_version: Default::default(),
                images: Default::default(),
                tasks: gio::ListStore::new::<DistroboxTask>(),
                selected_task: Default::default(),
                settings: gio::Settings::new("com.ranfdev.DistroShelf"),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for RootStore {}

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
    pub fn new(distrobox: Distrobox) -> Self {
        let this: Self = glib::Object::builder().build();

        this.imp()
            .distrobox
            .set(distrobox)
            .or(Err("distrobox already set"))
            .unwrap();

        let this_clone = this.clone();
        this.imp()
            .distrobox_version
            .replace(RemoteResource::new(move |_| {
                let this_clone = this_clone.clone();
                async move {
                    let distrobox = this_clone.distrobox();
                    distrobox.version().map_err(|e| e.into()).await
                }
            }));
        let this_clone = this.clone();
        this.distrobox_version()
            .connect_error_notify(move |resource| {
                if resource.error().is_some() {
                    this_clone.set_current_view(TaggedObject::new("welcome"));
                }
            });
        this.distrobox_version().reload();

        let this_clone = this.clone();
        this.set_images(RemoteResource::new(move |_| {
            let this_clone = this_clone.clone();
            async move {
                let distrobox = this_clone.distrobox();
                distrobox.list_images().map_err(|e| e.into()).await
            }
        }));

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

        this.load_containers();
        this
    }

    pub fn distrobox(&self) -> &crate::distrobox::Distrobox {
        self.imp().distrobox.get().unwrap()
    }

    pub fn terminal_repository(&self) -> &TerminalRepository {
        &self.imp().terminal_repository
    }

    pub fn load_containers(&self) {
        let this = self.clone();
        glib::MainContext::ref_thread_default().spawn_local_with_priority(
            glib::Priority::LOW,
            async move {
                let Ok(containers) = this.distrobox().list().await else {
                    return;
                };
                let containers: Vec<_> = containers
                    .into_values()
                    .map(|v| Container::from_info(&this, v))
                    .collect();
                reconcile_list_by_key(
                    this.containers(),
                    &containers[..],
                    |item| item.name(),
                    &["name", "status-tag", "status-detail", "distro", "image"],
                );
            },
        );
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
        self.tasks().retain(|task| {
            let task: &DistroboxTask = task.downcast_ref().unwrap();
            !task.ended()
        });
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
    pub fn assemble_container(&self, file_path: &str) {
        let this = self.clone();
        let file_path = file_path.to_string();
        self.create_task("assemble", "assemble", move |task| async move {
            let child = this.distrobox().assemble(&file_path)?;
            task.handle_child_output(child).await
        });
    }
    pub fn upgrade_all(&self) {
        for container in self.containers().snapshot() {
            let container: &Container = container.downcast_ref().unwrap();
            // container.upgrade();
        }
    }

    pub fn view_task(&self, task: &DistroboxTask) {
        self.set_selected_task(Some(task));
        self.set_current_dialog(TaggedObject::new("task-manager"));
    }
    pub fn view_exportable_apps(&self) {
        let this = self.clone();
        this.set_current_dialog(TaggedObject::new("exportable-apps"));
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
            .arg(supported_terminal.separator_arg)
            .arg(cmd.program.clone())
            .args(cmd.args.clone());
        spawn_cmd = wrap_flatpak_cmd(spawn_cmd);

        debug!(?spawn_cmd, "Spawning terminal command");
        let mut async_cmd: async_process::Command = spawn_cmd.into();
        let mut child = async_cmd.spawn()?;
        let this = self.clone();
        glib::MainContext::ref_thread_default().spawn_local(async move {
            this.reload_till_up(name, 5);
        });
        if !child.status().await?.success() {
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
            .terminal_by_name(&name_or_program);

        if let Some(terminal) = by_name {
            Some(terminal)
        } else if let Some(terminal) = self
            .imp()
            .terminal_repository
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
        cmd.arg(terminal.separator_arg)
            .arg("echo")
            .arg("DistroShelf terminal validation");
        cmd = wrap_flatpak_cmd(cmd);

        let mut async_cmd: async_process::Command = cmd.into();
        let mut child = match async_cmd.spawn() {
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

        if !child.status().await?.success() {
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
                let containers = this.distrobox().list().await.unwrap();
                let container = containers.get(&name).unwrap();

                // if the container is running, we finally update the UI
                if let Status::Up(_) = &container.status {
                    this.load_containers();
                    return;
                }
            }
        });
    }

    /// Execute a serializable AppCommand
    pub fn execute_command(&self, command: AppCommand) -> anyhow::Result<()> {
        println!("Executing command: {}", command.serialize()?);
        match command {
            AppCommand::CreateContainer {
                image,
                init,
                nvidia,
                name,
                home_path,
                volumes,
            } => {
                let create_args = CreateArgs {
                    image,
                    init,
                    nvidia,
                    name: CreateArgName::new(&name)?,
                    home_path,
                    volumes,
                };
                self.create_container(create_args);
            }
            AppCommand::AssembleContainer { file_path } => {
                self.assemble_container(&file_path);
            }
            AppCommand::AssembleContainerFromUrl { url } => {
                let this = self.clone();
                self.create_task("assemble", "assemble-url", move |task| async move {
                    let child = this.distrobox().assemble_from_url(&url)?;
                    task.handle_child_output(child).await
                });
            }
            AppCommand::DeleteContainer { container_name } => {
                if let Some(container) = self.find_container_by_name(&container_name) {
                    let this = self.clone();
                    self.create_task(&container.name(), "delete", move |_task| async move {
                        this.distrobox().remove(&container.name()).await?;
                        Ok(())
                    });
                }
            }
            AppCommand::CloneContainer {
                source_name,
                target_name,
            } => {
                if let Some(container) = self.find_container_by_name(&source_name) {
                    let this = self.clone();
                    let target_name_clone = target_name.to_string();
                    let task = self.create_task(&source_name, "clone", move |task| async move {
                        let child = this
                            .distrobox()
                            .clone_to(&container.name(), &target_name_clone)
                            .await?;
                        task.handle_child_output(child).await?;
                        Ok(())
                    });
                    self.view_task(&task);
                }
            }
            AppCommand::StopContainer { name } => {
                if let Some(container) = self.find_container_by_name(&name) {
                    let this = self.clone();
                    self.create_task(&container.name(), "stop", move |_task| async move {
                        this.distrobox().stop(&container.name()).await?;
                        // this.load_container_infos();
                        Ok(())
                    });
                }
            }
            AppCommand::StopAllContainers => {
                let this = self.clone();
                self.create_task("stop-all", "stop-all", move |_task| async move {
                    this.distrobox().stop_all().await?;
                    Ok(())
                });
            }
            AppCommand::UpgradeContainer { name } => {
                let this = self.clone();
                if let Some(container) = self.find_container_by_name(&name) {
                    let task = self.create_task(&name, "upgrade", move |task| async move {
                        let child = this.distrobox().upgrade(&container.name())?;
                        task.handle_child_output(child).await
                    });
                    self.view_task(&task);
                }
            }
            AppCommand::UpgradeAllContainers => {
                self.upgrade_all();
            }
            AppCommand::SpawnTerminal { name } => {
                if let Some(container) = self.find_container_by_name(&name) {
                    let this = self.clone();
                    self.create_task(
                        &container.name(),
                        "spawn-terminal",
                        move |_task| async move {
                            let enter_cmd = this.distrobox().enter_cmd(&container.name());
                            this.spawn_terminal_cmd(container.name(), &enter_cmd).await
                        },
                    );
                }
            }
            AppCommand::LaunchApp {
                container_name,
                app,
            } => {
                if let Some(container) = self.find_container_by_name(&container_name) {
                    let this = self.clone();
                    self.create_task(&container_name, "launch-app", move |task| async move {
                        let child = this.distrobox().launch_app(&container.name(), &app)?;
                        task.handle_child_output(child).await
                    });
                }
            }
            AppCommand::ExportApp {
                container_name,
                desktop_file_path,
            } => {
                if let Some(container) = self.find_container_by_name(&container_name) {
                    let this = self.clone();
                    self.create_task(&container.name(), "export", move |_task| async move {
                        this.distrobox()
                            .export_app(&container.name(), &desktop_file_path)
                            .await?;
                        container.apps().reload();
                        Ok(())
                    });
                }
            }
            AppCommand::UnexportApp {
                container_name,
                desktop_file_path,
            } => {
                if let Some(container) = self.find_container_by_name(&container_name) {
                    let this = self.clone();
                    self.create_task(&container.name(), "unexport", move |_task| async move {
                        this.distrobox()
                            .unexport_app(&container.name(), &desktop_file_path)
                            .await?;
                        container.apps().reload();
                        Ok(())
                    });
                }
            }
            AppCommand::InstallPackage {
                container_name,
                package_path,
            } => {
                if let Some(container) = self.find_container_by_name(&container_name) {
                    {
                        let this = self.clone();
                        let package_manager =
                            { container.distro().map(|d| d.package_manager()).unwrap() };
                        self.create_task(&container.name(), "install", move |task| async move {
                            task.set_description(format!("Installing {:?}", package_path));
                            // The file provided from the portal is under /run/user/1000 which is not accessible by root.
                            // We can copy the file as a normal user to /tmp and then install.

                            let enter_cmd = this.distrobox().enter_cmd(&container.name());

                            // the file of the package must have the correct extension (.deb for apt-get).
                            let tmp_path = format!(
                                "/tmp/com.ranfdev.DistroShelf.user_package_{}",
                                package_manager.installable_file().unwrap()
                            );
                            let tmp_path = Path::new(&tmp_path);
                            let cp_cmd_pure =
                                Command::new_with_args("cp", [&package_path, tmp_path]);
                            let install_cmd_pure = package_manager.install_cmd(tmp_path).unwrap();

                            let mut cp_cmd = enter_cmd.clone();
                            cp_cmd.extend("--", &cp_cmd_pure);
                            let mut install_cmd = enter_cmd.clone();
                            install_cmd.extend("--", &install_cmd_pure);

                            this.spawn_terminal_cmd(container.name().clone(), &cp_cmd)
                                .await?;
                            this.spawn_terminal_cmd(container.name().clone(), &install_cmd)
                                .await
                        });
                    };
                }
            }
            AppCommand::RequestConfirmation {
                message,
                title,
                command,
            } => {
                self.set_current_dialog(TaggedObject::new("confirmation"));
                // TODO: continue
            }
            AppCommand::ViewTask { task_id } => {
                if let Some(task) = self.find_task_by_id(&task_id) {
                    self.view_task(&task);
                }
            }
            AppCommand::ViewExportableAppsDialog { container_name } => {
                if let Some(_) = self.find_container_by_name(&container_name) {
                    self.view_exportable_apps();
                }
            }
            AppCommand::ViewInstallPackageDialog { container_name } => {
                self.set_current_dialog(TaggedObject::new("confirmation"));
                // TODO: continue
            }
            AppCommand::ViewCloneContainerDialog { container_name } => {
                if let Some(container) = self.find_container_by_name(&container_name) {
                    self.set_current_dialog(TaggedObject::new("clone-container"));
                    self.set_selected_container(Some(container));
                }
                // TODO: continue
            }
            AppCommand::ClearEndedTasks => {
                self.clear_ended_tasks();
            }
            AppCommand::SetSelectedTerminal { name } => {
                self.set_selected_terminal_name(&name);
            }
            AppCommand::ValidateTerminal => {
                let this = self.clone();
                glib::MainContext::ref_thread_default().spawn_local(async move {
                    let _ = this.validate_terminal().await;
                });
            }
            AppCommand::ReloadContainers => {
                self.load_containers();
            }
        }
        Ok(())
    }

    // Helper method to find a container by name
    fn find_container_by_name(&self, name: &str) -> Option<Container> {
        for i in 0..self.containers().n_items() {
            let container = self.containers().item(i)?.downcast::<Container>().ok()?;
            if container.name() == name {
                return Some(container);
            }
        }
        None
    }

    // Helper method to find a task by ID
    fn find_task_by_id(&self, task_id: &str) -> Option<DistroboxTask> {
        for i in 0..self.tasks().n_items() {
            let task = self.tasks().item(i)?.downcast::<DistroboxTask>().ok()?;
            if task.target() == task_id {
                return Some(task);
            }
        }
        None
    }
}

impl Default for RootStore {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
