// thin wrapper to provide a `spawn` function and a configurable `NullCommandRunner`
// returning predefined outputs, to ease code testing.

use std::{
    collections::HashMap,
    future::Future,
    io::{self},
    os::unix::process::ExitStatusExt,
    pin::Pin,
    process::ExitStatus,
    rc::Rc,
    sync::Arc,
};

use crate::fakers::{Command, FdMode, OutputTracker};

use async_process::{Command as AsyncCommand, Output};
use futures::{
    FutureExt,
    io::{AsyncRead, AsyncWrite, Cursor},
};

#[derive(Debug, Clone)]
pub enum CommandRunnerEvent {
    Spawned(usize, Command),
    /// Started command that will return the output
    Started(usize, Command),
    Output(usize, Result<(), ()>),
}

impl CommandRunnerEvent {
    pub fn event_id(&self) -> usize {
        match self {
            CommandRunnerEvent::Spawned(id, _) => *id,
            CommandRunnerEvent::Started(id, _) => *id,
            CommandRunnerEvent::Output(id, _) => *id,
        }
    }
    pub fn command(&self) -> Option<&Command> {
        match self {
            CommandRunnerEvent::Spawned(_, cmd) => Some(cmd),
            CommandRunnerEvent::Started(_, cmd) => Some(cmd),
            CommandRunnerEvent::Output(_, _) => None,
        }
    }
}

#[derive(Clone)]
pub struct CommandRunner {
    pub inner: Rc<dyn InnerCommandRunner>,
    pub output_tracker: OutputTracker<CommandRunnerEvent>,
}

impl CommandRunner {
    pub fn new(inner: Rc<dyn InnerCommandRunner>) -> Self {
        CommandRunner {
            inner,
            output_tracker: OutputTracker::new(),
        }
    }
    pub fn new_null() -> Self {
        CommandRunner::new(Rc::new(NullCommandRunner::default()))
    }
    pub fn new_real() -> Self {
        CommandRunner::new(Rc::new(RealCommandRunner {}))
    }

    pub fn map_cmd(&self, f: impl Fn(Command) -> Command + 'static) -> CommandRunner {
        let mapped_inner = Rc::new(Map {
            inner: self.inner.clone(),
            map_cmd: Rc::new(f),
        });
        CommandRunner {
            inner: mapped_inner,
            output_tracker: self.output_tracker.clone(),
        }
    }

    pub fn output_tracker(&self) -> OutputTracker<CommandRunnerEvent> {
        self.output_tracker.enable();
        self.output_tracker.clone()
    }

    fn event_id(&self) -> usize {
        self.output_tracker.len()
    }

    pub fn wrap_command(&self, command: Command) -> Command {
        self.inner.wrap_command(command)
    }

    pub fn spawn(&self, command: Command) -> io::Result<Box<dyn Child + Send>> {
        self.output_tracker.push(CommandRunnerEvent::Spawned(
            self.event_id(),
            command.clone(),
        ));
        self.inner.spawn(command)
    }

