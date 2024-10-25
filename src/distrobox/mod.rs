use std::{
    cell::RefCell, collections::HashMap, io::{self, BufRead, BufReader, Cursor, Read, Write}, os::unix::process::ExitStatusExt, path::Path, process::{Command, ExitStatus, Output}, rc::Rc, str::FromStr
};

mod desktop_file;
mod command_runner;

use command_runner::*;
use desktop_file::*;

#[derive(Default, Clone)]
struct OutputTracker<T> {
    store: Rc<RefCell<Option<Vec<T>>>>
}

impl<T: Clone> OutputTracker<T> {
    fn enable(&self) {
        let mut inner = self.store.borrow_mut();
        if inner.is_none() {
            *inner = Some(vec![]);
        }
    }
    fn push(&self, item: T) {
        if let Some(v) = &mut *self.store.borrow_mut() {
            v.push(item);
        }
    }
    fn items(&self) -> Vec<T> {
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


#[derive(Debug, PartialEq, Hash)]
pub struct ContainerInfo {
    pub id: String,
    pub name: String,
    pub status: String,
    pub image: String,
}

impl ContainerInfo {
    fn field_missing_error(text: &str) -> Error {
        Error::ParseOutput(format!("{text} missing"))
    }
}

#[derive(thiserror::Error, Debug)]
enum LsItemParseError {
    #[error("Invalid input format")]
    InvalidFormat,
    #[error("Empty {0} field")]
    EmptyField(String),
}

impl FromStr for ContainerInfo {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split('|').collect();
        if parts.len() != 4 {
            return Err(Error::ParseOutput("Invalid field count".into()));
        }

        let id = parts[0].trim();
        let name = parts[1].trim();
        let status = parts[2].trim();
        let image = parts[3].trim();

        // Check for empty fields
        if id.is_empty() {
            return Err(ContainerInfo::field_missing_error("id"));
        }
        if name.is_empty() {
            return Err(ContainerInfo::field_missing_error("name"));
        }
        if status.is_empty() {
            return Err(ContainerInfo::field_missing_error("status"));
        }
        if image.is_empty() {
            return Err(ContainerInfo::field_missing_error("image"));
        }

        Ok(ContainerInfo {
            id: id.to_string(),
            name: name.to_string(),
            status: status.to_string(),
            image: image.to_string(),
        })
    }
}


struct ExportableApp {
    entry: DesktopEntry,
    desktop_file_path: String,
    exported: bool,
}


#[derive(Default, Debug, PartialEq, Clone)]
struct CreateArgs<'a> {
    init: bool,
    nvidia: bool,
    home_path: &'a str,
    image: &'a str,
    name: &'a str,
    volumes: Vec<&'a str>
}

impl<'a> CreateArgs<'a> {
    fn init(&mut self) -> &mut Self {
        self.init = true;
        self
    }
    fn nvidia(&mut self) -> &mut Self {
        self.nvidia = true;
        self
    }
    fn home_path(&mut self, path: &'a str) -> &mut Self {
        self.home_path = path;
        self
    }
    fn volume(&mut self, volume: &'a str) -> &mut Self {
        self.volumes.push(volume);
        self
    }
    fn image(&mut self, image: &'a str) -> &mut Self {
        self.image = image;
        self
    }
    fn name(&mut self, name: &'a str) -> &mut Self {
        self.name = name;
        self
    }
    fn get_init(&self) -> bool {
        self.init
    }
    fn get_nvidia(&self) -> bool {
        self.nvidia
    }
    fn get_home_path(&self) -> &'a str {
        self.home_path
    }
    fn get_volumes(&self) -> Vec<&'a str> {
        self.volumes.clone()
    }
    fn get_image(&self) -> &'a str {
        self.image
    }
    fn get_name(&self) -> &'a str {
        self.name
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to read command stdout: {0}")]
    StdoutRead(#[from] io::Error),

    #[error("failed to spawn command: {0}")]
    Spawn(io::Error),

    #[error("failed to parse command output: {0}")]
    ParseOutput(String)
}

