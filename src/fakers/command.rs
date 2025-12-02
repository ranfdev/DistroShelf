use std::{
    ffi::{OsStr, OsString},
    fmt::Display,
    process::Stdio,
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

    // removes the first occurrence of an arg by name and its value
    pub fn remove_flag_value_arg(&mut self, name: &str) -> &mut Command {
        self.args.iter().position(|x| x == name).map(|index| {
            self.args.remove(index);
            self.args.remove(index); // same index, as the vector has shifted
        });
        self
    }
    pub fn remove_flag_arg(&mut self, name: &str) -> &mut Command {
        if let Some(index) = self.args.iter().position(|x| x == name) {
            self.args.remove(index);
        }
        self
    }

    pub fn to_vec(&self) -> Vec<OsString> {
        let mut v = Vec::with_capacity(1 + self.args.len());
        v.push(self.program.clone());
        v.extend(self.args.iter().cloned());
        v
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_new() {
        let cmd = Command::new("echo");
        assert_eq!(cmd.program, "echo");
        assert!(cmd.args.is_empty());
    }

    #[test]
    fn test_command_new_with_args() {
        let cmd = Command::new_with_args("ls", ["-la", "/tmp"]);
        assert_eq!(cmd.program, "ls");
        assert_eq!(cmd.args.len(), 2);
        assert_eq!(cmd.args[0], "-la");
        assert_eq!(cmd.args[1], "/tmp");
    }

    #[test]
    fn test_command_arg() {
        let mut cmd = Command::new("echo");
        cmd.arg("hello");
        cmd.arg("world");
        assert_eq!(cmd.args.len(), 2);
        assert_eq!(cmd.args[0], "hello");
        assert_eq!(cmd.args[1], "world");
    }

    #[test]
    fn test_command_args() {
        let mut cmd = Command::new("ls");
        cmd.args(["-l", "-a", "/home"]);
        assert_eq!(cmd.args.len(), 3);
    }

    #[test]
    fn test_command_extend() {
        let mut cmd1 = Command::new("sh");
        cmd1.arg("-c");

        let mut cmd2 = Command::new("echo");
        cmd2.arg("hello");

        cmd1.extend("&&", &cmd2);

        assert_eq!(cmd1.program, "sh");
        assert_eq!(cmd1.args.len(), 4);
        assert_eq!(cmd1.args[0], "-c");
        assert_eq!(cmd1.args[1], "&&");
        assert_eq!(cmd1.args[2], "echo");
        assert_eq!(cmd1.args[3], "hello");
    }

    #[test]
    fn test_command_remove_flag_arg() {
        let mut cmd = Command::new("ls");
        cmd.args(["-l", "-a", "--color"]);
        cmd.remove_flag_arg("-a");

        assert_eq!(cmd.args.len(), 2);
        assert_eq!(cmd.args[0], "-l");
        assert_eq!(cmd.args[1], "--color");
    }

    #[test]
    fn test_command_remove_flag_arg_not_found() {
        let mut cmd = Command::new("ls");
        cmd.args(["-l", "-a"]);
        cmd.remove_flag_arg("-z");

        // No change when flag not found
        assert_eq!(cmd.args.len(), 2);
    }

    #[test]
    fn test_command_remove_flag_value_arg() {
        let mut cmd = Command::new("git");
        cmd.args(["commit", "-m", "message", "--author", "user"]);
        cmd.remove_flag_value_arg("-m");

        assert_eq!(cmd.args.len(), 3);
        assert_eq!(cmd.args[0], "commit");
        assert_eq!(cmd.args[1], "--author");
        assert_eq!(cmd.args[2], "user");
    }

    #[test]
    fn test_command_to_vec() {
        let mut cmd = Command::new("echo");
        cmd.args(["hello", "world"]);

        let vec = cmd.to_vec();
        assert_eq!(vec.len(), 3);
        assert_eq!(vec[0], "echo");
        assert_eq!(vec[1], "hello");
        assert_eq!(vec[2], "world");
    }

    #[test]
    fn test_command_display() {
        let mut cmd = Command::new("echo");
        cmd.args(["hello", "world"]);

        let display = format!("{}", cmd);
        assert_eq!(display, "echo hello world");
    }

    #[test]
    fn test_command_display_no_args() {
        let cmd = Command::new("ls");
        let display = format!("{}", cmd);
        assert_eq!(display, "ls");
    }

    #[test]
    fn test_fd_mode_default() {
        let cmd = Command::new("echo");
        // Default should be Inherit for all
        matches!(cmd.stdin, FdMode::Inherit);
        matches!(cmd.stdout, FdMode::Inherit);
        matches!(cmd.stderr, FdMode::Inherit);
    }
}
