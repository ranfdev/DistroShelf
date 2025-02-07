// Generated by RustGObjectGenerator
// This file is licensed under the same terms as the project it belongs to

use adw::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::{glib, prelude::*};
use std::cell::RefCell;
use std::sync::OnceLock;

mod imp {
    use crate::{
        app_view_model::AppViewModel, distrobox_service::DistroboxService, gtk_utils::reaction,
        terminal_combo_row::TerminalComboRow, welcome_view_model::WelcomeViewModel,
    };

    use super::*;

    #[derive(Properties, Default, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::WelcomeView)]
    #[template(file = "welcome_view.ui")]
    pub struct WelcomeView {
        #[property(get, set=Self::set_model)]
        model: RefCell<WelcomeViewModel>,

        #[template_child]
        carousel: TemplateChild<adw::Carousel>,
        #[template_child]
        terminal_preferences_page: TemplateChild<adw::Clamp>,
        #[template_child]
        distrobox_page: TemplateChild<adw::Clamp>,
        #[template_child]
        terminal_combo_row: TemplateChild<TerminalComboRow>,
    }

    impl WelcomeView {
        pub fn set_model(&self, model: &WelcomeViewModel) {
            let obj = self.obj().to_owned();
            reaction!(model.current_page(), move |page: String| {
                match page.as_str() {
                    "terminal" => obj
                        .imp()
                        .carousel
                        .scroll_to(&*obj.imp().terminal_preferences_page, true),
                    "distrobox" => {
obj
                        .imp()
                        .carousel
                        .scroll_to(&*obj.imp().distrobox_page, true)
                    },
                    _ => {}
                }
            });
            self.model.replace(model.clone());
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for WelcomeView {}

    #[glib::object_subclass]
    impl ObjectSubclass for WelcomeView {
        const NAME: &'static str = "WelcomeView";
        type Type = super::WelcomeView;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl WidgetImpl for WelcomeView {}
    impl BinImpl for WelcomeView {}

    #[gtk::template_callbacks]
    impl WelcomeView {
        #[template_callback]
        fn continue_to_terminal_page(&self, _: &gtk::Button) {
            let obj = self.obj();
            obj.model().continue_to_terminal_page();
        }
        #[template_callback]
        fn continue_to_app(&self, _: &gtk::Button) {
            self.obj().model().complete_setup();
        }
    }
}

glib::wrapper! {
    pub struct WelcomeView(ObjectSubclass<imp::WelcomeView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}
