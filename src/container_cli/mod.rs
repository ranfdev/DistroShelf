mod command;
mod command_runner;
mod desktop_file;
mod distrobox;

use async_trait::async_trait;


#[async_trait(?Send)]
pub trait ContainerCli {
    async fn list_apps(&self, box_name: &str) -> Result<Vec<ExportableApp>, Error>;
    fn launch_app(
        &self,
        container: &str,
        app: &ExportableApp,
    ) -> Result<Box<dyn Child + Send>, Error>;
    async fn export_app(
        &self,
        container: &str,
        desktop_file_path: &str,
    ) -> Result<String, Error>;
    async fn unexport_app(
        &self,
        container: &str,
        desktop_file_path: &str,
    ) -> Result<String, Error>;
    fn assemble(&self, file_path: &str) -> Result<Box<dyn Child + Send>, Error>;
    fn assemble_from_url(&self, url: &str) -> Result<Box<dyn Child + Send>, Error>;
    // TODO why is this async and assemble not?
    async fn create(&self, args: CreateArgs) -> Result<Box<dyn Child + Send>, Error>;
    async fn list_images(&self) -> Result<Vec<String>, Error>;
    fn enter_cmd(&self, name: &str) -> Command;
    async fn clone_to(
        &self,
        source_name: &str,
        target_name: &str,
    ) -> Result<Box<dyn Child + Send>, Error>;
    async fn list(&self) -> Result<BTreeMap<String, ContainerInfo>, Error>;
    async fn remove(&self, name: &str) -> Result<String, Error>;
    async fn stop(&self, name: &str) -> Result<String, Error>;
    fn upgrade(&self, name: &str) -> Result<Box<dyn Child + Send>, Error>;
    async fn version(&self) -> Result<String, Error>;
    async fn stop_all(&self) -> Result<String, Error>;
    async fn upgrade_all(&mut self) -> Result<String, Error>;
}

use std::collections::BTreeMap;

pub use command::*;
pub use command_runner::*;
pub use desktop_file::*;
pub use distrobox::*;