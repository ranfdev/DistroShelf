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
};

use crate::fakers::{Command, OutputTracker};

use async_process::{Command as AsyncCommand, Output};
use futures::{
    io::{AsyncRead, AsyncWrite, Cursor},
    FutureExt,
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
        command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<std::process::Output>>>> {
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
    fallback_exit_status: ExitStatus,
}

impl NullCommandRunnerBuilder {
    pub fn new() -> Self {
        Default::default()
    }
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
        let stub = StubChild::new_null(
            vec![],
            Cursor::new(response()?),
            Ok(ExitStatus::from_raw(0)),
        );
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
    fn kill(&mut self) -> Result<(), io::Error>;
    fn wait(&mut self) -> Pin<Box<dyn Future<Output = Result<ExitStatus, io::Error>>>>;
}

struct StubChild {
    stdin: Option<Box<dyn AsyncWrite + Send + Unpin>>,
    stdout: Option<Box<dyn AsyncRead + Send + Unpin>>,
    exit_status: Option<io::Result<ExitStatus>>,
}

impl StubChild {
    fn new_null(
        stdin: impl AsyncWrite + Send + Unpin + 'static,
        stdout: impl AsyncRead + Send + Unpin + 'static,
        exit_status: io::Result<ExitStatus>, // TODO: replace with a closure, so that we can use it multiple times
    ) -> StubChild {
        StubChild {
            stdin: Some(Box::new(stdin)),
            stdout: Some(Box::new(stdout)),
            exit_status: Some(exit_status),
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

    fn kill(&mut self) -> Result<(), io::Error> {
        unimplemented!()
    }
    fn wait(&mut self) -> Pin<Box<dyn Future<Output = Result<ExitStatus, io::Error>>>> {
        async { Ok(ExitStatus::from_raw(0)) }.boxed_local()
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

    fn kill(&mut self) -> Result<(), io::Error> {
        self.kill()
    }

    fn wait(&mut self) -> Pin<Box<dyn Future<Output = Result<ExitStatus, io::Error>>>> {
        self.status().boxed_local()
    }
}
