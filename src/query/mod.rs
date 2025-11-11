use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use crate::query;

pub struct QueryInner<T, E> {
    key: String,
    /// The current data (if any successful fetch has occurred)
    pub data: Option<T>,
    /// Whether a fetch is currently in progress
    pub is_loading: bool,
    /// The last error (if any)
    pub error: Option<Rc<E>>,
    /// Timestamp of the last successful fetch
    pub last_fetched_at: Option<SystemTime>,
    query_fn:
        Option<Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>>>>>>,
    refetch_source_id: Option<glib::SourceId>,
    /// Active fetch task handle - cancellable when dropped
    fetch_task_handle: Option<glib::JoinHandle<()>>,
    query_obj: AsyncQuery,
}

impl<T, E> QueryInner<T, E> {
    pub fn new(key: String) -> Self {
        Self {
            key: key,
            data: None,
            is_loading: false,
            error: None,
            last_fetched_at: None,
            query_fn: None,
            refetch_source_id: None,
            fetch_task_handle: None,
            query_obj: glib::Object::new::<AsyncQuery>(),
        }
    }

    /// Check if the data is stale based on a given duration
    /// Returns true if data has never been fetched or if the duration has elapsed
    pub fn is_stale(&self, max_age: Duration) -> bool {
        match self.last_fetched_at {
            None => true,
            Some(fetched_at) => SystemTime::now()
                .duration_since(fetched_at)
                .map(|elapsed| elapsed > max_age)
                .unwrap_or(true),
        }
    }

    /// Get the age of the data since last fetch
    /// Returns None if data has never been fetched
    pub fn age(&self) -> Option<Duration> {
        self.last_fetched_at
            .and_then(|fetched_at| SystemTime::now().duration_since(fetched_at).ok())
    }
}

glib::wrapper! {
    pub struct AsyncQuery(ObjectSubclass<imp::AsyncQuery>);
}

mod imp {
    use super::*;
    use gtk::glib;
    use gtk::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::AsyncQuery)]
    pub struct AsyncQuery {
        #[property(get, set)]
        is_loading: RefCell<bool>,

        #[property(get, set)]
        is_error: RefCell<bool>,

        #[property(get, set)]
        is_success: RefCell<bool>,

        #[property(get, set, nullable)]
        error_message: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AsyncQuery {
        const NAME: &'static str = "AsyncQuery";
        type Type = super::AsyncQuery;
    }

    #[glib::derived_properties]
    impl ObjectImpl for AsyncQuery {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> =
                std::sync::OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("success")
                        .run_last()
                        .build(),
                    glib::subclass::Signal::builder("error")
                        .run_last()
                        .param_types([glib::Type::STRING])
                        .build(),
                ]
            })
        }
    }
}

pub struct QueryOptions<T, E, F>
where
    F: Future<Output = Result<T, E>> + 'static,
{
    /// Unique key for this query (for caching/deduplication)
    pub key: String,

    /// The async function that fetches data
    pub query_fn: Box<dyn Fn() -> F>,

    /// Whether to execute immediately or wait for manual trigger
    pub enabled: bool,

    /// Refetch interval in seconds (None = no auto-refetch)
    pub refetch_interval: Option<u32>,
}

pub struct Query<T, E> {
    inner: Rc<RefCell<QueryInner<T, E>>>,
}

impl<T, E> Clone for Query<T, E>
where
    T: Clone + 'static,
    E: std::fmt::Display + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<T: Clone + 'static, E: std::fmt::Display + 'static> Default for Query<T, E>
where
    T: Default,
{
    fn default() -> Self {
        Self::new("default".into(), || async { Ok(T::default()) })
    }
}

impl<T, E> Drop for Query<T, E> {
    fn drop(&mut self) {
        let is_last = Rc::strong_count(&self.inner) == 1;
        if is_last {
            // Remove the refetch timer if present
            if let Some(source_id) = self.inner.borrow_mut().refetch_source_id.take() {
                source_id.remove();
            }

            // Abort any active fetch task to ensure cleanup
            if let Some(handle) = self.inner.borrow_mut().fetch_task_handle.take() {
                handle.abort();
            }
        }
    }
}

