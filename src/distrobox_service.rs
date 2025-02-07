use adw::prelude::*;
use adw::subclass::prelude::*;
use futures::io::BufReader;
use futures::AsyncBufReadExt;
use futures::StreamExt;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib;
use gtk::glib::SignalHandlerId;
use gtk::prelude::TextBufferExt;
use im_rc::Vector;
use std::cell::RefCell;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

use crate::container::Container;
use crate::distrobox::wrap_flatpak_cmd;
use crate::distrobox::Child;
use crate::distrobox::Command;
use crate::distrobox::CreateArgs;
use crate::distrobox::Distrobox;
use crate::distrobox::DistroboxCommandRunnerResponse;
use crate::distrobox::ExportableApp;
use crate::distrobox::Status;
use crate::distrobox_task::DistroboxTask;
use crate::known_distros::KnownDistro;
use crate::resource::Resource;
use crate::supported_terminals::SupportedTerminal;
use crate::supported_terminals::SUPPORTED_TERMINALS;
use gtk::gio;
use im_rc as im;

mod imp {
    use super::*;

    #[derive(Properties)]
    #[properties(wrapper_type = super::DistroboxService)]
    pub struct DistroboxService {
        pub distrobox: OnceLock<Distrobox>,
        pub containers: RefCell<Resource<im::HashMap<String, Container>, anyhow::Error>>,
        pub tasks: RefCell<Vector<DistroboxTask>>,
        pub images: RefCell<Resource<Vector<String>, anyhow::Error>>,
        pub settings: gio::Settings,
        pub version: RefCell<Resource<String, anyhow::Error>>,
    }

    impl Default for DistroboxService {
        fn default() -> Self {
            Self {
                distrobox: Default::default(),
                containers: Default::default(),
                tasks: Default::default(),
                images: Default::default(),
                settings: gio::Settings::new("com.ranfdev.DistroHome"),
                version: Default::default(),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for DistroboxService {
        fn constructed(&self) {
            self.parent_constructed();
        }
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("containers-changed").build(),
                    Signal::builder("images-changed").build(),
                    Signal::builder("tasks-changed").build(),
                    Signal::builder("version-changed").build(),
                    Signal::builder("terminal-changed").build(),
                ]
            })
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DistroboxService {
        const NAME: &'static str = "DistroboxService";
        type Type = super::DistroboxService;
    }
}

glib::wrapper! {
    pub struct DistroboxService(ObjectSubclass<imp::DistroboxService>);
}
impl DistroboxService {
    pub fn new() -> Self {
        let this: Self = glib::Object::builder().build();

        let distrobox = Distrobox::new();
        this.imp().distrobox.set(distrobox).unwrap();

        this.load_version();
        this
    }

    pub fn new_null_with_responses(
        responses: &[DistroboxCommandRunnerResponse],
        is_in_flatpak: bool,
    ) -> Self {
        let this: Self = glib::Object::builder().build();

        let distrobox = Distrobox::new_null_with_responses(responses, is_in_flatpak);
        this.imp().distrobox.set(distrobox).unwrap();

        this.load_version();
        this
    }

    fn load_version(&self) {
        let this = self.clone();
        *self.imp().version.borrow_mut() = Resource::Loading(None);
        glib::MainContext::ref_thread_default().spawn_local(async move {
            let version = this.distrobox().version().await;

            *this.imp().version.borrow_mut() = Resource::from(version.map_err(|x| x.into()));
            this.emit_by_name::<()>("version-changed", &[]);
        });
    }

