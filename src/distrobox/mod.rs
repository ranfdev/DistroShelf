use std::{
    cell::{LazyCell, RefCell}, collections::BTreeMap, io, path::{Path, PathBuf}, process::Output, rc::Rc, str::FromStr
};
use tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

mod command;
mod command_runner;
mod desktop_file;

pub use command::*;
pub use command_runner::*;
pub use desktop_file::*;

#[derive(Default, Clone, Debug)]
pub struct OutputTracker<T> {
    store: Rc<RefCell<Option<Vec<T>>>>,
}

impl<T: Clone + std::fmt::Debug> OutputTracker<T> {
    pub fn enable(&self) {
        let mut inner = self.store.borrow_mut();
        if inner.is_none() {
            *inner = Some(vec![]);
        }
    }
    pub fn push(&self, item: T) {
        if let Some(v) = &mut *self.store.borrow_mut() {
            v.push(item);
        }
    }
    pub fn items(&self) -> Vec<T> {
        if let Some(v) = &*self.store.borrow() {
            v.clone()
        } else {
            vec![]
        }
    }
}

pub struct Distrobox {
    cmd_runner: Box<dyn CommandRunner>,
    output_tracker: OutputTracker<String>,
    is_in_flatpak: bool,
}

impl std::fmt::Debug for Distrobox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Distrobox")
            .field("is_in_flatpak", &self.is_in_flatpak)
            .field("output_tracker", &self.output_tracker)
            .finish()
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportableApp {
    pub entry: DesktopEntry,
    pub desktop_file_path: String,
    pub exported: bool,
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

fn dbcmd() -> Command {
    Command::new("distrobox")
}

#[derive(Clone)]
pub enum DistroboxCommandRunnerResponse {
    Version,
    NoVersion,
    List(Vec<ContainerInfo>),
    Compatibility(Vec<String>),
    // The exported apps commands is complex, it may fail, but we don't want the app to crash
    ExportedApps(String, Vec<(String, String, String)>), // (distrobox_name, [(filename, name, icon)])
}

impl DistroboxCommandRunnerResponse {
    pub fn common_distros() -> LazyCell<Vec<ContainerInfo>> {
        LazyCell::new(|| {
            [
                ("1", "Ubuntu", "docker.io/library/ubuntu:latest"),
                ("2", "Fedora", "docker.io/library/fedora:latest"),
                ("3", "Kali", "docker.io/kalilinux/kali-rolling"),
                ("4", "Debian", "docker.io/library/debian:latest"),
                ("5", "Arch Linux", "docker.io/library/archlinux:latest"),
                ("6", "CentOS", "docker.io/library/centos:latest"),
                ("7", "Alpine", "docker.io/library/alpine:latest"),
                ("8", "OpenSUSE", "docker.io/library/opensuse:latest"),
                ("9", "Gentoo", "docker.io/library/gentoo:latest"),
                ("10", "Slackware", "docker.io/library/slackware:latest"),
                ("11", "Void Linux", "docker.io/library/voidlinux:latest"),
                ("13", "Deepin", "docker.io/library/deepin:latest"),
                ("16", "Rocky Linux", "docker.io/library/rockylinux:latest"),
                (
                    "17",
                    "Crystal Linux",
                    "docker.io/library/crystal-linux:latest",
                ),
            ]
            .iter()
            .map(|(id, name, image)| ContainerInfo {
                id: id.to_string(),
                name: name.to_string(),
                status: Status::Created("2 minutes ago".into()),
                image: image.to_string(),
            })
            .collect()
        })
    }

    pub fn new_list_common_distros() -> Self {
        Self::List(Self::common_distros().to_owned())
    }

    pub fn new_common_exported_apps() -> Self {
        let dummy_exported_apps = vec![
            ("vim.desktop".into(), "Vim".into(), "vim".into()),
            ("matlab.desktop".into(), "MATLAB".into(), "matlab".into()),
            (
                "vscode.desktop".into(),
                "Visual Studio Code".into(),
                "code".into(),
            ),
            ("rstudio.desktop".into(), "RStudio".into(), "rstudio".into()),
            (
                "sublime_text.desktop".into(),
                "Sublime Text".into(),
                "subl".into(),
            ),
            ("zoom.desktop".into(), "Zoom".into(), "zoom".into()),
            ("slack.desktop".into(), "Slack".into(), "slack".into()),
            ("postman.desktop".into(), "Postman".into(), "postman".into()),
        ];
        DistroboxCommandRunnerResponse::ExportedApps("Ubuntu".into(), dummy_exported_apps)
    }

    pub fn new_common_images() -> Self {
        DistroboxCommandRunnerResponse::Compatibility(
            Self::common_distros()
                .iter()
                .map(|x| x.image.clone())
                .collect(),
        )
    }

    fn build_version_response() -> (Command, String) {
        let mut cmd = Command::new("distrobox");
        cmd.arg("version");
        (cmd, "distrobox: 1.7.2.1".to_string())
    }

    fn build_no_version_response() -> (Command, Rc<dyn Fn() -> io::Result<String>>) {
        let mut cmd = Command::new("distrobox");
        cmd.arg("version");
        (cmd, Rc::new(|| Err(io::Error::from_raw_os_error(0))))
    }

    fn build_list_response(containers: &[ContainerInfo]) -> (Command, String) {
        let mut output = String::new();
        output.push_str("ID           | NAME                 | STATUS             | IMAGE  \n");
        for container in containers {
            output.push_str(&container.id);
            output.push_str(" | ");
            output.push_str(&container.name);
            output.push_str(" | ");
            let status = container.status.to_string();
            output.push_str(&format!("{status} | "));
            output.push_str(&container.image);
            output.push('\n');
        }
        let mut cmd = Command::new("distrobox");
        cmd.arg("ls").arg("--no-color");
        (cmd, output.clone())
    }

    fn build_compatibility_response(images: &[String]) -> (Command, String) {
        let output = images.join("\n");
        let mut cmd = Command::new("distrobox");
        cmd.arg("create").arg("--compatibility");
        (cmd, output)
    }

    fn build_exported_apps_commands(
        box_name: &str,
        apps: &[(String, String, String)],
    ) -> Vec<(Command, String)> {
        let mut commands = Vec::new();

        // Get XDG_DATA_HOME
        commands.push((
            Command::new_with_args("sh", ["-c", "echo $XDG_DATA_HOME"]),
            String::new(),
        ));

        // Get HOME if XDG_DATA_HOME is empty
        commands.push((
            Command::new_with_args("sh", ["-c", "echo $HOME"]),
            "/home/me".to_string(),
        ));

        // List desktop files
        let file_list = apps
            .iter()
            .map(|(filename, _, _)| format!("ubuntu-{}", filename))
            .collect::<Vec<_>>()
            .join("\n");
        commands.push((
            Command::new_with_args("ls", ["/home/me/.local/share/applications"]),
            file_list,
        ));

        // Get desktop file contents
        let mut contents = String::new();
        for (filename, name, icon) in apps {
            contents.push_str(&format!(
                "# START FILE /usr/share/applications/{}\n\
                [Desktop Entry]\n\
                Type=Application\n\
                Name={}\n\
                Exec=/path/to/{}\n\
                Icon={}\n\
                Categories=Utility;Network;\n\n",
                filename, name, name, icon
            ));
        }
        commands.push((
            Command::new_with_args("distrobox", 
                ["enter", box_name, "--", "sh", "-c", "for file in $(grep --files-without-match \"NoDisplay=true\" /usr/share/applications/*.desktop); do echo \"# START FILE $file\"; cat \"$file\"; done"]),
            contents
        ));

        commands
    }

    fn wrap_err_fn(output: (Command, String)) -> (Command, Rc<dyn Fn() -> io::Result<String>>) {
        (output.0, Rc::new(move || Ok(output.1.clone())))
    }

    pub fn to_commands(self) -> Vec<(Command, Rc<dyn Fn() -> Result<String, io::Error>>)> {
        match self {
            Self::Version => {
                let working_response = Self::build_version_response();
                vec![Self::wrap_err_fn(working_response)]
            }
            Self::NoVersion => {
                vec![Self::build_no_version_response()]
            }
            Self::List(containers) => {
                vec![Self::wrap_err_fn(Self::build_list_response(&containers))]
            }
            Self::Compatibility(images) => vec![Self::wrap_err_fn(
                Self::build_compatibility_response(&images),
            )],
            Self::ExportedApps(box_name, apps) => {
                Self::build_exported_apps_commands(&box_name, &apps)
                    .into_iter()
                    .map(Self::wrap_err_fn)
                    .collect()
            }
        }
    }
}

impl Distrobox {
    pub fn new() -> Self {
        Self {
            cmd_runner: Box::new(RealCommandRunner {}),
            is_in_flatpak: Self::get_is_in_flatpak(),
            output_tracker: Default::default(),
        }
    }
    pub fn new_null(runner: NullCommandRunner, is_in_flatpak: bool) -> Self {
        Self {
            cmd_runner: Box::new(runner),
            output_tracker: OutputTracker::default(),
            is_in_flatpak,
        }
    }

    pub fn new_null_with_responses(
        responses: &[DistroboxCommandRunnerResponse],
        is_in_flatpak: bool,
    ) -> Self {
        let cmd_runner = {
            let mut builder = NullCommandRunnerBuilder::new();
            for res in responses {
                for (cmd, out) in res.clone().to_commands() {
                    builder.cmd_full(cmd, out.clone());
                }
            }
            builder.build()
        };
        Self {
            cmd_runner: Box::new(cmd_runner),
            output_tracker: OutputTracker::default(),
            is_in_flatpak,
        }
    }

    pub fn output_tracker(&self) -> OutputTracker<String> {
        self.output_tracker.enable();
        self.output_tracker.clone()
    }

    fn get_is_in_flatpak() -> bool {
        let fp_env = std::env::var("FLATPAK_ID").is_ok();
        if fp_env {
            return true;
        }

        Path::new("/.flatpak-info").exists()
    }

    pub fn cmd_spawn(&self, cmd: Command) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = if self.is_in_flatpak {
            wrap_flatpak_cmd(cmd)
        } else {
            cmd
        };
        wrap_capture_cmd(&mut cmd);

        let program = cmd.program.to_string_lossy().to_string();
        let args = cmd
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        debug!(command = %program, args = ?args, "Spawning command");
        let child = self.cmd_runner.spawn(cmd.clone()).map_err(|e| {
            let full_command = format!("{:?} {:?}", program, args);
            error!(error = ?e, command = %full_command, "Command spawn failed");
            Error::Spawn {
                source: e,
                command: full_command,
            }
        })?;

        self.output_tracker
            .push(format!("{:?} {:?}", program, args));
        Ok(child)
    }

    async fn cmd_output(&self, cmd: Command) -> Result<Output, Error> {
        let mut cmd = if self.is_in_flatpak {
            wrap_flatpak_cmd(cmd)
        } else {
            cmd
        };
        wrap_capture_cmd(&mut cmd);

        let program = cmd.program.to_string_lossy().to_string();
        let args = cmd
            .args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        info!(command = %program, args = ?args, "Executing command");
        let command_str = format!("{:?} {:?}", program, args);

        let output = self.cmd_runner.output(cmd).await.map_err(|e| {
            error!(error = ?e, command = %program, "Command execution failed");
            Error::Spawn {
                source: e,
                command: command_str.clone(),
            }
        })?;

        self.output_tracker.push(command_str.clone());

        let exit_code = output.status.code();
        debug!(
            exit_code = ?exit_code,
            "Command completed successfully"
        );
        Ok(output)
    }

    async fn cmd_output_string(&self, cmd: Command) -> Result<String, Error> {
        let command_str = format!("{:?} {:?}", cmd.program, cmd.args);
        let output = self.cmd_output(cmd).await?;
        let s = String::from_utf8_lossy(&output.stdout);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let exit_code = output.status.code();
            error!(
                exit_code = ?exit_code,
                stderr = %stderr,
                "Command failed"
            );
            return Err(Error::CommandFailed {
                exit_code,
                command: command_str,
                stderr,
            });
        }

        Ok(s.to_string())
    }

    async fn host_applications_path(&self) -> Result<PathBuf, Error> {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "echo $XDG_DATA_HOME"]);
        let xdg_data_home = self.cmd_output_string(cmd).await?;

        let xdg_data_home = if xdg_data_home.trim().is_empty() {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", "echo $HOME"]);
            let home = self.cmd_output_string(cmd).await?;
            Path::new(home.trim()).join(".local/share")
        } else {
            Path::new(xdg_data_home.trim()).to_path_buf()
        };
        let apps_path = xdg_data_home.join("applications");
        Ok(apps_path)
    }
    async fn get_exported_desktop_files(&self) -> Result<Vec<String>, Error> {
        // We do everything with the command line to ensure we can access the files and environment variables
        // even when inside a flatpak sandbox, with only the permissions to run `flatpak-spawn`
        let mut cmd = Command::new("ls");
        cmd.arg(self.host_applications_path().await?);
        let ls_out = self.cmd_output_string(cmd).await?;
        let apps = ls_out
            .trim()
            .split("\n")
            .map(|app| app.to_string())
            .collect::<Vec<_>>();
        Ok(apps)
    }

    async fn get_desktop_files(&self, box_name: &str) -> Result<Vec<(String, String)>, Error> {
        let mut cmd = dbcmd();
        cmd.args([
            "enter",
            box_name,
            "--",
            "sh",
            "-c",
            "for file in $(grep --files-without-match \"NoDisplay=true\" /usr/share/applications/*.desktop); do echo \"# START FILE $file\"; cat \"$file\"; done",
        ]);
        let concatenated_files = self.cmd_output_string(cmd).await?;
        debug!(concatenated_files = concatenated_files);
        let res = concatenated_files
            .split("# START FILE ")
            .skip(1)
            .map(|file_content| {
                let file_path = file_content.lines().next().map(|name| name.trim_start());
                (
                    file_path.unwrap_or_default().to_string(),
                    file_content.to_string(),
                )
            })
            .collect();
        Ok(res)
    }

    pub async fn list_apps(&self, box_name: &str) -> Result<Vec<ExportableApp>, Error> {
        let files = self.get_desktop_files(box_name).await?;
        debug!(desktop_files=?files);
        let exported = self.get_exported_desktop_files().await?;
        debug!(exported_files=?exported);
        let res: Vec<ExportableApp> = files
            .into_iter()
            .flat_map(|(path, content)| -> Option<ExportableApp> {
                let entry = parse_desktop_file(&content);
                let file_name = Path::new(&path)
                    .file_name()
                    .map(|x| x.to_str())
                    .unwrap_or_default()
                    .unwrap_or_default();

                let exported_as = format!("{box_name}-{file_name}");
                let is_exported = exported.contains(&exported_as);
                if is_exported {
                    debug!(found_exported = exported_as);
                }
                entry.map(|entry| ExportableApp {
                    desktop_file_path: path,
                    entry,
                    exported: is_exported,
                })
            })
            .collect();

        Ok(res)
    }

    pub fn launch_app(
        &self,
        container: &str,
        app: &ExportableApp,
    ) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = dbcmd();
        cmd.arg("enter").arg("--name").arg(container).arg("--");
        let to_be_replaced = [" %f", " %u", " %F", " %U"];
        let cleaned_exec = to_be_replaced
            .into_iter()
            .fold(app.entry.exec.clone(), |acc, x| acc.replace(x, ""));
        cmd.arg(cleaned_exec);
        self.cmd_spawn(cmd)
    }

    pub async fn export_app(
        &self,
        container: &str,
        desktop_file_path: &str,
    ) -> Result<String, Error> {
        let mut cmd = dbcmd();
        cmd.args(["enter", "--name", container]).extend(
            "--",
            &Command::new_with_args("distrobox-export", ["--app", desktop_file_path]),
        );

        self.cmd_output_string(cmd).await
    }
    pub async fn unexport_app(
        &self,
        container: &str,
        desktop_file_path: &str,
    ) -> Result<String, Error> {
        let mut cmd = dbcmd();
        cmd.args(["enter", "--name", container]).extend(
            "--",
            &Command::new_with_args("distrobox-export", ["-d", "--app", desktop_file_path]),
        );

        self.cmd_output_string(cmd).await
    }

    // assemble
    pub fn assemble(&self, file_path: &str) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = dbcmd();
        cmd.arg("assemble").arg("--file").arg(file_path);
        self.cmd_spawn(cmd)
    }

    pub fn assemble_from_url(&self, url: &str) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = dbcmd();
        cmd.arg("assemble").arg("--file").arg(url);
        self.cmd_spawn(cmd)
    }
    // create
    pub async fn create(&self, args: CreateArgs) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = dbcmd();
        cmd.arg("create").arg("--yes");
        if !args.image.is_empty() {
            cmd.arg("--image").arg(args.image);
        }
        if !args.name.0.is_empty() {
            cmd.arg("--name").arg(args.name.0);
        }
        if args.init {
            cmd.arg("--init");
        }
        if args.nvidia {
            cmd.arg("--nvidia");
        }
        if let Some(home_path) = args.home_path {
            cmd.arg("--home").arg(home_path);
        }
        for volume in args.volumes {
            cmd.arg("--volume").arg(volume);
        }
        self.cmd_spawn(cmd)
    }
    // create --compatibility
    pub async fn list_images(&self) -> Result<Vec<String>, Error> {
        let mut cmd = dbcmd();
        cmd.arg("create").arg("--compatibility");
        let text = self.cmd_output_string(cmd).await?;
        let lines = text
            .lines()
            .filter_map(|x| {
                if !x.is_empty() {
                    Some(x.to_string())
                } else {
                    None
                }
            })
            .collect();
        Ok(lines)
    }
    // enter
    pub fn enter_cmd(&self, name: &str) -> Command {
        let mut cmd = dbcmd();
        cmd.arg("enter").arg(name);
        cmd
    }
    // clone
    pub async fn clone_to(
        &self,
        source_name: &str,
        target_name: &str,
    ) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = dbcmd();
        cmd.arg("create")
            .arg("--name")
            .arg(target_name)
            .arg("--clone")
            .arg(source_name);
        self.cmd_spawn(cmd)
    }
    // list | ls
    pub async fn list(&self) -> Result<BTreeMap<String, ContainerInfo>, Error> {
        let mut cmd = dbcmd();
        cmd.arg("ls").arg("--no-color");
        let text = self.cmd_output_string(cmd).await?;
        let lines = text.lines().skip(1);
        let mut out = BTreeMap::new();
        for line in lines {
            match line.parse::<ContainerInfo>() {
                Ok(item) => {
                    debug!(
                        container_id = %item.id,
                        container_name = %item.name,
                        image = %item.image,
                        status = ?item.status,
                        "Discovered container"
                    );
                    out.insert(item.name.clone(), item);
                }
                Err(e) => {
                    error!(error = %e, line = %line, "Failed to parse container info");
                    return Err(e);
                }
            }
        }
        Ok(out)
    }
    // rm
    pub async fn remove(&self, name: &str) -> Result<String, Error> {
        let mut cmd = dbcmd();
        cmd.arg("rm").arg("--force").arg(name);
        self.cmd_output_string(cmd).await
    }
    // stop
    pub async fn stop(&self, name: &str) -> Result<String, Error> {
        let mut cmd = dbcmd();
        cmd.arg("stop").arg("--yes").arg(name);
        self.cmd_output_string(cmd).await
    }
    pub async fn stop_all(&self) -> Result<String, Error> {
        let mut cmd = dbcmd();
        cmd.arg("stop").arg("--all").arg("--yes");
        self.cmd_output_string(cmd).await
    }
    // upgrade
    pub fn upgrade(&self, name: &str) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = dbcmd();
        cmd.arg("upgrade").arg(name);

        self.cmd_spawn(cmd)
    }
    pub async fn upgrade_all(&mut self) -> Result<String, Error> {
        let mut cmd = dbcmd();
        cmd.arg("upgrade").arg("--all");
        self.cmd_output_string(cmd).await
    }
    // ephemeral
    // generate-entry
    // version
    pub async fn version(&self) -> Result<String, Error> {
        let mut cmd = dbcmd();
        cmd.arg("version");
        let text = self.cmd_output_string(cmd).await?;
        let mut parts = text.split(':');
        if let Some(v) = parts.nth(1) {
            let version = v.trim().to_string();
            info!(
                distrobox_version = %version,
                raw_output = %text,
                "Successfully parsed distrobox version"
            );
            Ok(version)
        } else {
            warn!(output = %text, "Failed to parse version from output");
            Err(Error::ParseOutput(format!(
                "Failed to parse version from output: {}",
                text
            )))
        }
    }

    // help
}