impl<T, E> Query<T, E>
where
    T: Clone + 'static,
    E: std::fmt::Display + 'static,
{
    pub fn new<F: Future<Output = Result<T, E>> + 'static>(
        key: String,
        query_fn: impl Fn() -> F + 'static,
    ) -> Self {
        Self::new_with_options(QueryOptions {
            key,
            query_fn: Box::new(query_fn),
            enabled: false,
            refetch_interval: None,
        })
    }
    pub fn new_with_options<F>(options: QueryOptions<T, E, F>) -> Self
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        let inner = Rc::new(RefCell::new(QueryInner::new(options.key.clone())));

        let query = Self {
            inner: inner.clone(),
        };

        if options.enabled {
            query.fetch();
        }

        // Setup auto-refetch if interval specified
        if let Some(interval) = options.refetch_interval {
            let weak = Rc::downgrade(&inner);
            let source_id = glib::timeout_add_seconds_local(interval, move || {
                Self::from_weak(&weak)
                    .map(|query| {
                        query.fetch();
                    });
                glib::ControlFlow::Continue
            });
            inner.borrow_mut().refetch_source_id = Some(source_id);
        }
        query
    }

    fn from_weak(weak: &std::rc::Weak<RefCell<QueryInner<T, E>>>) -> Option<Self> {
        weak.upgrade().map(|inner| Self { inner })
    }

    /// Execute a fetch operation and handle the result
    async fn execute_fetch(inner: &Rc<RefCell<QueryInner<T, E>>>) {
        let query_obj = {inner.borrow().query_obj.clone()};
        let Some(future) = inner.borrow().query_fn.as_ref().map(|f| f()) else {
            return;
        };
        let result = future.await;

        match result {
            Ok(_data) => {
                inner.borrow_mut().data = Some(_data.clone());
                inner.borrow_mut().is_loading = false;
                inner.borrow_mut().error = None;
                inner.borrow_mut().last_fetched_at = Some(SystemTime::now());
                query_obj.set_is_loading(false);
                query_obj.set_is_success(true);
                query_obj.set_is_error(false);
                query_obj.set_error_message(None::<String>);

                // Emit success signal
                query_obj.emit_by_name::<()>("success", &[]);
            }
            Err(error) => {
                let rc_error = Rc::new(error);
                let error_msg = rc_error.to_string();
                // Keep the previous data, just mark as error
                inner.borrow_mut().is_loading = false;
                inner.borrow_mut().error = Some(rc_error);
                query_obj.set_is_loading(false);
                query_obj.set_is_error(true);
                query_obj.set_is_success(inner.borrow().data.is_some());
                query_obj.set_error_message(Some(error_msg.clone()));

                // Emit error signal with error message
                query_obj.emit_by_name::<()>("error", &[&error_msg]);
            }
        }
    }

    pub fn fetch(&self)
    {
        let query_obj = {self.inner.borrow().query_obj.clone()};
        // Cancel any previous fetch task before starting a new one
        if let Some(handle) = self.inner.borrow_mut().fetch_task_handle.take() {
            handle.abort();
        }

        // Set loading inner, but preserve any previous data
        query_obj.set_is_loading(true);
        query_obj.set_is_error(false);
        query_obj.set_is_success(false);
        self.inner.borrow_mut().is_loading = true;

        let inner = self.inner.clone();

        // Spawn the async task on GLib main loop and store the handle
        let handle = glib::spawn_future_local(async move {
            Self::execute_fetch(&inner).await;
        });

        self.inner.borrow_mut().fetch_task_handle = Some(handle);
    }

    pub fn refetch(&self) {
        self.fetch();
    }

    pub fn set_fetcher<F>(&self, query_fn: impl Fn() -> F + 'static)
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        self.inner.borrow_mut().query_fn = Some(Box::new(move || Box::pin(query_fn())));
    }

    pub fn connect_success<F: Fn(&T) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let inner = self.inner.clone();
        let query_obj = {inner.borrow().query_obj.clone()};
        query_obj
            .connect_local("success", false, move |_args| {
                if let Some(data) = &inner.borrow().data {
                    f(data);
                }
                None
            })
    }

    pub fn connect_error<F: Fn(&E) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let inner = self.inner.clone();
        let query_obj = {inner.borrow().query_obj.clone()};
        query_obj.connect_local("error", false, move |_args| {
            if let Some(error) = &inner.borrow().error {
                f(error);
            }
            None
        })
    }

    pub fn connect_loading<F: Fn(bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let query_obj = {self.inner.borrow().query_obj.clone()};
        query_obj
            .connect_notify_local(Some("is-loading"), move |query_obj, _pspec| {
                let is_loading = query_obj.is_loading();
                f(is_loading);
            })
    }

    /// Bind a widget property to the query inner
    pub fn bind_to_widget<W: IsA<gtk::Widget>>(&self, widget: &W, property: &str) {
        let query_obj = {self.inner.borrow().query_obj.clone()};
        query_obj
            .bind_property(property, widget, property)
            .build();
    }

    pub fn data(&self) -> Option<T> {
        self.inner.borrow().data.clone()
    }

    /// Check if the cached data is stale based on a given max age
    /// Returns true if data has never been fetched or if the duration has elapsed
    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.inner.borrow().is_stale(max_age)
    }

    /// Get the age of the cached data since last successful fetch
    /// Returns None if data has never been fetched
    pub fn age(&self) -> Option<Duration> {
        self.inner.borrow().age()
    }

    /// Get the timestamp of the last successful fetch
    pub fn last_fetched_at(&self) -> Option<SystemTime> {
        self.inner.borrow().last_fetched_at
    }

    /// Refetch only if the data is stale based on the given max age
    /// Returns true if a refetch was triggered, false if data is still fresh
    pub fn refetch_if_stale(&self, max_age: Duration) -> bool {
        if self.is_stale(max_age) {
            self.refetch();
            true
        } else {
            false
        }
    }
}
