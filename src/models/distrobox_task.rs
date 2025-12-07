use futures::{AsyncBufReadExt, FutureExt, StreamExt, io::BufReader};
use glib::Properties;
use glib::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use std::cell::Ref;
use std::cell::RefCell;
use std::future::Future;
use tracing::{debug, info, warn};

use crate::fakers::Child;
use crate::widgets::TaskOutputTerminal;

/// Status of a DistroboxTask
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "TaskStatus")]
pub enum TaskStatus {
    #[default]
    Pending,
    Executing,
    Successful,
    Failed,
}

mod imp {
    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::DistroboxTask)]
    pub struct DistroboxTask {
        #[property(get, construct_only)]
        target: RefCell<String>,
        #[property(get, construct_only)]
        name: RefCell<String>,
        #[property(get, set)]
        description: RefCell<String>,
        #[property(get)]
        output: gtk::TextBuffer,
        #[property(get, set, builder(TaskStatus::default()))]
        pub status: RefCell<TaskStatus>,
        pub error: RefCell<Option<anyhow::Error>>, // set only if status is Failed
        pub cancellable: RefCell<Option<gtk::gio::Cancellable>>,
        pub vte_terminal: RefCell<Option<TaskOutputTerminal>>, // Optional VTE terminal
    }

    #[glib::derived_properties]
    impl ObjectImpl for DistroboxTask {}

    #[glib::object_subclass]
    impl ObjectSubclass for DistroboxTask {
        const NAME: &'static str = "DistroboxTask";
        type Type = super::DistroboxTask;
    }
}

glib::wrapper! {
    pub struct DistroboxTask(ObjectSubclass<imp::DistroboxTask>);
}
impl DistroboxTask {
    pub fn new<F: Future<Output = anyhow::Result<()>>>(
        target: &str,
        name: &str,
        f: impl FnOnce(Self) -> F + 'static,
    ) -> Self {
        let cancellable = gtk::gio::Cancellable::new();
        let this: Self = glib::Object::builder()
            .property("target", target)
            .property("name", name)
            .property("status", TaskStatus::Pending)
            .build();

        *this.imp().cancellable.borrow_mut() = Some(cancellable.clone());

        let this_clone = this.clone();
        glib::MainContext::ref_thread_default().spawn_local(async move {
            let this_clone_clone = this_clone.clone();
            this_clone.set_status_executing();
            let res = f(this_clone_clone).await;

            if cancellable.is_cancelled() {
                this_clone.set_status_failed(anyhow::anyhow!("Task cancelled"));
            } else if let Err(e) = res {
                this_clone.set_status_failed(e);
            } else {
                this_clone.set_status_successful();
            }
        });
        this
    }

    pub fn stop(&self) {
        if let Some(c) = self.imp().cancellable.borrow().as_ref() {
            c.cancel();
        }
    }

    pub async fn handle_child_output(
        &self,
        mut child: Box<dyn Child + Send>,
    ) -> Result<(), anyhow::Error> {
        debug!("Handling child process output");

        let cancellable = self.imp().cancellable.borrow().clone();
        let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel::<()>();

        // Keep the handler alive
        let _handler = cancellable.as_ref().map(|c| {
            c.connect_cancelled(move |_| {
                let _ = cancel_tx.send(());
            })
        });

        let stdout = child.take_stdout().unwrap();
        let stderr = child.take_stderr().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        let mut err_lines = BufReader::new(stderr).lines();

        let insert_line = |line: String| {
            // Write to TextBuffer
            self.output().insert(&mut self.output().end_iter(), &line);
            self.output().insert(&mut self.output().end_iter(), "\n");
            
            // Write to VTE terminal if available
            if let Some(vte) = self.imp().vte_terminal.borrow().as_ref() {
                vte.write_line(&line);
            }
        };

        let mut cancel_rx = cancel_rx.fuse();

        loop {
            futures::select! {
                line = lines.next().fuse() => {
                    match line {
                        Some(Ok(line)) => {
                            insert_line(line);
                        }
                        Some(Err(e)) => return Err(e.into()),
                        None => break,
                    }
                },
                err_line = err_lines.next().fuse() => {
                    match err_line {
                        Some(Ok(line)) => {
                            insert_line(line);
                        }
                        Some(Err(e)) => return Err(e.into()),
                        None => break,
                    }
                },
                _ = cancel_rx => {
                    info!("Task cancelled, killing child process");
                    let _ = child.kill();
                    return Err(anyhow::anyhow!("Task cancelled"));
                }
            }
        }

        match child.wait().await {
            Ok(e) if e.success() => {
                info!(exit_code = ?e.code(), "Child process exited successfully");
                Ok(())
            }
            Ok(e) => {
                warn!(exit_code = ?e.code(), "Child process exited with error");
                Err(anyhow::anyhow!(
                    "Child process exited with error: {:?}",
                    e.code()
                ))
            }
            Err(e) => Err(e.into()),
        }
    }
    pub fn set_status_executing(&self) {
        self.set_status(TaskStatus::Executing);
    }
    pub fn set_status_successful(&self) {
        self.set_status(TaskStatus::Successful);
    }
    pub fn set_status_failed(&self, error: anyhow::Error) {
        self.imp().error.replace(Some(error));
        self.set_status(TaskStatus::Failed);
    }
    pub fn is_failed(&self) -> bool {
        self.status() == TaskStatus::Failed
    }
    pub fn is_successful(&self) -> bool {
        self.status() == TaskStatus::Successful
    }
    pub fn ended(&self) -> bool {
        self.is_failed() || self.is_successful()
    }
    pub fn error(&self) -> Ref<'_, Option<anyhow::Error>> {
        self.imp().error.borrow()
    }
    pub fn take_error(&self) -> Option<anyhow::Error> {
        self.imp().error.borrow_mut().take()
    }
    pub fn error_message(&self) -> Option<String> {
        self.imp().error.borrow().as_ref().map(|e| e.to_string())
    }

    /// Set the VTE terminal for this task to write output to
    pub fn set_vte_terminal(&self, terminal: Option<TaskOutputTerminal>) {
        *self.imp().vte_terminal.borrow_mut() = terminal;
    }

    /// Get the current VTE terminal if one is attached
    pub fn vte_terminal(&self) -> Option<TaskOutputTerminal> {
        self.imp().vte_terminal.borrow().clone()
    }
}
