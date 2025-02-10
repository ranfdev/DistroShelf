use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib::BoxedAnyObject;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::sync::OnceLock;

use crate::container::Container;
use crate::distrobox::ExportableApp;
use crate::distrobox_store::DistroboxStore;
use crate::root_store;
use crate::root_store::RootStore;
use crate::tagged_object::TaggedObject;

mod imp {
    use crate::root_store::RootStore;

    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::WelcomeViewStore)]
    pub struct WelcomeViewStore {
        #[property(get, set)]
        root_store: RefCell<RootStore>,
        #[property(get, set, nullable)]
        terminal_error: RefCell<Option<String>>,
        #[property(get, set, nullable)]
        distrobox_error: RefCell<Option<String>>,
        /// pages: "distrobox", "terminal"
        #[property(get, set)]
        current_page: RefCell<String>,
    }

    #[glib::derived_properties]
    impl ObjectImpl for WelcomeViewStore {}

    #[glib::object_subclass]
    impl ObjectSubclass for WelcomeViewStore {
        const NAME: &'static str = "WelcomeViewStore";
        type Type = super::WelcomeViewStore;
    }
}

glib::wrapper! {
    pub struct WelcomeViewStore(ObjectSubclass<imp::WelcomeViewStore>);
}
impl WelcomeViewStore {
    pub fn new(root_store: &RootStore) -> Self {
        glib::Object::builder()
            .property("root-store", root_store)
            .build()
    }
    pub fn continue_to_terminal_page(&self) {
        if let Some(e) = self.root_store().distrobox_store().version().error() {
            self.set_distrobox_error(Some(e.to_string()));
        } else {
            self.set_current_page("terminal");
        }
    }
    pub fn complete_setup(&self) {
        if self
            .root_store()
            .distrobox_store()
            .selected_terminal()
            .is_some()
        {
            let this = self.clone();
            glib::MainContext::ref_thread_default().spawn_local(async move {
                match this
                    .root_store()
                    .distrobox_store()
                    .validate_terminal()
                    .await
                {
                    Ok(_) => {
                        this.root_store()
                            .set_current_view(&TaggedObject::new("main"));
                    }
                    Err(err) => {
                        this.set_terminal_error(Some(format!("{}", err)));
                    }
                }
            });
        }
    }
}

impl Default for WelcomeViewStore {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