impl Default for Distrobox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smol::block_on;

    #[test]
    fn list() -> Result<(), Error> {
        block_on(async {
            let output = "ID           | NAME                 | STATUS             | IMAGE                         
d24405b14180 | ubuntu               | Created            | ghcr.io/ublue-os/ubuntu-toolbox:latest";
            let db = Distrobox::new_null(
                NullCommandRunnerBuilder::new()
                    .cmd(&["distrobox", "ls", "--no-color"], output)
                    .build(),
                false,
            );
            assert_eq!(
                db.list().await?,
                BTreeMap::from_iter([("ubuntu".into(), ContainerInfo {
                    id: "d24405b14180".into(),
                    name: "ubuntu".into(),
                    status: Status::Created("".into()),
                    image: "ghcr.io/ublue-os/ubuntu-toolbox:latest".into(),
                })])
            );
            Ok(())
        })
    }

    #[test]
    fn version() -> Result<(), Error> {
        block_on(async {
            let output = "distrobox: 1.7.2.1";
            let db = Distrobox::new_null(
                NullCommandRunnerBuilder::new()
                    .cmd(&["distrobox", "version"], output)
                    .build(),
                false,
            );
            assert_eq!(db.version().await?, "1.7.2.1".to_string(),);
            Ok(())
        })
    }

    #[test]
    fn list_apps() -> Result<(), Error> {
        let db = Distrobox::new_null(NullCommandRunnerBuilder::new()
            .cmd(
                &[
                    "sh", "-c", "echo $XDG_DATA_HOME"
                ],
                ""
            )
            .cmd(
                &[
                    "sh", "-c", "echo $HOME"
                ],
                "/home/me"
            )
            .cmd(
                &[
                    "ls", "/home/me/.local/share/applications"
                ],
                "ubuntu-vim.desktop\n"
            )
            .cmd(
                &[
            "distrobox",
            "enter",
            "ubuntu",
            "--",
            "sh",
            "-c",
            "for file in $(grep --files-without-match \"NoDisplay=true\" /usr/share/applications/*.desktop); do echo \"# START FILE $file\"; cat \"$file\"; done",
        ],
            "# START FILE /usr/share/applications/vim.desktop
[Desktop Entry]
Type=Application
Name=Vim
Exec=/path/to/vim
Icon=/path/to/icon.png
Comment=A brief description of my application
Categories=Utility;Network;
# START FILE /usr/share/applications/fish.desktop
[Desktop Entry]
Type=Application
Name=Fish
Exec=/path/to/fish
Icon=/path/to/icon.png
Comment=A brief description of my application
Categories=Utility;Network;
",)
            .build(),
            false
        );

        let apps = block_on(db.list_apps("ubuntu"))?;
        assert_eq!(&apps[0].entry.name, "Vim");
        assert_eq!(&apps[0].entry.exec, "/path/to/vim");
        assert!(apps[0].exported);
        assert_eq!(&apps[1].entry.name, "Fish");
        assert_eq!(&apps[1].entry.exec, "/path/to/fish");
        assert!(!apps[1].exported);
        Ok(())
    }
    #[test]
    fn create() -> Result<(), Error> {
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
        let db = Distrobox::new_null(NullCommandRunner::default(), false);
        let output_tracker = db.output_tracker();
        debug!("Testing container creation");
        let args = CreateArgs {
            image: "docker.io/library/ubuntu:latest".into(),
            init: true,
            nvidia: true,
            home_path: Some("/home/me".into()),
            volumes: vec!["/mnt/sdb1".into(), "/mnt/sdb4".into()],
            ..Default::default()
        };

        block_on(db.create(args))?;
        let expected = "\"distrobox\" [\"create\", \"--yes\", \"--image\", \"docker.io/library/ubuntu:latest\", \"--init\", \"--nvidia\", \"--home\", \"/home/me\", \"--volume\", \"/mnt/sdb1\", \"--volume\", \"/mnt/sdb4\"]";
        assert_eq!(output_tracker.items()[0], expected);
        Ok(())
    }
    #[test]
    fn assemble() -> Result<(), Error> {
        let db = Distrobox::new_null(NullCommandRunner::default(), false);
        let output_tracker = db.output_tracker();
        db.assemble("/path/to/assemble.yml")?;
        assert_eq!(
            output_tracker.items()[0],
            "\"distrobox\" [\"assemble\", \"--file\", \"/path/to/assemble.yml\"]"
        );
        Ok(())
    }

    #[test]
    fn remove() -> Result<(), Error> {
        let db = Distrobox::new_null(NullCommandRunner::default(), false);
        let output_tracker = db.output_tracker();
        block_on(db.remove("ubuntu"))?;
        assert_eq!(
            output_tracker.items()[0],
            "\"distrobox\" [\"rm\", \"--force\", \"ubuntu\"]"
        );
        Ok(())
    }

    #[test]
    fn stub_responses() {
        let cmd_outputs = DistroboxCommandRunnerResponse::new_list_common_distros().to_commands();
        assert_eq!(
            cmd_outputs[0].1().unwrap(),
            "ID           | NAME                 | STATUS             | IMAGE  
1 | Ubuntu | Created 2 minutes ago | docker.io/library/ubuntu:latest
2 | Fedora | Created 2 minutes ago | docker.io/library/fedora:latest
3 | Kali | Created 2 minutes ago | docker.io/kalilinux/kali-rolling
4 | Debian | Created 2 minutes ago | docker.io/library/debian:latest
5 | Arch Linux | Created 2 minutes ago | docker.io/library/archlinux:latest
6 | CentOS | Created 2 minutes ago | docker.io/library/centos:latest
7 | Alpine | Created 2 minutes ago | docker.io/library/alpine:latest
8 | OpenSUSE | Created 2 minutes ago | docker.io/library/opensuse:latest
9 | Gentoo | Created 2 minutes ago | docker.io/library/gentoo:latest
10 | Slackware | Created 2 minutes ago | docker.io/library/slackware:latest
11 | Void Linux | Created 2 minutes ago | docker.io/library/voidlinux:latest
13 | Deepin | Created 2 minutes ago | docker.io/library/deepin:latest
16 | Rocky Linux | Created 2 minutes ago | docker.io/library/rockylinux:latest
17 | Crystal Linux | Created 2 minutes ago | docker.io/library/crystal-linux:latest\n"
        );
    }
}
