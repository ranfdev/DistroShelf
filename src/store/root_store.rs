// You can copy/paste this file every time you need a simple GObject
// to hold some data

use futures::prelude::*;
use glib::subclass::prelude::*;
use glib::Properties;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::default;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use tracing::debug;
use tracing::error;
use tracing::info;

use crate::container::Container;
use crate::distrobox::{self, wrap_capture_cmd, Command};
use crate::distrobox::CreateArgs;
use crate::distrobox::Distrobox;
use crate::distrobox::Status;
use crate::distrobox::{wrap_flatpak_cmd, CommandRunner};
use crate::distrobox_task::DistroboxTask;
use crate::gtk_utils::reconcile_list_by_key;
use crate::remote_resource::RemoteResource;
use crate::supported_terminals::{Terminal, TerminalRepository};
use crate::tagged_object::TaggedObject;

mod imp {
    use std::{cell::OnceCell, rc::Rc};

    use crate::{distrobox::NullCommandRunner, remote_resource::RemoteResource};

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::RootStore)]
    pub struct RootStore {
        pub distrobox: OnceCell<crate::distrobox::Distrobox>,
        pub terminal_repository: RefCell<TerminalRepository>,
        pub command_runner: OnceCell<Rc<dyn crate::distrobox::CommandRunner>>,

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
                command_runner: OnceCell::new(),
                terminal_repository: RefCell::new(TerminalRepository::new(Rc::new(
                    NullCommandRunner::default(),
                ))),
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
    pub fn new(command_runner: Rc<dyn CommandRunner>) -> Self {
        let this: Self = glib::Object::builder().build();

        this.imp()
            .command_runner
            .set(command_runner.clone())
            .or(Err("command_runner already set"))
            .unwrap();

        this.imp()
            .terminal_repository
            .replace(TerminalRepository::new(command_runner.clone()));

        this.imp()
            .distrobox
            .set(Distrobox::new(command_runner.clone()))
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

    pub fn command_runner(&self) -> Rc<dyn crate::distrobox::CommandRunner> {
        self.imp().command_runner.get().unwrap().clone()
    }

    pub fn terminal_repository(&self) -> TerminalRepository {
        self.imp().terminal_repository.borrow().clone()
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
            container.upgrade();
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

    pub async fn resolve_host_path(&self, path: &str) -> Result<String, distrobox::Error> {
        let mut cmd = Command::new_with_args(
            "getfattr",
            [
                "-n",
                "user.document-portal.host-path",
                "--only-values",
                path,
            ],
        );
        wrap_capture_cmd(&mut cmd);
        let output = self.command_runner()
            .output(cmd)
            .await
            .map_err(|e| distrobox::Error::ResolveHostPath(e.to_string()));

        
        let is_from_sandbox = path.starts_with("/run/user");

        // If the path is not from a flatpak sandbox, we assume it's a regular path, so we can skip the getfattr command error.
        // If the command was successful, but for some reason the output is empty, we also return the path as is.
        let stdout = if (output.is_err() && !is_from_sandbox) || output.as_ref().map_or(false, |o| o.stdout.is_empty()) {
            return Ok(path.to_string());
        } else {
            output?.stdout
        };

        Ok(String::from_utf8(stdout)
            .map_err(|e| distrobox::Error::ParseOutput(e.to_string()))?
            .trim().to_string())
    }
}

impl Default for RootStore {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