    fn distrobox(&self) -> &Distrobox {
        self.imp().distrobox.get().unwrap()
    }
    pub fn containers(&self) -> Resource<im::HashMap<String, Container>, anyhow::Error> {
        self.imp().containers.borrow().clone()
    }
    pub fn load_container_infos(&self) {
        let this = self.clone();
        *this.imp().containers.borrow_mut() = Resource::Loading(None);
        this.emit_by_name::<()>("containers-changed", &[]);
        glib::MainContext::ref_thread_default().spawn_local_with_priority(
            glib::Priority::LOW,
            async move {
                let containers = this.distrobox().list().await.unwrap();
                *this.imp().containers.borrow_mut() = Resource::Loaded(
                    containers
                        .into_iter()
                        .map(|(k, v)| (k, Container::from(v)))
                        .collect(),
                );
                this.emit_by_name::<()>("containers-changed", &[]);
            },
        );
    }
    pub fn load_images(&self) {
        let this = self.clone();
        *this.imp().images.borrow_mut() = Resource::Loading(None);
        this.emit_by_name::<()>("images-changed", &[]);
        glib::MainContext::ref_thread_default().spawn_local_with_priority(
            glib::Priority::LOW,
            async move {
                let images = this.distrobox().list_images().await.unwrap();
                *this.imp().images.borrow_mut() = Resource::Loaded(images);
                this.emit_by_name::<()>("images-changed", &[]);
            },
        );
    }
    pub async fn list_apps(
        &self,
        name: &str,
    ) -> Result<Vector<crate::distrobox::ExportableApp>, crate::distrobox::Error> {
        let result = self.distrobox().list_apps(name).await;
        // Refresh container list to update status
        self.load_container_infos();
        result
    }

    #[instrument(skip_all, fields(task_name = %name, task_action = %action))]
    fn create_task<F, Fut>(&self, name: &str, action: &str, operation: F) -> DistroboxTask
    where
        F: FnOnce(DistroboxTask) -> Fut + 'static,
        Fut: std::future::Future<Output = Result<(), anyhow::Error>> + 'static,
    {
        info!("Creating new distrobox task");
        let this = self.clone();
        let name = name.to_string();
        let action = action.to_string();

        let task = DistroboxTask::new(&name, &action, move |task| async move {
            debug!("Starting task execution");
            let result = operation(task).await;
            if let Err(ref e) = result {
                error!(error = %e, "Task execution failed");
            }
            result
        });

        self.imp().tasks.borrow_mut().push_back(task.clone());
        self.emit_by_name::<()>("tasks-changed", &[]);
        task
    }

