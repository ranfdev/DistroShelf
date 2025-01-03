// thin wrapper to provide a `spawn` function and a configurable `NullCommandRunner`
// returning predefined outputs, to ease code testing.

use std::{
    collections::HashMap,
    future::Future,
    io::{self},
    os::unix::process::ExitStatusExt,
    pin::Pin,
    process::ExitStatus,
};

use crate::distrobox::Command;
use async_process::{Command as AsyncCommand, Output};
use futures::{
    io::{AsyncRead, AsyncWrite, Cursor},
    FutureExt,
};

pub trait CommandRunner {
    fn spawn(&self, command: Command) -> io::Result<Box<dyn Child + Send>>;
    fn output(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<std::process::Output>>>>;
}

#[derive(Clone, Debug)]
pub struct RealCommandRunner {}

impl CommandRunner for RealCommandRunner {
    fn spawn(&self, command: Command) -> io::Result<Box<dyn Child + Send>> {
        let mut command: AsyncCommand = command.into();
        Ok(Box::new(dbg!(command.spawn()?)))
    }
    fn output(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<async_process::Output>>>> {
        let mut command: AsyncCommand = command.into();
        command.output().boxed()
    }
}

#[derive(Default, Debug, Clone)]
pub struct NullCommandRunnerBuilder {
    responses: HashMap<Vec<String>, (Vec<u8>, ExitStatus)>,
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
        self.cmd_full(cmd, out)
    }
    pub fn cmd_full<T: AsRef<str>>(&mut self, cmd: Command, out: T) -> &mut Self {
        
        let key = NullCommandRunner::key_for_cmd(&cmd);
        dbg!(&key);
        self.responses
            .insert(key, (out.as_ref().as_bytes().to_vec(), ExitStatus::from_raw(0)));
        self
    }
    pub fn fallback(&mut self, status: ExitStatus) -> &mut Self {
        self.fallback_exit_status = status;
        self
    }
    pub fn build(&self) -> NullCommandRunner {
        NullCommandRunner {
            responses: self.responses.clone(),
            fallback_exit_status: self.fallback_exit_status,
        }
    }
}

#[derive(Default, Clone)]
pub struct NullCommandRunner {
    responses: HashMap<Vec<String>, (Vec<u8>, ExitStatus)>,
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

impl CommandRunner for NullCommandRunner {
    fn spawn(&self, command: Command) -> io::Result<Box<dyn Child + Send>> {
        let key = Self::key_for_cmd(&command);
        let (response, exit_status) = self
            .responses
            .get(&key[..])
            .cloned()
            .unwrap_or((vec![], self.fallback_exit_status));
        let stub = StubChild::new_null(vec![], Cursor::new(response), Ok(exit_status));
        Ok(Box::new(stub))
    }
    fn output(
        &self,
        command: Command,
    ) -> Pin<Box<dyn Future<Output = io::Result<std::process::Output>>>> {
        let key = Self::key_for_cmd(&command);
        dbg!(&key);
        let (response, exit_status) = self
            .responses
            .get(&key[..])
            .cloned()
            .unwrap_or((vec![], self.fallback_exit_status));

        futures::future::ok(Output {
            status: exit_status,
            stdout: response.into(),
            stderr: vec![],
        })
        .boxed()
    }
}

pub trait Child {
    fn take_stdin(&mut self) -> Option<Box<dyn AsyncWrite + Send + Unpin>>;
    fn take_stdout(&mut self) -> Option<Box<dyn AsyncRead + Send + Unpin>>;
    fn kill(&mut self) -> Result<(), io::Error>;
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
        exit_status: io::Result<ExitStatus>,
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
}