    pub fn output(
        &self,
        mut command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<std::process::Output>>>> {
        command.stdout = FdMode::Pipe;
        command.stderr = FdMode::Pipe;

        let event_id = self.event_id();
        self.output_tracker
            .push(CommandRunnerEvent::Started(event_id, command.clone()));
        let fut = self.inner.output(command);
        let this = self.clone();
        fut.map(move |result| {
            let res_summary = match &result {
                Ok(_output) => Ok(()),
                Err(_e) => Err(()),
            };
            this.output_tracker
                .push(CommandRunnerEvent::Output(event_id, res_summary));
            result
        })
        .boxed_local()
    }
    pub fn output_string(
        &self,
        mut command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<String>>>> {
        let this = self.clone();
        async move {
            command.stdout = FdMode::Pipe;
            command.stderr = FdMode::Pipe;
            let output = this.output(command).await?;
            let s = String::from_utf8_lossy(&output.stdout).to_string();
            Ok(s)
        }
        .boxed_local()
    }
}

impl Default for CommandRunner {
    fn default() -> Self {
        CommandRunner::new_null()
    }
}

pub trait InnerCommandRunner {
    fn wrap_command(&self, command: Command) -> Command {
        command
    }
    fn spawn(&self, command: Command) -> io::Result<Box<dyn Child + Send>>;
    fn output(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<std::process::Output>>>>;
}

#[derive(Clone, Debug)]
pub struct RealCommandRunner {}
impl RealCommandRunner {
    #[allow(dead_code)]
    pub fn new() -> Self {
        RealCommandRunner {}
    }
}

impl InnerCommandRunner for RealCommandRunner {
    fn spawn(&self, command: Command) -> io::Result<Box<dyn Child + Send>> {
        let mut command: AsyncCommand = command.into();
        Ok(Box::new(command.spawn()?))
    }
    fn output(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<async_process::Output>>>> {
        let mut command: AsyncCommand = command.into();
        command.output().boxed()
    }
}

#[derive(Default, Clone)]
pub struct NullCommandRunnerBuilder {
    responses: HashMap<Vec<String>, Rc<dyn Fn() -> Result<String, io::Error>>>,
    #[allow(dead_code)]
    fallback_exit_status: ExitStatus,
}

impl NullCommandRunnerBuilder {
    pub fn new() -> Self {
        Default::default()
    }
    #[allow(dead_code)]
    pub fn cmd<T: AsRef<str>>(&mut self, args: &[T], out: T) -> &mut Self {
        let args: Vec<_> = args.iter().map(|x| x.as_ref()).collect();
        let mut cmd = Command::new(args[0]);
        cmd.args(&args[1..]);
        let out_text = out.as_ref().to_string();
        self.cmd_full(cmd, move || Ok(out_text.clone()))
    }
    pub fn cmd_full(
        &mut self,
        cmd: Command,
        out: impl Fn() -> Result<String, io::Error> + 'static,
    ) -> &mut Self {
        let key = NullCommandRunner::key_for_cmd(&cmd);
        self.responses.insert(key, Rc::new(out));
        self
    }
    #[allow(dead_code)]
    pub fn fallback(&mut self, status: ExitStatus) -> &mut Self {
        self.fallback_exit_status = status;
        self
    }
    pub fn build(&self) -> CommandRunner {
        let inner = Rc::new(NullCommandRunner {
            responses: self.responses.clone(),
            fallback_exit_status: self.fallback_exit_status,
        });
        CommandRunner::new(inner)
    }
}

#[derive(Default, Clone)]
pub struct NullCommandRunner {
    responses: HashMap<Vec<String>, Rc<dyn Fn() -> Result<String, io::Error>>>,
    #[allow(dead_code)]
    fallback_exit_status: ExitStatus,
}

impl NullCommandRunner {
    fn key_for_cmd(command: &Command) -> Vec<String> {
        let mut key: Vec<_> = command
            .args
            .iter()
            .map(|x| x.to_string_lossy().to_string())
            .collect();
        key.insert(0, command.program.to_string_lossy().to_string());
        key
    }
}

impl InnerCommandRunner for NullCommandRunner {
    fn spawn(&self, command: Command) -> io::Result<Box<dyn Child + Send>> {
        let key = Self::key_for_cmd(&command);
        let response = self
            .responses
            .get(&key[..])
            .cloned()
            .unwrap_or(Rc::new(|| Ok(String::new())));
        let stub = StubChild::new_null(vec![], Cursor::new(response()?), Cursor::new(""), || {
            Ok(ExitStatus::from_raw(0))
        });
        Ok(Box::new(stub))
    }
    fn output(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<std::process::Output>>>> {
        let key = Self::key_for_cmd(&command);
        let response = self
            .responses
            .get(&key[..])
            .cloned()
            .unwrap_or(Rc::new(|| Ok(String::new())));

        async move {
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: response()?.into(),
                stderr: vec![],
            })
        }
        .boxed_local()
    }
}

pub trait Child {
    fn take_stdin(&mut self) -> Option<Box<dyn AsyncWrite + Send + Unpin>>;
    fn take_stdout(&mut self) -> Option<Box<dyn AsyncRead + Send + Unpin>>;
    fn take_stderr(&mut self) -> Option<Box<dyn AsyncRead + Send + Unpin>>;
    fn kill(&mut self) -> Result<(), io::Error>;
    fn wait(&mut self) -> Pin<Box<dyn Future<Output = Result<ExitStatus, io::Error>>>>;
}

struct StubChild {
    stdin: Option<Box<dyn AsyncWrite + Send + Unpin>>,
    stdout: Option<Box<dyn AsyncRead + Send + Unpin>>,
    stderr: Option<Box<dyn AsyncRead + Send + Unpin>>,
    exit_status_fn: Arc<dyn Fn() -> io::Result<ExitStatus> + Send + Sync>,
}

impl StubChild {
    fn new_null(
        stdin: impl AsyncWrite + Send + Unpin + 'static,
        stdout: impl AsyncRead + Send + Unpin + 'static,
        stderr: impl AsyncRead + Send + Unpin + 'static,
        exit_status_fn: impl Fn() -> io::Result<ExitStatus> + Send + Sync + 'static,
    ) -> StubChild {
        StubChild {
            stdin: Some(Box::new(stdin)),
            stdout: Some(Box::new(stdout)),
            stderr: Some(Box::new(stderr)),
            exit_status_fn: Arc::new(exit_status_fn),
        }
    }
}
impl Child for StubChild {
    fn take_stdin(&mut self) -> Option<Box<dyn AsyncWrite + Send + Unpin>> {
        self.stdin.take()
    }
    fn take_stdout(&mut self) -> Option<Box<dyn AsyncRead + Send + Unpin>> {
        self.stdout.take()
    }
    fn take_stderr(&mut self) -> Option<Box<dyn AsyncRead + Send + Unpin>> {
        self.stderr.take()
    }

    fn kill(&mut self) -> Result<(), io::Error> {
        Ok(())
    }
    fn wait(&mut self) -> Pin<Box<dyn Future<Output = Result<ExitStatus, io::Error>>>> {
        let status = (self.exit_status_fn)();
        async move { status }.boxed_local()
    }
}

impl Child for async_process::Child {
    fn take_stdin(&mut self) -> Option<Box<dyn AsyncWrite + Send + Unpin>> {
        self.stdin
            .take()
            .map(|x| Box::new(x) as Box<dyn AsyncWrite + Send + Unpin>)
    }
    fn take_stdout(&mut self) -> Option<Box<dyn AsyncRead + Send + Unpin>> {
        self.stdout
            .take()
            .map(|x| Box::new(x) as Box<dyn AsyncRead + Send + Unpin>)
    }

    fn take_stderr(&mut self) -> Option<Box<dyn AsyncRead + Send + Unpin>> {
        self.stderr
            .take()
            .map(|x| Box::new(x) as Box<dyn AsyncRead + Send + Unpin>)
    }

    fn kill(&mut self) -> Result<(), io::Error> {
        self.kill()
    }

    fn wait(&mut self) -> Pin<Box<dyn Future<Output = Result<ExitStatus, io::Error>>>> {
        self.status().boxed_local()
    }
}

// CommandRunner Combinators

/// CommandRunner that maps commands before passing them to the inner CommandRunner.
/// Useful to implement aliases or other command transformations.
struct Map {
    inner: Rc<dyn InnerCommandRunner>,
    map_cmd: Rc<dyn Fn(Command) -> Command>,
}

impl InnerCommandRunner for Map {
    fn wrap_command(&self, command: Command) -> Command {
        let cmd = (self.map_cmd)(command);
        self.inner.wrap_command(cmd)
    }
    fn spawn(&self, command: Command) -> io::Result<Box<dyn Child + Send>> {
        let cmd = (self.map_cmd)(command);
        self.inner.spawn(cmd)
    }
    fn output(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<std::process::Output>>>> {
        let cmd = (self.map_cmd)(command);
        self.inner.output(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smol::block_on;

    #[test]
    fn test_null_command_runner_default_output() {
        let runner = CommandRunner::new_null();
        let cmd = Command::new("some-command");

        let output = block_on(runner.output(cmd)).unwrap();

        assert!(output.status.success());
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }

    #[test]
    fn test_null_command_runner_configured_response() {
        let runner = NullCommandRunnerBuilder::new()
            .cmd(&["echo", "hello"], "hello world\n")
            .build();

        let cmd = Command::new_with_args("echo", ["hello"]);
        let output = block_on(runner.output(cmd)).unwrap();

        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "hello world\n");
    }

    #[test]
    fn test_output_string() {
        let runner = NullCommandRunnerBuilder::new()
            .cmd(&["cat", "file.txt"], "file contents")
            .build();

        let cmd = Command::new_with_args("cat", ["file.txt"]);
        let result = block_on(runner.output_string(cmd)).unwrap();

        assert_eq!(result, "file contents");
    }

    #[test]
    fn test_output_tracker() {
        let runner = CommandRunner::new_null();
        let tracker = runner.output_tracker();

        assert_eq!(tracker.len(), 0);

        let cmd = Command::new_with_args("ls", ["-la"]);
        let _ = block_on(runner.output(cmd));

        let items = tracker.items();
        assert_eq!(items.len(), 2); // Started + Output events

        match &items[0] {
            CommandRunnerEvent::Started(_, cmd) => {
                assert_eq!(cmd.program.to_string_lossy(), "ls");
            }
            _ => panic!("Expected Started event"),
        }
    }

    #[test]
    fn test_map_cmd() {
        let runner = NullCommandRunnerBuilder::new()
            .cmd(&["wrapped", "original-cmd", "arg1"], "mapped output")
            .build();

        // Map all commands to be prefixed with "wrapped"
        let mapped_runner = runner.map_cmd(|mut cmd| {
            let original_program = cmd.program.clone();
            cmd.program = "wrapped".into();
            cmd.args.insert(0, original_program);
            cmd
        });

        let cmd = Command::new_with_args("original-cmd", ["arg1"]);
        let result = block_on(mapped_runner.output_string(cmd)).unwrap();

        assert_eq!(result, "mapped output");
    }

    #[test]
    fn test_wrap_command() {
        let runner = CommandRunner::new_null();

        // Default NullCommandRunner doesn't modify commands
        let cmd = Command::new_with_args("test", ["arg"]);
        let wrapped = runner.wrap_command(cmd.clone());

        assert_eq!(wrapped.program, cmd.program);
        assert_eq!(wrapped.args, cmd.args);
    }

    #[test]
    fn test_stub_child_wait_multiple_times() {
        // Test that the closure-based exit_status allows multiple waits
        let stub = StubChild::new_null(vec![], Cursor::new("output"), Cursor::new(""), || {
            Ok(ExitStatus::from_raw(0))
        });

        let mut boxed: Box<dyn Child + Send> = Box::new(stub);

        // First wait
        let status1 = block_on(boxed.wait()).unwrap();
        assert!(status1.success());

        // Second wait should also work (this was the bug with Option-based approach)
        let status2 = block_on(boxed.wait()).unwrap();
        assert!(status2.success());
    }

    #[test]
    fn test_stub_child_take_stdin_stdout() {
        let mut stub = StubChild::new_null(
            vec![1, 2, 3],
            Cursor::new("stdout data"),
            Cursor::new("stderr data"),
            || Ok(ExitStatus::from_raw(0)),
        );

        // First take should succeed
        assert!(stub.take_stdin().is_some());
        assert!(stub.take_stdout().is_some());

        // Second take should return None
        assert!(stub.take_stdin().is_none());
        assert!(stub.take_stdout().is_none());
    }

    #[test]
    fn test_stub_child_kill() {
        let mut stub = StubChild::new_null(vec![], Cursor::new(""), Cursor::new(""), || {
            Ok(ExitStatus::from_raw(0))
        });

        // kill should always succeed for stub
        assert!(stub.kill().is_ok());
    }

    #[test]
    fn test_command_runner_event_accessors() {
        let cmd = Command::new("test");

        let spawned = CommandRunnerEvent::Spawned(1, cmd.clone());
        assert_eq!(spawned.event_id(), 1);
        assert!(spawned.command().is_some());

        let started = CommandRunnerEvent::Started(2, cmd.clone());
        assert_eq!(started.event_id(), 2);
        assert!(started.command().is_some());

        let output = CommandRunnerEvent::Output(3, Ok(()));
        assert_eq!(output.event_id(), 3);
        assert!(output.command().is_none());
    }

    #[test]
    fn test_null_command_runner_spawn() {
        let runner = NullCommandRunnerBuilder::new()
            .cmd(&["test-cmd"], "spawn output")
            .build();

        let cmd = Command::new("test-cmd");
        let mut child = runner.spawn(cmd).unwrap();

        // Should be able to wait on spawned child
        let status = block_on(child.wait()).unwrap();
        assert!(status.success());
    }

    #[test]
    fn test_command_runner_default() {
        // Default should create a null runner
        let runner = CommandRunner::default();
        let cmd = Command::new("anything");

        let output = block_on(runner.output(cmd)).unwrap();
        assert!(output.status.success());
    }

    #[test]
    fn test_null_command_runner_builder_cmd_full() {
        use std::sync::atomic::{AtomicU32, Ordering};

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let runner = NullCommandRunnerBuilder::new()
            .cmd_full(Command::new_with_args("counter", ["cmd"]), move || {
                let count = call_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok(format!("call {}", count))
            })
            .build();

        let cmd1 = Command::new_with_args("counter", ["cmd"]);
        let result1 = block_on(runner.output_string(cmd1)).unwrap();
        assert_eq!(result1, "call 0");

        let cmd2 = Command::new_with_args("counter", ["cmd"]);
        let result2 = block_on(runner.output_string(cmd2)).unwrap();
        assert_eq!(result2, "call 1");
    }
}
