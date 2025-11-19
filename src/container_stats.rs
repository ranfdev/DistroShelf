use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerRuntime {
    Podman,
    Docker,
}

impl ContainerRuntime {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContainerRuntime::Podman => "podman",
            ContainerRuntime::Docker => "docker",
        }
    }
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
