// thin wrapper to provide a `spawn` function and a configurable `NullCommandRunner`
// returning predefined outputs, to ease code testing.

use std::{
    collections::HashMap,
    io::{self, Cursor, Read, Write},
    os::unix::process::ExitStatusExt,
    process::{Command, ExitStatus, Output},
};

pub trait CommandRunner {
    fn spawn(&self, command: &mut Command) -> io::Result<Box<dyn Child>>;
    fn output(&self, command: &mut Command) -> io::Result<std::process::Output>;
}

#[derive(Clone, Debug)]
pub struct RealCommandRunner {}

impl CommandRunner for RealCommandRunner {
    fn spawn(&self, command: &mut Command) -> io::Result<Box<dyn Child>> {
        dbg!("running", &command);
        Ok(Box::new(command.spawn()?))
    }
    fn output(&self, command: &mut Command) -> io::Result<std::process::Output> {
        dbg!("running", &command);
        Ok(command.output()?)
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
    pub fn cmd(&mut self, args: &[&str], out: &str) -> &mut Self {
        let args = args.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        self.responses
            .insert(args, (out.as_bytes().to_vec(), ExitStatus::from_raw(0)));
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
            .get_args()
            .map(|x| x.to_str().unwrap_or_default().to_string())
            .collect();
        key.insert(0, command.get_program().to_string_lossy().to_string());
        key
    }
}

impl CommandRunner for NullCommandRunner {
    fn spawn(&self, command: &mut Command) -> io::Result<Box<dyn Child>> {
        let key = Self::key_for_cmd(command);
        let (response, exit_status) = self
            .responses
            .get(&key[..])
            .cloned()
            .unwrap_or((vec![], self.fallback_exit_status));
        let stub = ChildImpl::new_null(vec![], Cursor::new(response), Ok(exit_status));
        Ok(Box::new(stub))
    }
    fn output(&self, command: &mut Command) -> io::Result<std::process::Output> {
        let key = Self::key_for_cmd(command);
        let (response, exit_status) = self
            .responses
            .get(&key[..])
            .cloned()
            .unwrap_or((vec![], self.fallback_exit_status));
        Ok(Output {
            status: exit_status,
            stdout: response.into(),
            stderr: vec![],
        })
    }
}

pub trait Child {
    fn take_stdin(&mut self) -> Option<Box<dyn Write>>;
    fn take_stdout(&mut self) -> Option<Box<dyn Read>>;
    fn wait(self) -> Result<ExitStatus, io::Error>;
}

struct ChildImpl {
    stdin: Option<Box<dyn Write>>,
    stdout: Option<Box<dyn Read>>,
    exit_status: Option<io::Result<ExitStatus>>,
}

impl ChildImpl {
    fn new_null(
        stdin: impl Write + 'static,
        stdout: impl Read + 'static,
        exit_status: io::Result<ExitStatus>,
    ) -> ChildImpl {
        ChildImpl {
            stdin: Some(Box::new(stdin)),
            stdout: Some(Box::new(stdout)),
            exit_status: Some(exit_status),
        }
    }
}
impl Child for ChildImpl {
    fn take_stdin(&mut self) -> Option<Box<dyn Write>> {
        self.stdin.take()
    }
    fn take_stdout(&mut self) -> Option<Box<dyn Read>> {
        self.stdout.take()
    }
    fn wait(self) -> io::Result<ExitStatus> {
        self.exit_status.unwrap()
    }
}

impl Child for std::process::Child {
    fn take_stdin(&mut self) -> Option<Box<dyn Write>> {
        self.stdin.take().map(|x| Box::new(x) as Box<dyn Write>)
    }
    fn take_stdout(&mut self) -> Option<Box<dyn Read>> {
        self.stdout.take().map(|x| Box::new(x) as Box<dyn Read>)
    }
    fn wait(mut self) -> Result<ExitStatus, io::Error> {
        std::process::Child::wait(&mut self)
    }
}
