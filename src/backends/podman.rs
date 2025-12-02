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
    #[allow(dead_code)]
    #[serde(rename = "ID")]
    pub id: Option<String>,
    #[allow(dead_code)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_docker_to_podman() {
        let cmd = Command::new("docker");
        let mapped = map_docker_to_podman(cmd);

        assert_eq!(mapped.program.to_string_lossy(), "podman");
    }

    #[test]
    fn test_map_docker_to_podman_with_args() {
        let mut cmd = Command::new("docker");
        cmd.args(["ps", "-a"]);

        let mapped = map_docker_to_podman(cmd);

        assert_eq!(mapped.program.to_string_lossy(), "podman");
        assert_eq!(mapped.args[0].to_string_lossy(), "ps");
        assert_eq!(mapped.args[1].to_string_lossy(), "-a");
    }

    #[test]
    fn test_map_docker_to_podman_non_docker() {
        let cmd = Command::new("other-command");
        let mapped = map_docker_to_podman(cmd);

        // Non-docker commands should not be changed
        assert_eq!(mapped.program.to_string_lossy(), "other-command");
    }

    #[test]
    fn test_podman_event_is_distrobox() {
        let mut attrs = HashMap::new();
        attrs.insert("manager".to_string(), "distrobox".to_string());

        let event = PodmanEvent {
            id: Some("abc123".to_string()),
            name: Some("my-container".to_string()),
            status: Some("start".to_string()),
            event_type: Some("container".to_string()),
            attributes: Some(attrs),
        };

        assert!(event.is_distrobox());
    }

    #[test]
    fn test_podman_event_not_distrobox() {
        let mut attrs = HashMap::new();
        attrs.insert("manager".to_string(), "other".to_string());

        let event = PodmanEvent {
            id: Some("abc123".to_string()),
            name: None,
            status: None,
            event_type: None,
            attributes: Some(attrs),
        };

        assert!(!event.is_distrobox());
    }

    #[test]
    fn test_podman_event_no_attributes() {
        let event = PodmanEvent {
            id: None,
            name: None,
            status: None,
            event_type: None,
            attributes: None,
        };

        assert!(!event.is_distrobox());
    }

    #[test]
    fn test_podman_event_is_container_event() {
        let event = PodmanEvent {
            id: None,
            name: None,
            status: None,
            event_type: Some("container".to_string()),
            attributes: None,
        };

        assert!(event.is_container_event());
    }

    #[test]
    fn test_podman_event_not_container_event() {
        let event = PodmanEvent {
            id: None,
            name: None,
            status: None,
            event_type: Some("image".to_string()),
            attributes: None,
        };

        assert!(!event.is_container_event());
    }

    #[test]
    fn test_podman_event_no_type() {
        let event = PodmanEvent {
            id: None,
            name: None,
            status: None,
            event_type: None,
            attributes: None,
        };

        assert!(!event.is_container_event());
    }
}
