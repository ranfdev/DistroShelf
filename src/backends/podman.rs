use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use async_trait::async_trait;
use futures::Stream;
use serde::Deserialize;

use crate::{
    backends::container_runtime::{ContainerRuntime, Usage},
    fakers::{Child, Command, CommandRunner, FdMode},
};

pub fn map_docker_to_podman(mut command: Command) -> Command {
    if command.program == "docker" {
        command.program = "podman".into();
    }
    command
}

/// Podman event structure
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PodmanEvent {
    #[serde(rename = "ID")]
    pub id: Option<String>,
    pub name: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "Type")]
    pub event_type: Option<String>,
    pub attributes: Option<HashMap<String, String>>,
}

impl PodmanEvent {
    /// Check if this event is for a distrobox container
    pub fn is_distrobox(&self) -> bool {
        self.attributes
            .as_ref()
            .and_then(|attrs| attrs.get("manager"))
            .map(|manager| manager == "distrobox")
            .unwrap_or(false)
    }

    /// Check if this is a container event
    pub fn is_container_event(&self) -> bool {
        self.event_type
            .as_ref()
            .map(|t| t == "container")
            .unwrap_or(false)
    }
}

/// Stream wrapper for podman events
pub struct PodmanEventStream {
    lines: Option<
        futures::io::Lines<futures::io::BufReader<Box<dyn futures::io::AsyncRead + Send + Unpin>>>,
    >,
    _child: Option<Box<dyn Child + Send>>,
}

impl Stream for PodmanEventStream {
    type Item = Result<String, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(ref mut lines) = self.lines {
            Pin::new(lines).poll_next(cx)
        } else {
            Poll::Ready(None)
        }
    }
}

// This is a wrapper around Docker that maps commands to Podman
pub struct Podman {
    docker: crate::backends::docker::Docker,
}

impl Podman {
    pub fn new(cmd_runner: Rc<CommandRunner>) -> Self {
        Self {
            docker: crate::backends::docker::Docker::new(Rc::new(
                cmd_runner.map_cmd(map_docker_to_podman),
            )),
        }
    }
    /// Listen to podman events and return a stream of event lines
    pub fn listen_events(&self) -> Result<PodmanEventStream, std::io::Error> {
        use futures::io::{AsyncBufReadExt, BufReader};

        // Create the podman events command
        let mut cmd = Command::new("podman");
        cmd.arg("events");
        cmd.arg("--format");
        cmd.arg("json");
        cmd.stdout = FdMode::Pipe;
        cmd.stderr = FdMode::Pipe;

        // Spawn the command
        let mut child = self.docker.cmd_runner.spawn(cmd)?;

        // Get stdout and create a buffered reader
        let stdout = child
            .take_stdout()
            .ok_or_else(|| std::io::Error::other("No stdout available"))?;

        let bufread = BufReader::new(stdout);
        let lines = bufread.lines();

        Ok(PodmanEventStream {
            lines: Some(lines),
            _child: Some(child),
        })
    }
}

#[async_trait(?Send)]
impl ContainerRuntime for Podman {
    fn name(&self) -> &'static str {
        "podman"
    }

    async fn version(&self) -> anyhow::Result<String> {
        self.docker.version().await
    }

    async fn usage(&self, container_id: &str) -> anyhow::Result<Usage> {
        self.docker.usage(container_id).await
    }

    async fn downloaded_images(&self) -> anyhow::Result<HashSet<String>> {
        self.docker.downloaded_images().await
    }
}
