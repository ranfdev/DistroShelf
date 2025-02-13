// You can copy/paste this file every time you need a simple GObject
// to hold some data

use futures::FutureExt;
use glib::prelude::*;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib;
use std::cell::RefCell;
use std::sync::OnceLock;
use std::{any::Any, cell::OnceCell, future::Future, pin::Pin};

type AnyLoaderClosure =
    Box<dyn Fn(Option<&dyn Any>) -> Pin<Box<dyn Future<Output = anyhow::Result<Box<dyn Any>>>>>>;
mod imp {
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
        #[property(get)]
        pub loading: RefCell<bool>,
        #[property(get, set, nullable)]
        pub error: RefCell<Option<String>>,
        pub data: RefCell<Option<Box<dyn Any>>>,
        pub loader: OnceCell<AnyLoaderClosure>,
    }

    #[glib::derived_properties]
    impl ObjectImpl for RemoteResource {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("data-changed").build()])
        }
    }

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
    pub fn new<T: 'static, F: Future<Output = anyhow::Result<T>> + 'static>(
        loader: impl Fn(Option<&T>) -> F + 'static,
    ) -> Self {
        let this: Self = glib::Object::builder().build();
        this.imp()
            .loader
            .set(Box::new(move |prev_data| {
                let prev_data = prev_data.and_then(|x| x.downcast_ref::<T>());
                let data = loader(prev_data);
                { async move { data.await.map(|obj| Box::new(obj) as Box<dyn Any>) } }.boxed_local()
            }))
            .map_err(|_| "loader already set")
            .unwrap();
        this
    }
    pub fn reload(&self) {
        let weak = self.downgrade();
        self.imp().loading.replace(true);
        self.notify_loading();
        glib::MainContext::ref_thread_default().spawn_local(async move {
            let Some(this) = weak.upgrade() else {
                return;
            };
            let data = this.imp().data.take();
            let res = this.imp().loader.get().unwrap()(data.as_deref()).await;
            match res {
                Ok(res) => {
                    this.imp().data.replace(Some(res));
                    this.set_error(None::<String>);
                    this.emit_by_name("data-changed", &[])
                }
                Err(e) => {
                    this.imp().data.replace(data);
                    this.set_error(Some(format!("{}", e)));
                }
            }
            this.imp().loading.replace(false);
            this.notify_loading();
        });
    }
    pub fn data<T: Clone + 'static>(&self) -> Option<T> {
        self.imp()
            .data
            .borrow()
            .as_ref()
            .map(|d| (d.downcast_ref::<T>().unwrap()).clone())
    }
    pub fn connect_data_changed(&self, f: impl Fn(&Self) + 'static) {
        self.connect_local("data-changed", true, move |values| {
            let this = values[0].get::<RemoteResource>().unwrap();
            f(&this);
            None
        });
    }
}

impl Default for RemoteResource {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}
