use crate::{
    distrobox::{Command, ContainerInfo, ExportableApp, Status},
    known_distros::{known_distro_by_image, KnownDistro},
    remote_resource::RemoteResource,
    root_store::RootStore,
};

use gtk::{
    gio,
    glib::{derived_properties, BoxedAnyObject, Properties},
};

use adw::prelude::*;
use glib::subclass::prelude::*;
use gtk::glib;
use std::{cell::RefCell, path::Path};

mod imp {
    use crate::remote_resource::RemoteResource;

    use super::*;

    // This contains all the container informations given by distrobox, plus an associated KnownDistro struct
    #[derive(Default, Properties)]
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
        #[property(get, set)]
        pub distro: RefCell<Option<KnownDistro>>,
        #[property(get, set)]
        pub apps: RefCell<RemoteResource>,
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
        let distro = known_distro_by_image(&value.image);

        let (status_tag, status_detail) = match value.status {
            Status::Up(v) => ("up", v),
            Status::Created(v) => ("created", v),
            Status::Exited(v) => ("exited", v),
            Status::Other(v) => ("other", v),
        };
        let this: Self = glib::Object::builder()
            .property("root-store", root_store)
            .property("name", value.name)
            .property("image", value.image)
            .property("distro", distro)
            .property("status-tag", status_tag)
            .property("status-detail", status_detail)
            .build();

        let this_clone = this.clone();
        let loader = move |apps_list: Option<&gio::ListStore>| {
            dbg!(apps_list);
            let this = this_clone.clone();
            let mut apps_list = apps_list
                .cloned()
                .unwrap_or_else(|| gio::ListStore::new::<BoxedAnyObject>());
            async move {
                let apps = this
                    .root_store()
                    .distrobox()
                    .list_apps(&this.name())
                    .await?;

                apps_list.remove_all();
                apps_list.extend(apps.into_iter().map(|app| BoxedAnyObject::new(app)));

                // Listing the apps starts the container, we need to update its status
                this.root_store().load_containers();
                Ok(apps_list)
            }
        };
        this.set_apps(RemoteResource::new::<gio::ListStore, _>(loader));

        this
    }

    pub fn upgrade(&self) {
        let this = self.clone();
        self.root_store()
            .create_task(&self.name(), "upgrade", move |task| async move {
                let child = this.root_store().distrobox().upgrade(&this.name())?;
                task.handle_child_output(child).await
            });
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
        let package_manager = {
            self.distro().map(|d| d.package_manager().clone()).unwrap()
        };
        let path_clone = path.to_owned();
        let name_clone = self.name();
        self.root_store().create_task(&self.name(), "install", move |task| async move {
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
            let install_cmd_pure = package_manager.install_cmd(&tmp_path).unwrap();

            let mut cp_cmd = enter_cmd.clone();
            cp_cmd.extend("--", &cp_cmd_pure);
            let mut install_cmd = enter_cmd.clone();
            install_cmd.extend("--", &install_cmd_pure);

            this.root_store().spawn_terminal_cmd(name_clone.clone(), &cp_cmd).await?;
            this.root_store().spawn_terminal_cmd(name_clone, &install_cmd).await
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
                Ok(())
            });
        self.apps().reload();
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
                Ok(())
            });
        self.apps().reload();
    }
    pub fn clone_to(&self, target_name: &str) {
        let this = self.clone();
        let target_name_clone = target_name.to_string();
        let task = self
            .root_store()
            .create_task(&this.name(), "clone", move |task| async move {
                let child = this
                    .root_store()
                    .distrobox()
                    .clone_to(&this.name(), &target_name_clone)
                    .await?;
                task.handle_child_output(child).await?;
                Ok(())
            });
        self.root_store().view_task(&task);
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
                // this.load_container_infos();
                Ok(())
            });
    }
    pub fn spawn_terminal(&self) {
        let this = self.clone();
        self.root_store()
            .create_task(&self.name(), "spawn-terminal", move |_task| async move {
                let enter_cmd = this.root_store().distrobox().enter_cmd(&this.name());
                this.root_store()
                    .spawn_terminal_cmd(this.name(), &enter_cmd)
                    .await
            });
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}