    pub fn do_upgrade(&self, name: &str) -> DistroboxTask {
        let this = self.clone();
        let name_clone = name.to_string();
        self.create_task(name, "upgrade", move |task| async move {
            let child = this.distrobox().upgrade(&name_clone)?;
            this.handle_child_output_for_task(child, &task).await
        })
    }
    pub fn do_upgrade_all(&self) {
        if let Resource::Loaded(containers) = self.containers() {
            for container in containers.values() {
                self.do_upgrade(&container.name());
            }
        }
    }
    pub fn do_launch(&self, name: &str, app: ExportableApp) -> DistroboxTask {
        let this = self.clone();
        let name_clone = name.to_string();
        self.create_task(name, "launch-app", move |task| async move {
            let child = this.distrobox().launch_app(&name_clone, &app)?;
            this.handle_child_output_for_task(child, &task).await
        })
    }
    pub fn do_export(&self, name: &str, desktop_file_path: &str) -> DistroboxTask {
        let this = self.clone();
        let name_clone = name.to_string();
        let desktop_file_path = desktop_file_path.to_string();
        self.create_task(name, "export", move |_task| async move {
            this.distrobox()
                .export_app(&name_clone, &desktop_file_path)
                .await?;
            Ok(())
        })
    }
    pub fn do_unexport(&self, name: &str, desktop_file_path: &str) -> DistroboxTask {
        let this = self.clone();
        let name_clone = name.to_string();
        let desktop_file_path = desktop_file_path.to_string();
        self.create_task(name, "unexport", move |_task| async move {
            this.distrobox()
                .unexport_app(&name_clone, &desktop_file_path)
                .await?;
            Ok(())
        })
    }
    pub fn do_create(&self, create_args: CreateArgs) -> DistroboxTask {
        let this = self.clone();
        let name = create_args.name.to_string();
        let task = self.create_task(&name, "create", move |task| async move {
            let child = this.distrobox().create(create_args).await?;
            this.handle_child_output_for_task(child, &task).await
        });
        task.set_description(
            "Creation requires downloading the container image, which may take some time...",
        );
        task
    }
    pub fn do_install(&self, name: &str, path: &Path) -> DistroboxTask {
        let this = self.clone();
        todo!("read known distro");
        // let package_manager = {
        //     self.imp()
        //         .containers
        //         .borrow()
        //         .data()
        //         .and_then(|hash_map| hash_map.get(name))
        //         .and_then(|container| container.distro())
        //         .and_then(|known_distro: KnownDistro| known_distro.package_manager)
        //         .expect(&format!("package manager not found for distro {}", name))
        // };
        // let path_clone = path.to_owned();
        // let name_clone = name.to_string();
        // let task = self.create_task(name, "install", move |_task| async move {
        //     // The file provided from the portal is under /run/user/1000 which is not accessible by root.
        //     // We can copy the file as a normal user to /tmp and then install.

        //     let enter_cmd = this.distrobox().enter_cmd(&name_clone);

        //     // the file of the package must have the correct extension (.deb for apt-get).
        //     let tmp_path = format!(
        //         "/tmp/com.ranfdev.DistroShelf.user_package{}",
        //         package_manager.installable_file()
        //     );
        //     let tmp_path = Path::new(&tmp_path);
        //     let cp_cmd_pure = Command::new_with_args("cp", [&path_clone, tmp_path]);
        //     let install_cmd_pure = package_manager.install_cmd(&tmp_path);

        //     let mut cp_cmd = enter_cmd.clone();
        //     cp_cmd.extend("--", &cp_cmd_pure);
        //     let mut install_cmd = enter_cmd.clone();
        //     install_cmd.extend("--", &install_cmd_pure);

        //     this.spawn_terminal_cmd(name_clone.clone(), &cp_cmd).await?;
        //     this.spawn_terminal_cmd(name_clone, &install_cmd).await
        // });
        // task.set_description(format!("Installing {:?}", path));
        // task
    }
    pub fn do_clone(&self, source_name: &str, target_name: &str) -> DistroboxTask {
        let this = self.clone();
        let source_name_clone = source_name.to_string();
        let target_name_clone = target_name.to_string();
        self.create_task(source_name, "clone", move |task| async move {
            let child = this
                .distrobox()
                .clone_to(&source_name_clone, &target_name_clone)
                .await?;
            this.handle_child_output_for_task(child, &task).await?;
            Ok(())
        })
    }
    pub fn do_delete(&self, name: &str) -> DistroboxTask {
        let this = self.clone();
        let name_clone = name.to_string();
        self.create_task(name, "delete", move |_task| async move {
            this.distrobox().remove(&name_clone).await?;
            this.load_container_infos();
            Ok(())
        })
    }
    pub fn do_stop(&self, name: &str) -> DistroboxTask {
        let this = self.clone();
        let name_clone = name.to_string();
        self.create_task(name, "stop", move |_task| async move {
            this.distrobox().stop(&name_clone).await?;
            this.load_container_infos();
            Ok(())
        })
    }
    pub fn do_spawn_terminal(&self, name: &str) -> DistroboxTask {
        let this = self.clone();
        let name_clone = name.to_string();
        self.create_task(name, "spawn-terminal", move |_task| async move {
            let enter_cmd = this.distrobox().enter_cmd(&name_clone);
            this.spawn_terminal_cmd(name_clone, &enter_cmd).await
        })
    }
    pub fn do_assemble(&self, file_path: &str) -> DistroboxTask {
        let this = self.clone();
        let file_path = file_path.to_string();
        self.create_task("assemble", "assemble", move |task| async move {
            let child = this.distrobox().assemble(&file_path)?;
            this.handle_child_output_for_task(child, &task).await
        })
    }

