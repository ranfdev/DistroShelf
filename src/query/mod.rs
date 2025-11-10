use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct QueryState<T, E> {
    /// The current data (if any successful fetch has occurred)
    pub data: Option<T>,
    /// Whether a fetch is currently in progress
    pub is_loading: bool,
    /// The last error (if any)
    pub error: Option<Rc<E>>,
}

impl<T, E> QueryState<T, E> {
    pub fn new() -> Self {
        Self {
            data: None,
            is_loading: false,
            error: None,
        }
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
            static SIGNALS: std::sync::OnceLock<Vec<glib::subclass::Signal>> = std::sync::OnceLock::new();
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
    pub query_fn: Rc<dyn Fn() -> F>,
    
    /// Whether to execute immediately or wait for manual trigger
    pub enabled: bool,
    
    /// Refetch interval in seconds (None = no auto-refetch)
    pub refetch_interval: Option<u32>,
}

pub struct Query<T, E> {
    state: Rc<RefCell<QueryState<T, E>>>,
    query_obj: AsyncQuery,
    query_fn: Rc<RefCell<Option<Rc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>>>>>>>>,
    refetch_source_id: Rc<RefCell<Option<glib::SourceId>>>,
    _phantom: std::marker::PhantomData<(T, E)>,
}

impl<T, E> Clone for Query<T, E>
where
    T: Clone + 'static,
    E: std::fmt::Display + 'static,
{
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            query_obj: self.query_obj.clone(),
            query_fn: self.query_fn.clone(),
            refetch_source_id: self.refetch_source_id.clone(),
            _phantom: std::marker::PhantomData,
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
        if let Some(source_id) = self.refetch_source_id.borrow_mut().take() {
            source_id.remove();
        }
    }
}

impl<T, E> Query<T, E>
where
    T: Clone + 'static,
    E: std::fmt::Display + 'static,
{
    pub fn new<F: Future<Output = Result<T, E>> + 'static>(key: String, query_fn: impl Fn() -> F + 'static) -> Self {
        Self::new_with_options(QueryOptions {
            key,
            query_fn: Rc::new(query_fn),
            enabled: false,
            refetch_interval: None,
        })
    }
    pub fn new_with_options<F>(options: QueryOptions<T, E, F>) -> Self
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        let query_obj = glib::Object::new::<AsyncQuery>();
        let state = Rc::new(RefCell::new(QueryState::new()));
        let refetch_source_id: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        let query_fn: Rc<RefCell<Option<Rc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>>>>>>>> = Rc::new(RefCell::new(None));

        let query = Self {
            state: state.clone(),
            query_obj: query_obj.clone(),
            query_fn: query_fn.clone(),
            refetch_source_id: refetch_source_id.clone(),
            _phantom: std::marker::PhantomData,
        };

        if options.enabled {
            query.fetch(options.query_fn.clone());
        }

        // Setup auto-refetch if interval specified
        if let Some(interval) = options.refetch_interval {
            let query_fn = options.query_fn.clone();
            let source_id = glib::timeout_add_seconds_local(interval, move || {
                let state = state.clone();
                let query_obj = query_obj.clone();
                let query_fn = query_fn.clone();
                glib::spawn_future_local(async move {
                    Self::execute_fetch(&state, &query_obj, query_fn).await;
                });
                glib::ControlFlow::Continue
            });
            *refetch_source_id.borrow_mut() = Some(source_id);
        }

        query
    }

    /// Execute a fetch operation and handle the result
    async fn execute_fetch<F>(
        state: &Rc<RefCell<QueryState<T, E>>>,
        query_obj: &AsyncQuery,
        query_fn: Rc<dyn Fn() -> F>,
    )
    where
        F: std::future::Future<Output = Result<T, E>>,
    {
        let future = query_fn();
        let result = future.await;
        
        match result {
            Ok(_data) => {
                state.borrow_mut().data = Some(_data.clone());
                state.borrow_mut().is_loading = false;
                state.borrow_mut().error = None;
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
                state.borrow_mut().is_loading = false;
                state.borrow_mut().error = Some(rc_error);
                query_obj.set_is_loading(false);
                query_obj.set_is_error(true);
                query_obj.set_is_success(state.borrow().data.is_some());
                query_obj.set_error_message(Some(error_msg.clone()));
                
                // Emit error signal with error message
                query_obj.emit_by_name::<()>("error", &[&error_msg]);
            }
        }
    }

    pub fn fetch<F>(&self, query_fn: Rc<dyn Fn() -> F>)
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        // Set loading state, but preserve any previous data
        self.query_obj.set_is_loading(true);
        self.query_obj.set_is_error(false);
        self.query_obj.set_is_success(false);
        self.state.borrow_mut().is_loading = true;
        
        let state = self.state.clone();
        let query_obj = self.query_obj.clone();
        
        // Spawn the async task on GLib main loop
        glib::spawn_future_local(async move {
            Self::execute_fetch(&state, &query_obj, query_fn).await;
        });
    }
    
    
    pub fn refetch(&self) {
        if let Some(query_fn) = self.query_fn.borrow().as_ref() {
            self.fetch(query_fn.clone());
        }
    }
    
    pub fn set_fetcher<F>(&self, query_fn: impl Fn() -> F + 'static)
    where
        F: Future<Output = Result<T, E>> + 'static,
    {
        *self.query_fn.borrow_mut() = Some(Rc::new(move || {
            Box::pin(query_fn())
        }));
    }

    pub fn connect_success<F: Fn(&T) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let state = self.state.clone();
        self.query_obj.connect_local("success", false, move |_args| {
            if let Some(data) = &state.borrow().data {
                f(data);
            }
            None
        })
    }

    pub fn connect_error<F: Fn(&E) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let state = self.state.clone();
        self.query_obj.connect_local("error", false, move |_args| {
            if let Some(error) = &state.borrow().error {
                f(error);
            }
            None
        })
    }

    pub fn connect_loading<F: Fn(bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.query_obj.connect_notify_local(Some("is-loading"), move |query_obj, _pspec| {
            let is_loading = query_obj.is_loading();
            f(is_loading);
        })
    }
    
    /// Bind a widget property to the query state
    pub fn bind_to_widget<W: IsA<gtk::Widget>>(
        &self,
        widget: &W,
        property: &str,
    ) {
        self.query_obj.bind_property(property, widget, property)
            .build();
    }
    
    pub fn data(&self) -> Option<T> {
        self.state.borrow().data.clone()
    }
}
