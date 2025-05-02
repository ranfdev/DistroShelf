// You can copy/paste this file every time you need a simple GObject
// to hold some data

use futures::{io::BufReader, AsyncBufReadExt, StreamExt};
use glib::subclass::prelude::*;
use glib::Properties;
use gtk::glib;
use gtk::prelude::*;
use std::cell::Ref;
use std::cell::RefCell;
use std::future::Future;
use tracing::{debug, error, info, warn};

use crate::container_cli::Child;

mod imp {
    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::ContainerCliTask)]
    pub struct DistroboxTask {
        #[property(get, construct_only)]
        target: RefCell<String>,
        #[property(get, construct_only)]
        name: RefCell<String>,
        #[property(get, set)]
        description: RefCell<String>,
        #[property(get)]
        output: gtk::TextBuffer,
        #[property(get, set)]
        pub status: RefCell<String>, // "pending", "executing", "successful", "failed"
        pub error: RefCell<Option<anyhow::Error>>, // set only if status is "failed"
    }

    #[glib::derived_properties]
    impl ObjectImpl for DistroboxTask {}

    #[glib::object_subclass]
    impl ObjectSubclass for DistroboxTask {
        const NAME: &'static str = "DistroboxTask";
        type Type = super::ContainerCliTask;
    }
}

glib::wrapper! {
    pub struct ContainerCliTask(ObjectSubclass<imp::DistroboxTask>);
}
impl ContainerCliTask {
    pub fn new<F: Future<Output = anyhow::Result<()>>>(
        target: &str,
        name: &str,
        f: impl FnOnce(Self) -> F + 'static,
    ) -> Self {
        let this: Self = glib::Object::builder()
            .property("target", target)
            .property("name", name)
            .build();
        let this_clone = this.clone();
        this.set_status("pending".to_string());
        glib::MainContext::ref_thread_default().spawn_local(async move {
            let this_clone_clone = this_clone.clone();
            this_clone.set_status_executing();
            let res = f(this_clone_clone).await;
            if let Err(e) = res {
                this_clone.set_status_failed(e);
            } else {
                this_clone.set_status_successful();
            }
        });
        this
    }

    pub async fn handle_child_output(
        &self,
        mut child: Box<dyn Child + Send>,
    ) -> Result<(), anyhow::Error> {
        debug!("Handling child process output");
        let stdout = child.take_stdout().unwrap();
        let bufread = BufReader::new(stdout);
        let mut lines = bufread.lines();
        while let Some(line) = lines.next().await {
            let line = line?;
            self.output().insert(&mut self.output().end_iter(), &line);
            self.output().insert(&mut self.output().end_iter(), "\n");
        }

        match child.wait().await {
            Ok(e) if e.success() => {
                info!(exit_code = ?e.code(), "Child process exited successfully");
            }
            Ok(e) => {
                warn!(exit_code = ?e.code(), "Child process exited with error");
                anyhow::bail!("Status: {:?}", e.code());
            }
            Err(e) => {
                error!(error = %e, "Child process failed");
                return Err(e.into());
            }
        }
        Ok(())
    }
    pub fn set_status_executing(&self) {
        self.imp().status.replace("executing".to_string());
        self.notify_status();
    }
    pub fn set_status_successful(&self) {
        self.imp().status.replace("successful".to_string());
        self.notify_status();
    }
    pub fn set_status_failed(&self, error: anyhow::Error) {
        self.imp().status.replace("failed".to_string());
        self.imp().error.replace(Some(error));
        self.notify_status();
    }
    pub fn is_failed(&self) -> bool {
        &*self.imp().status.borrow() == "failed"
    }
    pub fn is_successful(&self) -> bool {
        &*self.imp().status.borrow() == "successful"
    }
    pub fn ended(&self) -> bool {
        self.is_failed() || self.is_successful()
    }
    pub fn error(&self) -> Ref<Option<anyhow::Error>> {
        self.imp().error.borrow()
    }
    pub fn take_error(&self) -> Option<anyhow::Error> {
        self.imp().error.borrow_mut().take()
    }
}