fn dbcmd() -> Command {
    Command::new("distrobox")
}
impl Distrobox {
    pub fn new() -> Self {
        Self {
            cmd_runner: Box::new(RealCommandRunner{}),
            is_in_flatpak: Self::get_is_in_flatpak(),
            output_tracker: Default::default()
        }
    }
    fn new_null(runner: NullCommandRunner, is_in_flatpak: bool) -> Self {
        Self {
            cmd_runner: Box::new(runner),
            output_tracker: OutputTracker::default(),
            is_in_flatpak
        }
    }
    fn output_tracker(&self) -> OutputTracker<String> {
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

    fn cmd_output(&self, cmd: &mut Command) -> Result<Output, Error> {
        let cmd = if self.is_in_flatpak {
            &mut Self::wrap_flatpak_cmd(cmd)
        } else {
            cmd
        };
        let child = self.cmd_runner.output(cmd).map_err(Error::Spawn)?;

        self.output_tracker.push(format!("{:?} {:?}", cmd.get_program(), cmd.get_args().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>()));
        Ok(child)
    }
    
    fn cmd_output_string(&self, cmd: &mut Command) -> Result<String, Error> {
        let output = self.cmd_output(cmd)?;
        let s = String::from_utf8_lossy(&output.stdout);
        Ok(s.to_string())
    }

    fn wrap_flatpak_cmd(prev: &Command) -> Command {
        let mut cmd = Command::new("flatpak-spawn");
        cmd.arg("--host")
            .arg(prev.get_program())
            .args(prev.get_args());
        cmd
    }
     
    fn get_exported_desktop_files(&self, box_name: &str) -> Result<Vec<String>, Error> {
        // We do everything with the command line to ensure we can access the files and environment variables
        // even when inside a flatpak sandbox, with only the permissions to run `flatpak-spawn`
        let xdg_data_home = self.cmd_output_string(Command::new("sh").args(["-c", "echo $XDG_DATA_HOME"]))?;

        let xdg_data_home = if xdg_data_home.is_empty() {
            let home = self.cmd_output_string(Command::new("sh").args(["-c", "echo $HOME"]))?;
            Path::new(home.trim()).join(".local/share")
        } else {
            Path::new(xdg_data_home.trim()).to_path_buf()
        };
        let apps_path = xdg_data_home.join("applications");
        let ls_out = self.cmd_output_string(Command::new("ls")
            .arg(apps_path))?;
        let apps = ls_out.trim().split("\n").map(|app| app.to_string()).collect::<Vec<_>>();
        Ok(apps)
    }
    fn get_desktop_file_paths(&self, box_name: &str) -> Result<String, Error> {
        self.cmd_output_string(dbcmd().args([
            "enter",
            box_name,
            "--",
            "bash",
            "-c",
            "grep --files-without-match \"NoDisplay=true\" /usr/share/applications/*.desktop",
        ]))
    }
    fn get_desktop_file(&self, box_name: &str, path: &str) -> Result<String, Error> {
        self.cmd_output_string(dbcmd().args(["enter", box_name, "--", "cat", path]))
    }

    pub fn list_apps(&mut self, box_name: &str) -> Result<Vec<ExportableApp>, Error> {
        let exported = self.get_exported_desktop_files(box_name)?;
        let desktop_file_paths = self.get_desktop_file_paths(box_name)?;
        let res: Result<Vec<Option<ExportableApp>>, _> = desktop_file_paths
            .split('\n')
            .map(|path| -> Result<Option<ExportableApp>, Error> {
                let content = self.get_desktop_file(box_name, path)?;
                let entry = parse_desktop_file(&content);
                let file_name = Path::new(path).file_name().map(|x| x.to_str()).unwrap_or_default().unwrap_or_default();
                let exported_as = format!("{box_name}-{file_name}");
                Ok(entry.map(|entry| ExportableApp {
                    desktop_file_path: path.to_string(),
                    entry,
                    exported: exported.contains(&exported_as),
                }))
            })
            .collect();
            
        Ok(res?.into_iter().flatten().collect())
    }
    // assemble
    pub fn assemble(&mut self) -> Result<(), Error> {
        !unimplemented!()
    }
    // create
    pub fn create(&mut self, args: CreateArgs) -> Result<String, Error> {
        let mut cmd = dbcmd();
        cmd.arg("--yes");
        if !args.get_image().is_empty() {
            cmd.arg("--image").arg(args.get_image());
        }
        if !args.get_name().is_empty() {
            cmd.arg("--name").arg(args.get_name());
        }
        if args.get_init() {
            cmd.arg("--init");
        }
        if args.get_nvidia() {
            cmd.arg("--nvidia");
        }
        if !args.get_home_path().is_empty() {
            cmd.arg("--home").arg(args.get_home_path());
        }
        for volume in args.get_volumes() {
            cmd.arg("--volume").arg(volume);
        }
        self.cmd_output_string(&mut cmd)
    }
    // enter
    // list | ls
    pub fn list(&self) -> Result<Vec<ContainerInfo>, Error> {
        let text = self.cmd_output_string(dbcmd().arg("ls"))?;
        let lines = text.lines().skip(1);
        let mut out = vec![];
        for line in lines {
            let item: ContainerInfo = line.parse()?;
            out.push(item);
        }
        Ok(out)
    }
    // rm
    pub fn remove(&mut self, name: &str) -> Result<String, Error> {
        self.cmd_output_string(dbcmd().arg("rm").arg("--force").arg(name))
    }
    // stop
    pub fn stop(&mut self, name: &str) -> Result<String, Error> {
        self.cmd_output_string(dbcmd().arg("stop").arg("--yes").arg(name))
    }
    pub fn stop_all(&mut self, name: &str) -> Result<String, Error> {
        self.cmd_output_string(dbcmd().arg("stop").arg("--all").arg("--yes"))
    }
    // upgrade
    pub fn upgrade(&mut self, name: &str) -> Result<String, Error> {
        self.cmd_output_string(dbcmd().arg("upgrade").arg(name))
    }
    pub fn upgrade_all(&mut self, name: &str) -> Result<String, Error> {
        self.cmd_output_string(dbcmd().arg("upgrade").arg("--all"))
    }
    // ephemeral
    // generate-entry
    // version
    pub fn version(&mut self) -> Result<String, Error> {
        let text = self.cmd_output_string(dbcmd().arg("version"))?;
        let mut parts = text.split(':');
        if let Some(v) = parts.nth(1) {
            Ok(v.trim().to_string())
        } else {
            Err(Error::ParseOutput(
                "parsing version, trying to find ':'".to_string(),
            ))
        }
    }
    // help
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list() -> Result<(), Error> {
        let output = "ID           | NAME                 | STATUS             | IMAGE                         
d24405b14180 | ubuntu               | Created            | ghcr.io/ublue-os/ubuntu-toolbox:latest";
        let mut db = Distrobox::new_null(
            NullCommandRunnerBuilder::new().cmd(&["distrobox", "ls"], output).build(), false
            );
        assert_eq!(
            db.list()?,
            vec![ContainerInfo {
                id: "d24405b14180".into(),
                name: "ubuntu".into(),
                status: "Created".into(),
                image: "ghcr.io/ublue-os/ubuntu-toolbox:latest".into(),
            }]
        );
        Ok(())
    }

    #[test]
    fn version() -> Result<(), Error> {
        let output = "distrobox: 1.7.2.1";
        let mut db = Distrobox::new_null(NullCommandRunnerBuilder::new().cmd(&["distrobox", "version"], output).build(), false);
        assert_eq!(db.version()?, "1.7.2.1".to_string(),);
        Ok(())
    }

    #[test]
    fn list_apps() -> Result<(), Error> {
        let mut db = Distrobox::new_null(NullCommandRunnerBuilder::new()
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
                "ubuntu-vim.desktop"
            )
            .cmd(&[
                    "distrobox",
                    "enter",
                    "ubuntu",
                    "--",
                    "bash",
                    "-c",
                    "grep --files-without-match \"NoDisplay=true\" /usr/share/applications/*.desktop",
                ],
            "/usr/share/applications/vim.desktop
/usr/share/applications/fish.desktop",)
            .cmd(&[
                    "distrobox",
                    "enter",
                    "ubuntu",
                    "--",
                    "cat",
                    "/usr/share/applications/vim.desktop",
                ], 
            "[Desktop Entry]
Type=Application
Name=Vim
Exec=/path/to/vim
Icon=/path/to/icon.png
Comment=A brief description of my application
Categories=Utility;Network;")
            .cmd(&[
                    "distrobox",
                    "enter",
                    "ubuntu",
                    "--",
                    "cat",
                    "/usr/share/applications/fish.desktop",
                ], 
            "[Desktop Entry]
Type=Application
Name=Fish
Exec=/path/to/fish
Icon=/path/to/icon.png
Comment=A brief description of my application
Categories=Utility;Network;")
            .build(),
            false
        );
        // let output_tracker = db.output_tracker();
        let apps = db.list_apps("ubuntu")?;
        // dbg!(output_tracker.items());
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
        let mut db = Distrobox::new_null(NullCommandRunner::default(), false);
        let output_tracker = db.output_tracker();
        let mut args = CreateArgs::default();
        args.image("docker.io/library/ubuntu:latest")
            .init()
            .nvidia()
            .volume("/mnt/sdb1")
            .volume("/mnt/sdb4")
            .home_path("/home/me");
        db.create(args)?;
        let expected = "\"distrobox\" [\"--yes\", \"--image\", \"docker.io/library/ubuntu:latest\", \"--init\", \"--nvidia\", \"--home\", \"/home/me\", \"--volume\", \"/mnt/sdb1\", \"--volume\", \"/mnt/sdb4\"]";
        assert_eq!(output_tracker.items()[0], expected);
        Ok(())
    }
    #[test]
    fn remove() -> Result<(), Error> {
        let mut db = Distrobox::new_null(NullCommandRunner::default(), false);
        let output_tracker = db.output_tracker();
        db.remove("ubuntu")?;
        assert_eq!(output_tracker.items()[0], "\"distrobox\" [\"rm\", \"--force\", \"ubuntu\"]");
        Ok(())
    }
}
