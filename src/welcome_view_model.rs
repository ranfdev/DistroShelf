use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib::BoxedAnyObject;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::sync::OnceLock;

use crate::app_view_model;
use crate::app_view_model::AppViewModel;
use crate::container::Container;
use crate::distrobox::ExportableApp;
use crate::distrobox_service::DistroboxService;
use crate::tagged_object::TaggedObject;

mod imp {
    use crate::app_view_model::AppViewModel;

    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::WelcomeViewModel)]
    pub struct WelcomeViewModel {
        #[property(get, set)]
        app_view_model: RefCell<AppViewModel>,
        #[property(get, set, nullable)]
        terminal_error: RefCell<Option<String>>,
        #[property(get, set, nullable)]
        distrobox_error: RefCell<Option<String>>,
        /// pages: "distrobox", "terminal"
        #[property(get, set)]
        current_page: RefCell<String>,
    }

    #[glib::derived_properties]
    impl ObjectImpl for WelcomeViewModel {}

    #[glib::object_subclass]
    impl ObjectSubclass for WelcomeViewModel {
        const NAME: &'static str = "WelcomeViewModel";
        type Type = super::WelcomeViewModel;
    }
}

glib::wrapper! {
    pub struct WelcomeViewModel(ObjectSubclass<imp::WelcomeViewModel>);
}
impl WelcomeViewModel {
    pub fn new(app_view_model: &AppViewModel) -> Self {
        glib::Object::builder()
            .property("app-view-model", app_view_model)
            .build()
    }
    pub fn continue_to_terminal_page(&self) {
        if let Some(e) = self.app_view_model().distrobox_service().version().error() {
            self.set_distrobox_error(Some(e.to_string()));
        } else {
            self.set_current_page("terminal");
        }
    }
    pub fn complete_setup(&self) {
        if self
            .app_view_model()
            .distrobox_service()
            .selected_terminal()
            .is_some()
        {
            let this = self.clone();
            glib::MainContext::ref_thread_default().spawn_local(async move {
                match this
                    .app_view_model()
                    .distrobox_service()
                    .validate_terminal()
                    .await
                {
                    Ok(_) => {
                        this.app_view_model()
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

impl Default for WelcomeViewModel {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
