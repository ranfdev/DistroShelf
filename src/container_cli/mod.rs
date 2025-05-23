mod command;
mod command_runner;
mod desktop_file;
mod distrobox;
mod toolbox;

use async_trait::async_trait;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to read command stdout: {0}")]
    StdoutRead(#[from] io::Error),

    #[error("failed to spawn command {command}: {source}")]
    Spawn { source: io::Error, command: String },

    #[error("failed to parse command output: {0}")]
    ParseOutput(String),

    #[error("invalid field {0}: {1}")]
    InvalidField(String, String),

    #[error("command failed with exit code {exit_code:?}: {command}\n{stderr}")]
    CommandFailed {
        exit_code: Option<i32>,
        command: String,
        stderr: String,
    },
}

#[derive(Default, Debug, PartialEq, Clone)]
pub struct CreateArgName(String);

impl std::fmt::Display for CreateArgName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl CreateArgName {
    pub fn new(value: &str) -> Result<Self, Error> {
        let re = regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*$").unwrap();
        if re.is_match(value) {
            Ok(CreateArgName(value.to_string()))
        } else {
            Err(Error::InvalidField(
                "name".into(),
                "Must respect the format [a-zA-Z0-9][a-zA-Z0-9_.-]*".into(),
            ))
        }
    }
}

#[derive(Default, Debug, PartialEq, Clone)]
pub struct CreateArgs {
    pub init: bool,
    pub nvidia: bool,
    pub home_path: Option<String>,
    pub image: String,
    pub name: CreateArgName,
    pub volumes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Hash)]
pub enum Status {
    Up(String),
    Created(String),
    Exited(String),
    // I don't want the app to crash if the parsing fails because distrobox changed with an update.
    // We will just disable some features, but still show the status value.
    Other(String),
}

impl Default for Status {
    fn default() -> Self {
        Self::Other("".into())
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Up(s) => write!(f, "Up {}", s),
            Status::Created(s) => write!(f, "Created {}", s),
            Status::Exited(s) => write!(f, "Exited {}", s),
            Status::Other(s) => write!(f, "{}", s),
        }
    }
}

impl Status {
    fn from_str(s: &str) -> Self {
        if let Some(rest) = s.strip_prefix("Up") {
            Status::Up(rest.trim().to_string())
        } else if let Some(rest) = s.strip_prefix("Exited") {
            Status::Exited(rest.trim().to_string())
        } else if let Some(rest) = s.strip_prefix("Created") {
            Status::Created(rest.trim().to_string())
        } else {
            Status::Other(s.to_string())
        }
    }
}

#[derive(Debug, PartialEq, Hash, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub status: Status,
    pub image: String,
}

impl ContainerInfo {
    fn field_missing_error(text: &str, line: &str) -> Error {
        Error::ParseOutput(format!("{text} missing in line: {}", line))
    }
}

impl FromStr for ContainerInfo {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('|').collect();
        if parts.len() != 4 {
            return Err(Error::ParseOutput(format!(
                "Invalid field count (expected 4, got {}) in line: {}",
                parts.len(),
                s
            )));
        }

        let id = parts[0].trim();
        let name = parts[1].trim();
        let status = parts[2].trim();
        let image = parts[3].trim();

        // Check for empty fields
        if id.is_empty() {
            return Err(ContainerInfo::field_missing_error("id", s));
        }
        if name.is_empty() {
            return Err(ContainerInfo::field_missing_error("name", s));
        }
        if status.is_empty() {
            return Err(ContainerInfo::field_missing_error("status", s));
        }
        if image.is_empty() {
            return Err(ContainerInfo::field_missing_error("image", s));
        }

        Ok(ContainerInfo {
            id: id.to_string(),
            name: name.to_string(),
            status: Status::from_str(status),
            image: image.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ExportableApp {
    pub entry: DesktopEntry,
    pub desktop_file_path: String,
    pub exported: bool,
}


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

use std::{collections::BTreeMap, io, str::FromStr};

pub use command::*;
pub use command_runner::*;
pub use desktop_file::*;
pub use distrobox::*;
pub use toolbox::*;