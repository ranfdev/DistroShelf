use std::{
    cell::{LazyCell, RefCell}, collections::BTreeMap, io, path::{Path, PathBuf}, process::Output, rc::Rc, str::FromStr
};
use tracing::{debug, error, info, warn};

use async_trait::async_trait;
use super::*;


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

pub struct Toolbox {
    cmd_runner: Box<dyn CommandRunner>,
    output_tracker: OutputTracker<String>,
    is_in_flatpak: bool,
}

impl std::fmt::Debug for Toolbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Toolbox")
            .field("is_in_flatpak", &self.is_in_flatpak)
            .field("output_tracker", &self.output_tracker)
            .finish()
    }
}

fn tbcmd() -> Command {
    Command::new("toolbox")
}

#[derive(Clone)]
pub enum ToolboxCommandRunnerResponse {
    Version,
    NoVersion,
    List(Vec<ContainerInfo>),
    Compatibility(Vec<String>),
    ExportedApps(String, Vec<(String, String, String)>),
}

impl ToolboxCommandRunnerResponse {
    pub fn common_distros() -> LazyCell<Vec<ContainerInfo>> {
        LazyCell::new(|| {
            [
                ("1", "Fedora", "registry.fedoraproject.org/fedora-toolbox:latest"),
                ("2", "Ubuntu", "docker.io/library/ubuntu:latest"),
                ("3", "Debian", "docker.io/library/debian:latest"),
                ("4", "CentOS", "registry.centos.org/centos:latest"),
                ("5", "RHEL", "registry.access.redhat.com/ubi8/ubi:latest"),
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
            ("firefox.desktop".into(), "Firefox".into(), "firefox".into()),
            ("gedit.desktop".into(), "Text Editor".into(), "gedit".into()),
            (
                "gnome-terminal.desktop".into(),
                "Terminal".into(),
                "gnome-terminal".into(),
            ),
        ];
        ToolboxCommandRunnerResponse::ExportedApps("Fedora".into(), dummy_exported_apps)
    }

    pub fn new_common_images() -> Self {
        ToolboxCommandRunnerResponse::Compatibility(
            Self::common_distros()
                .iter()
                .map(|x| x.image.clone())
                .collect(),
        )
    }

    fn build_version_response() -> (Command, String) {
        let mut cmd = Command::new("toolbox");
        cmd.arg("--version");
        (cmd, "toolbox version 0.0.99.3".to_string())
    }

    fn build_no_version_response() -> (Command, Rc<dyn Fn() -> io::Result<String>>) {
        let mut cmd = Command::new("toolbox");
        cmd.arg("--version");
        (cmd, Rc::new(|| Err(io::Error::from_raw_os_error(0))))
    }

    fn build_list_response(containers: &[ContainerInfo]) -> (Command, String) {
        let mut output = String::new();
        output.push_str("CONTAINER ID  CONTAINER NAME     CREATED         STATUS   IMAGE NAME\n");
        for container in containers {
            output.push_str(&container.id);
            output.push_str("  ");
            output.push_str(&container.name);
            output.push_str("  ");
            // Status in toolbox output doesn't directly map to our Status enum format
            // so we'll convert it to a more appropriate format
            let status_text = match &container.status {
                Status::Created(time) => format!("{}  created", time),
                Status::Exited(time) => format!("{}  exited", time),
                Status::Up(time) => format!("{}  running", time),
                Status::Other(s) => s.clone(),
            };
            output.push_str(&status_text);
            output.push_str("  ");
            output.push_str(&container.image);
            output.push('\n');
        }
        let mut cmd = Command::new("toolbox");
        cmd.arg("list").arg("-c");
        (cmd, output.clone())
    }

    fn build_compatibility_response(images: &[String]) -> (Command, String) {
        let output = images.join("\n");
        let mut cmd = Command::new("toolbox");
        cmd.arg("create").arg("--image");
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
            .map(|(filename, _, _)| format!("{}-{}", box_name, filename))
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
            Command::new_with_args("toolbox", 
                ["run", "-c", box_name, "sh", "-c", "for file in $(grep --files-without-match \"NoDisplay=true\" /usr/share/applications/*.desktop); do echo \"# START FILE $file\"; cat \"$file\"; done"]),
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

impl Toolbox {
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
        responses: &[ToolboxCommandRunnerResponse],
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
        let mut cmd = tbcmd();
        cmd.args([
            "run",
            "-c",
            box_name,
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

}

#[async_trait(?Send)]
impl ContainerCli for Toolbox {
    async fn list_apps(&self, box_name: &str) -> Result<Vec<ExportableApp>, Error> {
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

    fn launch_app(
        &self,
        container: &str,
        app: &ExportableApp,
    ) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = tbcmd();
        cmd.arg("run").arg("-c").arg(container).arg("--");
        let to_be_replaced = [" %f", " %u", " %F", " %U"];
        let cleaned_exec = to_be_replaced
            .into_iter()
            .fold(app.entry.exec.clone(), |acc, x| acc.replace(x, ""));
        cmd.arg(cleaned_exec);
        self.cmd_spawn(cmd)
    }

    async fn export_app(
        &self,
        container: &str,
        desktop_file_path: &str,
    ) -> Result<String, Error> {
        // Toolbox doesn't have a built-in export feature like distrobox-export
        // Instead, we'd need to manually copy the desktop file and modify it
        // For now, we'll just implement a basic simulation
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &format!("cp -f $(toolbox run -c {} -- sh -c 'echo {}') $HOME/.local/share/applications/{}-{} && echo 'App exported successfully'", 
            container, 
            desktop_file_path,
            container,
            Path::new(desktop_file_path).file_name().unwrap_or_default().to_string_lossy()
        )]);
        
        self.cmd_output_string(cmd).await
    }
    
    async fn unexport_app(
        &self,
        container: &str,
        desktop_file_path: &str,
    ) -> Result<String, Error> {
        // Simulate removing the exported desktop file
        let file_name = Path::new(desktop_file_path)
            .file_name()
            .map(|x| x.to_string_lossy().to_string())
            .unwrap_or_default();
        
        let mut cmd = Command::new("sh");
        cmd.args(["-c", &format!("rm -f $HOME/.local/share/applications/{}-{} && echo 'App unexported successfully'", 
            container, 
            file_name
        )]);
        
        self.cmd_output_string(cmd).await
    }

    // Toolbox doesn't have an assemble feature, so we'll just return an error
    fn assemble(&self, _file_path: &str) -> Result<Box<dyn Child + Send>, Error> {
        Err(Error::ParseOutput("Toolbox does not support assemble command".into()))
    }

    fn assemble_from_url(&self, _url: &str) -> Result<Box<dyn Child + Send>, Error> {
        Err(Error::ParseOutput("Toolbox does not support assemble command".into()))
    }
    
    async fn create(&self, args: CreateArgs) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = tbcmd();
        cmd.arg("create");
        
        if !args.name.0.is_empty() {
            cmd.arg("-c").arg(args.name.0);
        }
        
        if !args.image.is_empty() {
            cmd.arg("-i").arg(args.image);
        }
        
        // Toolbox doesn't support the following flags directly, but some could be added as different options
        // if args.init { ... }
        // if args.nvidia { ... }
        
        if let Some(home_path) = args.home_path {
            cmd.arg("--home").arg(home_path);
        }
        
        for volume in args.volumes {
            cmd.arg("-v").arg(volume);
        }
        
        self.cmd_spawn(cmd)
    }
    
    async fn list_images(&self) -> Result<Vec<String>, Error> {
        // Toolbox doesn't have a direct way to list available images, but we can list local ones
        let mut cmd = Command::new("podman");
        cmd.args(["images", "--format", "{{.Repository}}:{{.Tag}}"]);
        
        let text = self.cmd_output_string(cmd).await?;
        let lines = text
            .lines()
            .filter_map(|x| {
                if !x.is_empty() && x.contains("toolbox") {
                    Some(x.to_string())
                } else {
                    None
                }
            })
            .collect();
        Ok(lines)
    }
    
    fn enter_cmd(&self, name: &str) -> Command {
        let mut cmd = tbcmd();
        cmd.arg("enter").arg("-c").arg(name);
        cmd
    }
    
    async fn clone_to(
        &self,
        source_name: &str,
        target_name: &str,
    ) -> Result<Box<dyn Child + Send>, Error> {
        // Toolbox doesn't have a direct clone command, we need to use podman
        let mut cmd = Command::new("podman");
        cmd.args([
            "container", 
            "clone", 
            &format!("toolbox-{}", source_name), 
            &format!("toolbox-{}", target_name)
        ]);
        
        self.cmd_spawn(cmd)
    }
    
    async fn list(&self) -> Result<BTreeMap<String, ContainerInfo>, Error> {
        let mut cmd = tbcmd();
        cmd.args(["list", "-c"]);
        
        let text = self.cmd_output_string(cmd).await?;
        let lines = text.lines().skip(1); // Skip header
        let mut out = BTreeMap::new();
        
        for line in lines {
            // Parse the space-separated output from toolbox
            // Format: CONTAINER ID  CONTAINER NAME  CREATED  STATUS  IMAGE NAME
            let parts: Vec<_> = line.split("  ").collect();


            let id = parts[0].trim().to_string();
            let name = parts[1].trim().to_string();
            let status_str = parts[3].trim().to_string();
            let image = parts[4].trim() .to_string();
            
            // Map the status string to our Status enum
            let status = if status_str.contains("running") {
                Status::Up(status_str.replace("running", "").trim().to_string())
            } else if status_str.contains("created") {
                Status::Created(status_str.replace("created", "").trim().to_string())
            } else if status_str.contains("exited") {
                Status::Exited(status_str.replace("exited", "").trim().to_string())
            } else {
                Status::Other(status_str.clone())
            };
            
            // Check for empty fields
            if id.is_empty() {
                return Err(Error::ParseOutput(format!("Container ID missing in line: {}", line)));
            }
            if name.is_empty() {
                return Err(Error::ParseOutput(format!("Container name missing in line: {}", line)));
            }
            if status_str.is_empty() {
                return Err(Error::ParseOutput(format!("Status missing in line: {}", line)));
            }
            if image.is_empty() {
                return Err(Error::ParseOutput(format!("Image missing in line: {}", line)));
            }
            
            let container_info = ContainerInfo {
                id,
                name: name.to_string(),
                status,
                image,
            };
            
            debug!(
                container_id = %container_info.id,
                container_name = %container_info.name,
                image = %container_info.image,
                status = ?container_info.status,
                "Discovered container"
            );
            out.insert(container_info.name.clone(), container_info);
        }
        Ok(out)
    }
    
    async fn remove(&self, name: &str) -> Result<String, Error> {
        let mut cmd = tbcmd();
        cmd.args(["rm", "-f", "-c", name]);
        self.cmd_output_string(cmd).await
    }
    
    async fn stop(&self, name: &str) -> Result<String, Error> {
        // Toolbox doesn't have a direct stop command, we need to use podman
        let mut cmd = Command::new("podman");
        cmd.args(["stop", &format!("toolbox-{}", name)]);
        self.cmd_output_string(cmd).await
    }
    
    async fn stop_all(&self) -> Result<String, Error> {
        // Get all toolbox containers and stop them
        let mut cmd = Command::new("sh");
        cmd.args([
            "-c", 
            "podman ps -a --format '{{.Names}}' | grep '^toolbox-' | xargs -r podman stop"
        ]);
        self.cmd_output_string(cmd).await
    }
    
    fn upgrade(&self, name: &str) -> Result<Box<dyn Child + Send>, Error> {
        let mut cmd = tbcmd();
        cmd.args(["run", "-c", name, "--", "dnf", "upgrade", "-y"]);
        self.cmd_spawn(cmd)
    }
    
    async fn upgrade_all(&mut self) -> Result<String, Error> {
        // Get all toolbox containers and upgrade them
        let mut cmd = Command::new("sh");
        cmd.args([
            "-c", 
            "for container in $(toolbox list -c | tail -n +2 | awk '{print $2}'); do toolbox run -c $container -- dnf upgrade -y; done && echo 'All containers upgraded'"
        ]);
        self.cmd_output_string(cmd).await
    }
    
    async fn version(&self) -> Result<String, Error> {
        let mut cmd = tbcmd();
        cmd.arg("--version");
        let text = self.cmd_output_string(cmd).await?;
        
        // Expected format: "toolbox version X.Y.Z.W"
        if let Some(version) = text.strip_prefix("toolbox version ") {
            let version = version.trim().to_string();
            info!(
                toolbox_version = %version,
                raw_output = %text,
                "Successfully parsed toolbox version"
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
}

impl Default for Toolbox {
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
            let output = "CONTAINER ID  CONTAINER NAME     CREATED         STATUS   IMAGE NAME
9258381fccce  fedora-toolbox-42  53 minutes ago  running  registry.fedoraproject.org/fedora-toolbox:42";
            let tb = Toolbox::new_null(
                NullCommandRunnerBuilder::new()
                    .cmd(&["toolbox", "list", "-c"], output)
                    .build(),
                false,
            );
            let result = tb.list().await?;
            assert_eq!(
                result.get("fedora-toolbox-42").map(|c| c.id.clone()),
                Some("9258381fccce".to_string())
            );
            assert_eq!(
                result.get("fedora-toolbox-42").map(|c| c.status.to_string()),
                Some("Up ".to_string())
            );
            assert_eq!(
                result.get("fedora-toolbox-42").map(|c| c.image.clone()),
                Some("registry.fedoraproject.org/fedora-toolbox:42".to_string())
            );
            Ok(())
        })
    }

    #[test]
    fn version() -> Result<(), Error> {
        block_on(async {
            let output = "toolbox version 0.0.99.3";
            let tb = Toolbox::new_null(
                NullCommandRunnerBuilder::new()
                    .cmd(&["toolbox", "--version"], output)
                    .build(),
                false,
            );
            assert_eq!(tb.version().await?, "0.0.99.3".to_string(),);
            Ok(())
        })
    }

    #[test]
    fn list_apps() -> Result<(), Error> {
        let tb = Toolbox::new_null(NullCommandRunnerBuilder::new()
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
                "fedora-firefox.desktop\n"
            )
            .cmd(
                &[
                    "toolbox",
                    "run",
                    "-c",
                    "fedora",
                    "sh",
                    "-c",
                    "for file in $(grep --files-without-match \"NoDisplay=true\" /usr/share/applications/*.desktop); do echo \"# START FILE $file\"; cat \"$file\"; done",
                ],
                "# START FILE /usr/share/applications/firefox.desktop
[Desktop Entry]
Type=Application
Name=Firefox
Exec=/usr/bin/firefox
Icon=firefox
Comment=Web Browser
Categories=Network;WebBrowser;
# START FILE /usr/share/applications/gedit.desktop
[Desktop Entry]
Type=Application
Name=Text Editor
Exec=/usr/bin/gedit
Icon=gedit
Comment=Edit text files
Categories=Utility;TextEditor;
",)
            .build(),
            false
        );

        let apps = block_on(tb.list_apps("fedora"))?;
        assert_eq!(&apps[0].entry.name, "Firefox");
        assert_eq!(&apps[0].entry.exec, "/usr/bin/firefox");
        assert!(apps[0].exported);
        assert_eq!(&apps[1].entry.name, "Text Editor");
        assert_eq!(&apps[1].entry.exec, "/usr/bin/gedit");
        assert!(!apps[1].exported);
        Ok(())
    }
    
    #[test]
    fn create() -> Result<(), Error> {
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();
        let tb = Toolbox::new_null(NullCommandRunner::default(), false);
        let output_tracker = tb.output_tracker();
        debug!("Testing container creation");
        let args = CreateArgs {
            image: "registry.fedoraproject.org/fedora-toolbox:latest".into(),
            init: true,  // Not used by toolbox but included in args
            nvidia: true, // Not used by toolbox but included in args
            home_path: Some("/home/me".into()),
            volumes: vec!["/mnt/data".into(), "/mnt/projects".into()],
            ..Default::default()
        };

        block_on(tb.create(args))?;
        let expected = "\"toolbox\" [\"create\", \"--home\", \"/home/me\", \"-v\", \"/mnt/data\", \"-v\", \"/mnt/projects\"]";
        assert_eq!(output_tracker.items()[0], expected);
        Ok(())
    }
}