use std::{
    ffi::{OsStr, OsString}, fmt::Display, process::Stdio
};

#[derive(Debug, Clone)]
pub enum FdMode {
    Inherit,
    Pipe,
}

impl From<FdMode> for Stdio {
    fn from(val: FdMode) -> Stdio {
        match val {
            FdMode::Inherit => Stdio::inherit(),
            FdMode::Pipe => Stdio::piped(),
        }
    }
}

// The standard library's `std::process::Command` isn't clonable and has some private parameters,
// like the stdout mode (inherited/piped).
// This `Command` struct fully owns its parameters, so that it can be cloned,
// passed around and transformed as a proper "Value type".
#[derive(Clone, Debug)]
pub struct Command {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub stdout: FdMode,
    pub stderr: FdMode,
    pub stdin: FdMode,
}

impl Command {
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: Vec::new(),
            stdin: FdMode::Inherit,
            stdout: FdMode::Inherit,
            stderr: FdMode::Inherit,
        }
    }

    pub fn new_with_args(
        program: impl AsRef<OsStr>,
        args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    ) -> Self {
        Self {
            program: program.as_ref().to_owned(),
            args: args
                .into_iter()
                .map(|arg| arg.as_ref().to_owned())
                .collect(),
            stdin: FdMode::Inherit,
            stdout: FdMode::Inherit,
            stderr: FdMode::Inherit,
        }
    }

    pub fn extend(&mut self, separator: impl AsRef<OsStr>, other: &Command) -> &mut Command {
        self.arg(separator);
        self.arg(other.program.clone());
        self.args(other.args.clone());
        self
    }

    // backward compatibility with some `std::process::Command` methods

    // appends multiple args
    pub fn args<I, S>(&mut self, args: I) -> &mut Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|x| x.as_ref().to_owned()));
        self
    }

    // appends one arg
    pub fn arg<S>(&mut self, arg: S) -> &mut Command
    where
        S: AsRef<OsStr>,
    {
        self.args.push(arg.as_ref().to_owned());
        self
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.program.to_string_lossy())?;
        for arg in &self.args {
            write!(f, " {}", arg.to_string_lossy())?;
        }
        Ok(())
    }
}

impl From<Command> for async_process::Command {
    fn from(val: Command) -> Self {
        let mut cmd = async_process::Command::new(val.program);
        cmd.args(val.args)
            .stdin::<Stdio>(val.stdin.into())
            .stdout::<Stdio>(val.stdout.into())
            .stderr::<Stdio>(val.stderr.into());
        cmd
    }
}

pub fn wrap_flatpak_cmd(mut prev: Command) -> Command {
    let mut args = vec!["--host".into(), prev.program];
    args.extend(prev.args);

    prev.args = args;
    prev.program = "flatpak-spawn".into();
    prev
}

pub fn wrap_capture_cmd(cmd: &mut Command) -> &mut Command {
    cmd.stdout = FdMode::Pipe;
    cmd.stderr = FdMode::Pipe;
    cmd
}
