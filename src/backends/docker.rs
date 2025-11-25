use std::{collections::HashSet, rc::Rc};

use async_trait::async_trait;
use tracing::{debug, error, info};

use crate::{
    backends::{container_runtime::{ContainerRuntime, Usage}},
    fakers::{Command, CommandRunner, FdMode},
    root_store::Image,
};

pub(crate) struct Docker {
    pub cmd_runner: Rc<CommandRunner>,
}

impl Docker {
    pub fn new(cmd_runner: Rc<CommandRunner>) -> Self {
        Self { cmd_runner }
    }
}

#[async_trait(?Send)]
impl ContainerRuntime for Docker {
    fn name(&self) -> &'static str {
        "docker"
    }
    async fn version(&self) -> anyhow::Result<String> {
        let mut cmd = Command::new("docker");
        cmd.arg("--version");

        let output = self.cmd_runner.output_string(cmd).await?;

        Ok(output.trim().to_string())
    }
    async fn downloaded_images(&self) -> anyhow::Result<HashSet<String>> {
        let mut cmd = Command::new("docker");
        cmd.arg("images").arg("--format").arg("json");

        let output = self.cmd_runner.output_string(cmd).await?;
        // Some versions of podman/docker might return empty string if no images?
        if output.trim().is_empty() {
            return Ok(HashSet::new());
        }

        // Handle potential JSON Lines vs JSON Array
        // Try parsing as array first
        let images_vec: Vec<Image> = match serde_json::from_str::<Vec<Image>>(&output) {
            Ok(images) => images,
            Err(_) => {
                // Try parsing as JSON lines
                let mut images = Vec::new();
                for line in output.lines() {
                    if !line.trim().is_empty() {
                        images.push(serde_json::from_str::<Image>(line)?);
                    }
                }
                images
            }
        };

        let names: HashSet<String> = images_vec
            .into_iter()
            .flat_map(|img| img.names.unwrap_or_default())
            .collect();

        Ok(names)
    }

    async fn usage(
        &self,
        container_id: &str,
    ) -> anyhow::Result<Usage> {
        let mut cmd = Command::new("docker");
        cmd.arg("stats");
        cmd.arg("--no-stream");
        cmd.arg("--format");
        cmd.arg("json");
        cmd.arg(container_id);
        cmd.stdout = crate::fakers::FdMode::Pipe;
        cmd.stderr = crate::fakers::FdMode::Pipe;

        let output = self.cmd_runner.output_string(cmd).await?;
        let usages: Vec<Usage> = serde_json::from_str(&output)?;

        usages
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No stats found"))
    }
}
