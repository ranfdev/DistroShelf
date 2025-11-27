// A container runtime is docker/podman/etc.

use std::{collections::HashSet, rc::Rc};

use async_trait::async_trait;
use serde::Deserialize;
use tracing::info;

use super::docker::Docker;

use crate::{backends::podman::Podman, fakers::CommandRunner};

#[async_trait(?Send)]
pub trait ContainerRuntime {
    fn name(&self) -> &'static str;
    async fn version(&self) -> anyhow::Result<String>;
    async fn usage(&self, container_id: &str) -> anyhow::Result<Usage>;
    async fn downloaded_images(&self) -> anyhow::Result<HashSet<String>>;
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Usage {
    #[serde(rename = "mem_usage", alias = "MemUsage")]
    pub mem_usage: String,
    #[serde(rename = "mem_percent", alias = "MemPerc")]
    pub mem_perc: String,
    #[serde(rename = "cpu_percent", alias = "CPU")]
    pub cpu_perc: String,
    #[serde(rename = "net_io", alias = "NetIO")]
    pub net_io: String,
    #[serde(rename = "block_io", alias = "BlockIO")]
    pub block_io: String,
    #[serde(rename = "pids", alias = "PIDs")]
    pub pids: String,
}

pub async fn get_container_runtime(
    command_runner: CommandRunner,
) -> Option<Rc<dyn ContainerRuntime>> {
    // Prefer Podman when both are available because Podman is rootless by default
    let podman = Podman::new(Rc::new(command_runner.clone()));
    if let Err(podman_err) = podman.version().await {
        let docker = Docker::new(Rc::new(command_runner));
        if let Err(docker_err) = docker.version().await {
            info!(docker = ?docker_err, podman = ?podman_err, "Container runtime check results");
            None
        } else {
            Some(Rc::new(docker) as Rc<dyn ContainerRuntime>)
        }
    } else {
        Some(Rc::new(podman) as Rc<dyn ContainerRuntime>)
    }
}