    async fn spawn_terminal_cmd(&self, name: String, cmd: &Command) -> Result<(), anyhow::Error> {
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
                    this.load_container_infos();
                    return;
                }
            }
        });
    }
    #[instrument(skip_all, fields(task_name = %task.name()))]
    async fn handle_child_output_for_task(
        &self,
        mut child: Box<dyn Child + Send>,
        task: &DistroboxTask,
    ) -> Result<(), anyhow::Error> {
        debug!("Handling child process output");
        let stdout = child.take_stdout().unwrap();
        let bufread = BufReader::new(stdout);
        let mut lines = bufread.lines();
        while let Some(line) = lines.next().await {
            let line = line?;
            task.output().insert(&mut task.output().end_iter(), &line);
            task.output().insert(&mut task.output().end_iter(), "\n");
        }

        match child.wait().await {
            Ok(e) if e.success() => {
                info!(exit_code = ?e.code(), "Child process exited successfully");
            }
            Ok(e) => {
                warn!(exit_code = ?e.code(), "Child process exited with error");
                anyhow::bail!("Status: {:?}", e.code());
            }
            Err(e) => {
                error!(error = %e, "Child process failed");
                return Err(e.into());
            }
        }
        self.load_container_infos();
        Ok(())
    }

    pub fn tasks(&self) -> Vector<DistroboxTask> {
        self.imp().tasks.borrow().clone()
    }
    pub fn images(&self) -> Resource<Vector<String>, anyhow::Error> {
        self.imp().images.borrow().clone()
    }

    // TODO: This should be defined in the AppViewModel directly
    pub fn set_selected_terminal_program(&self, program: &str) {
        if SUPPORTED_TERMINALS
            .iter()
            .find(|x| &x.program == &program)
            .is_none()
        {
            panic!("Unsupported terminal");
        }

        self.imp()
            .settings
            .set_string("selected-terminal", program)
            .expect("Failed to save setting");
        self.emit_by_name::<()>("terminal-changed", &[]);
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

    pub fn selected_terminal(&self) -> Option<SupportedTerminal> {
        let program: String = self.imp().settings.string("selected-terminal").into();
        SUPPORTED_TERMINALS
            .iter()
            .find(|x| &x.program == &program)
            .cloned()
    }

    pub fn version(&self) -> Resource<String, anyhow::Error> {
        self.imp().version.borrow().clone()
    }

    pub fn clear_ended_tasks(&self) {
        self.imp().tasks.borrow_mut().retain(|task| !task.ended());
        self.emit_by_name::<()>("tasks-changed", &[]);
    }

    pub fn connect_tasks_changed(&self, f: impl Fn(&Self) -> () + 'static) -> SignalHandlerId {
        let this = self.clone();
        self.connect_local("tasks-changed", true, move |_values| {
            f(&this);
            None
        })
    }
    pub fn connect_containers_changed(&self, f: impl Fn(&Self) -> () + 'static) -> SignalHandlerId {
        let this = self.clone();
        self.connect_local("containers-changed", true, move |_values| {
            f(&this);
            None
        })
    }
    pub fn connect_images_changed(&self, f: impl Fn(&Self) -> () + 'static) -> SignalHandlerId {
        let this = self.clone();
        self.connect_local("images-changed", true, move |_values| {
            f(&this);
            None
        })
    }

    pub fn connect_terminal_changed(&self, f: impl Fn(&Self) -> () + 'static) -> SignalHandlerId {
        let this = self.clone();
        self.connect_local("terminal-changed", true, move |_values| {
            f(&this);
            None
        })
    }

    pub fn connect_version_changed(&self, f: impl Fn(&Self) -> () + 'static) -> SignalHandlerId {
        let this = self.clone();
        self.connect_local("version-changed", true, move |_values| {
            f(&this);
            None
        })
    }
}

impl Default for DistroboxService {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
