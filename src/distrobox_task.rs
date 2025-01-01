// You can copy/paste this file every time you need a simple GObject
// to hold some data

use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib;
use std::cell::RefCell;
use std::future::Future;
use std::sync::OnceLock;

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
        #[property(get, set)]
        pub status: RefCell<String>, // "pending", "executing", "successful", "failed"
        pub error: RefCell<Option<anyhow::Error>>, // set only if status is "failed"
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
    pub fn take_error(&self) -> Option<anyhow::Error> {
        self.imp().error.borrow_mut().take()
    }
}
