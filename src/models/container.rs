use crate::{
    backends::{ContainerInfo, ExportableApp, Status, container_runtime::Usage},
    models::DistroboxTask,
    fakers::{Command, CommandRunner},
    gtk_utils::TypedListStore,
    models::{KnownDistro, known_distro_by_image},
    query::Query,
    root_store::RootStore,
};

use adw::prelude::*;
use glib::subclass::prelude::*;
use gtk::glib;
use gtk::glib::{BoxedAnyObject, Properties, derived_properties};
use std::cell::RefCell;
use std::path::Path;
use std::time::Duration;

mod imp {
    use super::*;

    // This contains all the container informations given by distrobox, plus an associated KnownDistro struct
    #[derive(Properties)]
    #[properties(wrapper_type=super::Container)]
    pub struct Container {
        #[property(get, set)]
        root_store: RefCell<RootStore>,
        #[property(get, set)]
        pub name: RefCell<String>,
        #[property(get, set)]
        pub status_tag: RefCell<String>,
        #[property(get, set)]
        pub status_detail: RefCell<String>,
        #[property(get, set)]
        pub image: RefCell<String>,
        #[property(get, set, nullable)]
        pub distro: RefCell<Option<KnownDistro>>,
        pub apps: Query<TypedListStore<glib::BoxedAnyObject>>,
        pub binaries: Query<TypedListStore<glib::BoxedAnyObject>>,
        pub usage: Query<Usage>,
    }

    impl Default for Container {
        fn default() -> Self {
            Self {
                root_store: RefCell::new(RootStore::new(CommandRunner::new_null())),
                name: RefCell::new(String::new()),
                status_tag: RefCell::new(String::new()),
                status_detail: RefCell::new(String::new()),
                image: RefCell::new(String::new()),
                distro: RefCell::new(None),

                // Fetching apps often fails when the container is not running and distrobox has to start it,
                // so we add retries
                apps: Query::new("apps".into(), || async { Ok(TypedListStore::new()) })
                    .with_timeout(Duration::from_secs(1))
                    .with_retry_strategy(|n| {
                        if n < 3 {
                            Some(Duration::from_secs(n as u64))
                        } else {
                            None
                        }
                    }),
                binaries: Query::new("binaries".into(), || async { Ok(TypedListStore::new()) })
                    .with_timeout(Duration::from_secs(1))
                    .with_retry_strategy(|n| {
                        if n < 3 {
                            Some(Duration::from_secs(n as u64))
                        } else {
                            None
                        }
                    }),
                usage: Query::new("usage".into(), || async { Ok(Usage::default()) }),
            }
        }
    }

    #[derived_properties]
    impl ObjectImpl for Container {}

    #[glib::object_subclass]
    impl ObjectSubclass for Container {
        const NAME: &'static str = "Container";
        type Type = super::Container;
    }
}

