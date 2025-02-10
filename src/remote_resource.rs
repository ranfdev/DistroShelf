// You can copy/paste this file every time you need a simple GObject
// to hold some data

use futures::FutureExt;
use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib;
use std::cell::RefCell;
use std::future::Future;
use std::sync::OnceLock;

mod imp {
    use std::{cell::OnceCell, future::Future, pin::Pin};

    use super::*;

    /// Manages a resource (not necessarily a gio resource) that can be loaded from outside the app.
    /// The loading can fail: the error will be stored inside this object.
    /// The data is available after the first load, even during successive reloads,
    /// while the data is stale.
    /// A reconciler can be provided to mutate the existing data, instead of replacing it.
    /// That can be useful to mutate a `gio::ListStore` instead of replacing it.
    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::RemoteResource)]
    pub struct RemoteResource {
        #[property(get, set)]
        pub loading: RefCell<bool>,
        #[property(get, set)]
        pub error: RefCell<String>,
        #[property(get, set, nullable)]
        pub data: RefCell<Option<glib::Object>>,
        pub loader:
            OnceCell<Box<dyn Fn() -> Pin<Box<dyn Future<Output = anyhow::Result<glib::Object>>>>>>,
        pub reconciler: RefCell<Option<Box<dyn Fn(&glib::Object, &glib::Object)>>>,
    }

    #[glib::derived_properties]
    impl ObjectImpl for RemoteResource {}

    #[glib::object_subclass]
    impl ObjectSubclass for RemoteResource {
        const NAME: &'static str = "RemoteResource";
        type Type = super::RemoteResource;
    }
}

glib::wrapper! {
    pub struct RemoteResource(ObjectSubclass<imp::RemoteResource>);
}
impl RemoteResource {
    pub fn new<T, F: Future<Output = anyhow::Result<T>> + 'static>(
        loader: impl Fn() -> F + 'static,
        reconciler: Option<impl Fn(&T, &T) -> () + 'static>,
    ) -> Self
    where
        T: IsA<glib::Object>,
    {
        let this: Self = glib::Object::builder().build();
        this.imp().reconciler.replace(reconciler.map(|r| {
            Box::new(move |prev: &glib::Object, current: &glib::Object| {
                r(
                    &prev.downcast_ref().unwrap(),
                    &current.downcast_ref().unwrap(),
                )
            }) as Box<dyn Fn(&glib::Object, &glib::Object)>
        }));
        this.imp()
            .loader
            .set(Box::new(move || {
                let data = loader();
                { async move { data.await.map(|obj| obj.upcast()) } }.boxed_local()
            }))
            .map_err(|_| "loader already set")
            .unwrap();
        this
    }
    pub fn reload(&self) {
        let weak = self.downgrade();
        self.imp().loading.replace(true);
        glib::MainContext::ref_thread_default().spawn_local(async move {
            let Some(this) = weak.upgrade() else {
                return;
            };
            let res = this.imp().loader.get().unwrap()().await;
            match res {
                Ok(res) => {
                    if let Some(reconciler) = this.imp().reconciler.borrow().as_ref() {
                        if let Some(data) = this.data() {
                            reconciler(&data, &res);
                        }
                    } else {
                        this.set_data(Some(&res));
                    };
                }
                Err(e) => {
                    this.set_error(format!("{}", e));
                }
            }
        });
    }
    pub fn typed_data<T: IsA<glib::Object>>(&self) -> Option<T> {
        self.data().and_then(|d| d.downcast().ok())
    }
}

impl Default for RemoteResource {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
