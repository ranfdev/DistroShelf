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
use crate::distrobox::CreateArgs;
use crate::distrobox::Distrobox;
use crate::distrobox::Status;
use crate::distrobox_task::DistroboxTask;
use crate::gtk_utils::reconcile_list_by_key;
use crate::remote_resource::RemoteResource;
use crate::supported_terminals::SupportedTerminal;
use crate::supported_terminals::SUPPORTED_TERMINALS;
use crate::tagged_object::TaggedObject;

mod imp {
    use std::cell::OnceCell;

    use crate::remote_resource::RemoteResource;

    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::RootStore)]
    pub struct RootStore {
        pub distrobox: OnceCell<crate::distrobox::Distrobox>,
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
                selected_container: Default::default(),
                current_view: Default::default(),
                current_dialog: Default::default(),
                distrobox: Default::default(),
                distrobox_version: Default::default(),
                images: Default::default(),
                tasks: gio::ListStore::new::<DistroboxTask>(),
                selected_task: Default::default(),
                settings: gio::Settings::new("com.ranfdev.DistroHome"),
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

        this.load_containers();
        this
    }

    pub fn distrobox(&self) -> &crate::distrobox::Distrobox {
        self.imp().distrobox.get().unwrap()
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
                    .into_iter()
                    .map(|(_k, v)| Container::from_info(&this, v))
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
    pub fn selected_terminal(&self) -> Option<SupportedTerminal> {
        let program: String = self.settings().string("selected-terminal").into();
        SUPPORTED_TERMINALS
            .iter()
            .find(|x| x.program == program)
            .cloned()
    }
    pub fn set_selected_terminal_program(&self, program: &str) {
        if !SUPPORTED_TERMINALS.iter().any(|x| x.program == program) {
            panic!("Unsupported terminal");
        }

        self.imp()
            .settings
            .set_string("selected-terminal", program)
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
            .arg("DistroHome terminal validation");
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
            for i in 0..times {
                glib::timeout_future(Duration::from_millis(i as u64 * 300)).await;

                // refresh the status of the container
                let containers = this.distrobox().list().await.unwrap();
                let container = containers.get(&name).unwrap();

                // if the container is running, we finally update the UI
                if let Status::Up(_) = &container.status {
                    // this.load_container_infos();
                    return;
                }
            }
        });
    }
}

impl Default for RootStore {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