glib::wrapper! {
    pub struct Container(ObjectSubclass<imp::Container>);
}
impl Container {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }
    pub fn from_info(root_store: &RootStore, value: ContainerInfo) -> Self {
        let this: Self = glib::Object::builder()
            .property("root-store", root_store)
            .build();

        this.apply_container_info(value);

        let this_clone = this.clone();
        this.apps().set_fetcher(move || {
            let this = this_clone.clone();
            async move {
                let apps = this
                    .root_store()
                    .distrobox()
                    .list_apps(&this.name())
                    .await?;

                let apps_list: TypedListStore<BoxedAnyObject> =
                    TypedListStore::from_iter(apps.into_iter().map(BoxedAnyObject::new));

                // Listing the apps starts the container, we need to update its status
                this.root_store().load_containers();
                Ok(apps_list)
            }
        });

        let this_clone = this.clone();
        this.binaries().set_fetcher(move || {
            let this = this_clone.clone();
            async move {
                let binaries = this
                    .root_store()
                    .distrobox()
                    .get_exported_binaries(&this.name())
                    .await?;

                let binaries_list: TypedListStore<BoxedAnyObject> =
                    TypedListStore::from_iter(binaries.into_iter().map(BoxedAnyObject::new));

                // Listing the binaries starts the container, we need to update its status
                this.root_store().load_containers();
                Ok(binaries_list)
            }
        });

        let this_clone = this.clone();
        this.usage().set_fetcher(move || {
            let this = this_clone.clone();
            async move {
                let root_store = this.root_store();
                let runtime = root_store.container_runtime().data().unwrap();
                let usage = runtime.usage(&this.name()).await?;
                Ok(usage)
            }
        });

        this
    }

    pub fn apply_container_info(&self, value: ContainerInfo) {
        let distro = known_distro_by_image(&value.image);

        let (status_tag, status_detail) = match value.status {
            Status::Up(v) => ("up", v),
            Status::Created(v) => ("created", v),
            Status::Exited(v) => ("exited", v),
            Status::Other(v) => ("other", v),
        };

        self.set_name(value.name);
        self.set_image(value.image);
        self.set_distro(distro);
        self.set_status_tag(status_tag.to_string());
        self.set_status_detail(status_detail);
    }

    pub fn is_running(&self) -> bool {
        self.status_tag() == "up"
    }

    pub fn apps(&self) -> Query<TypedListStore<BoxedAnyObject>> {
        self.imp().apps.clone()
    }

    pub fn binaries(&self) -> Query<TypedListStore<BoxedAnyObject>> {
        self.imp().binaries.clone()
    }

    pub fn usage(&self) -> Query<Usage> {
        self.imp().usage.clone()
    }

    pub fn upgrade(&self) -> DistroboxTask {
        let this = self.clone();
        self.root_store()
            .create_task(&self.name(), "upgrade", move |task| async move {
                let child = this.root_store().distrobox().upgrade(&this.name())?;
                task.handle_child_output(child).await
            })
    }

    pub fn launch(&self, app: ExportableApp) {
        let this = self.clone();
        self.root_store()
            .create_task(&self.name(), "launch-app", move |task| async move {
                let child = this
                    .root_store()
                    .distrobox()
                    .launch_app(&this.name(), &app)?;
                task.handle_child_output(child).await
            });
    }
    pub fn install(&self, path: &Path) {
        let this = self.clone();
        let package_manager = { self.distro().map(|d| d.package_manager()).unwrap() };
        let path_clone = path.to_owned();
        let name_clone = self.name();
        self.root_store()
            .create_task(&self.name(), "install", move |task| async move {
                task.set_description(format!("Installing {:?}", path_clone));
                // The file provided from the portal is under /run/user/1000 which is not accessible by root.
                // We can copy the file as a normal user to /tmp and then install.

                let enter_cmd = this.root_store().distrobox().enter_cmd(&name_clone);

                // the file of the package must have the correct extension (.deb for apt-get).
                let tmp_path = format!(
                    "/tmp/com.ranfdev.DistroShelf.user_package_{}",
                    package_manager.installable_file().unwrap()
                );
                let tmp_path = Path::new(&tmp_path);
                let cp_cmd_pure = Command::new_with_args("cp", [&path_clone, tmp_path]);
                let install_cmd_pure = package_manager.install_cmd(tmp_path).unwrap();

                let mut cp_cmd = enter_cmd.clone();
                cp_cmd.extend("--", &cp_cmd_pure);
                let mut install_cmd = enter_cmd.clone();
                install_cmd.extend("--", &install_cmd_pure);

                this.root_store()
                    .spawn_terminal_cmd(name_clone.clone(), &cp_cmd)
                    .await?;
                this.root_store()
                    .spawn_terminal_cmd(name_clone, &install_cmd)
                    .await
            });
    }
    pub fn export(&self, desktop_file_path: &str) {
        let this = self.clone();
        let desktop_file_path = desktop_file_path.to_string();
        self.root_store()
            .create_task(&self.name(), "export", move |_task| async move {
                this.root_store()
                    .distrobox()
                    .export_app(&this.name(), &desktop_file_path)
                    .await?;
                this.apps().refetch();
                Ok(())
            });
    }
    pub fn unexport(&self, desktop_file_path: &str) {
        let this = self.clone();
        let desktop_file_path = desktop_file_path.to_string();
        self.root_store()
            .create_task(&self.name(), "unexport", move |_task| async move {
                this.root_store()
                    .distrobox()
                    .unexport_app(&this.name(), &desktop_file_path)
                    .await?;
                this.apps().refetch();
                Ok(())
            });
    }
    pub fn export_binary(&self, binary_path: &str) -> DistroboxTask {
        let this = self.clone();
        let binary_path = binary_path.to_string();
        self.root_store()
            .create_task(&self.name(), "export-binary", move |_task| async move {
                this.root_store()
                    .distrobox()
                    .export_binary(&this.name(), &binary_path)
                    .await?;
                this.binaries().refetch();
                Ok(())
            })
    }
    pub fn unexport_binary(&self, binary_path: &str) {
        let this = self.clone();
        let binary_path = binary_path.to_string();
        self.root_store()
            .create_task(&self.name(), "unexport-binary", move |_task| async move {
                this.root_store()
                    .distrobox()
                    .unexport_binary(&this.name(), &binary_path)
                    .await?;
                this.binaries().refetch();
                Ok(())
            });
    }
    pub fn delete(&self) {
        let this = self.clone();
        self.root_store()
            .create_task(&self.name(), "delete", move |_task| async move {
                this.root_store().distrobox().remove(&this.name()).await?;
                Ok(())
            });
    }
    pub fn stop(&self) {
        let this = self.clone();
        self.root_store()
            .create_task(&self.name(), "stop", move |_task| async move {
                this.root_store().distrobox().stop(&this.name()).await?;
                Ok(())
            });
    }
    pub fn spawn_terminal(&self) -> DistroboxTask {
        let this = self.clone();
        self.root_store()
            .create_task(&self.name(), "spawn-terminal", move |_task| async move {
                let enter_cmd = this.root_store().distrobox().enter_cmd(&this.name());
                this.root_store()
                    .spawn_terminal_cmd(this.name(), &enter_cmd)
                    .await
            })
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}
