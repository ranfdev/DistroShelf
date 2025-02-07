// You can copy/paste this file every time you need a simple GObject
// to hold some data

use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib;
use std::cell::RefCell;
use std::sync::OnceLock;

mod imp {
    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::TaggedObject)]
    pub struct TaggedObject {
        #[property(get, set)]
        tag: RefCell<String>,
        #[property(get, set, nullable)]
        object: RefCell<Option<glib::Object>>,
    }

    #[glib::derived_properties]
    impl ObjectImpl for TaggedObject {}

    #[glib::object_subclass]
    impl ObjectSubclass for TaggedObject {
        const NAME: &'static str = "TaggedObject";
        type Type = super::TaggedObject;
    }
}

glib::wrapper! {
    pub struct TaggedObject(ObjectSubclass<imp::TaggedObject>);
}
impl TaggedObject {
    pub fn new(tag: &str) -> Self {
        glib::Object::builder().property("tag", tag).build()
    }
    pub fn with_object(tag: &str, object: &impl IsA<glib::Object>) -> Self {
        glib::Object::builder()
            .property("tag", tag)
            .property("object", object)
            .build()
    }
}

impl Default for TaggedObject {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
