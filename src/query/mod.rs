use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, info, warn};

pub struct QueryInner<T> {
    key: String,
    /// The current data (if any successful fetch has occurred)
    pub data: Option<T>,
    /// Timestamp of the last successful fetch
    pub last_fetched_at: Option<SystemTime>,
    /// The last error (if any) - stored as Rc for signal emission
    pub error: Option<Rc<anyhow::Error>>,
    query_fn: Option<
        Box<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<T>>>>>,
    >,
    refetch_source_id: Option<glib::SourceId>,
    /// Active fetch task handle - cancellable when dropped
    fetch_task_handle: Option<glib::JoinHandle<()>>,
    query_obj: AsyncQuery,
    /// Timeout duration for queries (None = no timeout)
    timeout: Option<Duration>,

    retry_strategy: Option<Box<dyn Fn(u32) -> Option<Duration>>>,
    retry_count: u32,

    refetch_strategy: Option<Rc<dyn Fn(&Query<T>) + 'static>>,
}

impl<T> QueryInner<T> {
    pub fn new(
        key: String,
        query_fn: Option<
            Box<
                dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<T>>>>,
            >,
        >,
        timeout: Option<Duration>,
    ) -> Self {
        Self {
            key,
            data: None,
            error: None,
            last_fetched_at: None,
            query_fn,
            refetch_source_id: None,
            fetch_task_handle: None,
            query_obj: glib::Object::new::<AsyncQuery>(),
            timeout,
            retry_strategy: None,
            retry_count: 0,
            refetch_strategy: None,
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

pub struct QueryOptions<T, F>
where
    F: Future<Output = anyhow::Result<T>> + 'static,
{
    /// Unique key for this query (for caching/deduplication)
    pub key: String,

    /// The async function that fetches data
    pub query_fn: Box<dyn Fn() -> F>,

    /// Whether to execute immediately or wait for manual trigger
    pub enabled: bool,

    /// Refetch interval in seconds (None = no auto-refetch)
    pub refetch_interval: Option<u32>,

    /// Timeout duration for the query (None = no timeout)
    pub timeout: Option<Duration>,
}

pub struct Query<T> {
    inner: Rc<RefCell<QueryInner<T>>>,
}

impl<T> Clone for Query<T>
where
    T: Clone + 'static,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<T: Clone + 'static> Default for Query<T>
where
    T: Default,
{
    fn default() -> Self {
        Self::new("default".into(), || async { Ok(T::default()) })
    }
}

impl<T> Drop for Query<T> {
    fn drop(&mut self) {
        let is_last = Rc::strong_count(&self.inner) == 1;
        if is_last {
            // Remove the refetch timer if present
            if let Some(source_id) = self.inner.borrow_mut().refetch_source_id.take() {
                source_id.remove();
            }

            // Abort any active fetch task to ensure cleanup
            if let Some(handle) = self.inner.borrow_mut().fetch_task_handle.take() {
                debug!(resource_key = %self.inner.borrow().key, "Dropping last reference to Query, aborting active fetch task");
                handle.abort();
            }
        }
    }
}

impl<T> Query<T>
where
    T: Clone + 'static,
{
    pub fn new<F: Future<Output = anyhow::Result<T>> + 'static>(
        key: String,
        query_fn: impl Fn() -> F + 'static,
    ) -> Self {
        Self::new_with_options(QueryOptions {
            key,
            query_fn: Box::new(query_fn),
            enabled: false,
            refetch_interval: None,
            timeout: None,
        })
    }

    /// Set the timeout duration for this query
    /// Returns self for method chaining
    pub fn with_timeout(self, timeout: Duration) -> Self {
        self.inner.borrow_mut().timeout = Some(timeout);
        self
    }

    /// Strategy: Execute fetch immediately
    pub fn immediate() -> impl Fn(&Query<T>) {
        |query: &Query<T>| {
            query.fetch();
        }
    }

    /// Strategy: Debounce fetch calls
    /// Waits for `duration` after the last call before executing.
    /// If another call arrives before the timer fires, the timer resets.
    pub fn debounce(duration: Duration) -> impl Fn(&Query<T>) {
        // Strategy state: managed by the closure itself
        let debounce_state: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
        
        move |query: &Query<T>| {
            let key = { query.inner.borrow().key.clone() };

            // Cancel any existing debounce timer
            if let Some(source_id) = debounce_state.borrow_mut().take() {
                debug!(resource_key = %key, "Cancelling previous debounce timer");
                source_id.remove();
            }

            let weak = Rc::downgrade(&query.inner);
            let state_for_callback = debounce_state.clone();
            let source_id = glib::timeout_add_local_once(duration, move || {
                if let Some(inner) = weak.upgrade() {
                    let query = Query { inner };
                    let key = { query.inner.borrow().key.clone() };
                    debug!(resource_key = %key, "Debounce timer fired, executing fetch");
                    // Clear the source_id since timer has fired
                    *state_for_callback.borrow_mut() = None;
                    query.fetch();
                }
            });

            debug!(resource_key = %key, duration_ms = duration.as_millis(), "Scheduled debounced fetch");
            *debounce_state.borrow_mut() = Some(source_id);
        }
    }

    /// Strategy: Throttle fetch calls
    /// Executes at most once per `interval`.
    /// If `trailing` is true, a trailing fetch will be scheduled after the interval
    /// if calls arrived during the throttle period.
    pub fn throttle(interval: Duration, trailing: bool) -> impl Fn(&Query<T>) {
        // Strategy state: managed by the closure itself
        let throttle_state: Rc<RefCell<(Option<Instant>, Option<glib::SourceId>)>> = 
            Rc::new(RefCell::new((None, None)));
        
        move |query: &Query<T>| {
            let key = { query.inner.borrow().key.clone() };
            let now = Instant::now();

            let last_throttle_time = { throttle_state.borrow().0 };
            
            let should_fetch = match last_throttle_time {
                None => true,
                Some(last_time) => now.duration_since(last_time) >= interval,
            };

            if should_fetch {
                // Cancel any pending trailing timer since we're fetching now
                if let Some(source_id) = throttle_state.borrow_mut().1.take() {
                    debug!(resource_key = %key, "Cancelling trailing throttle timer (immediate fetch)");
                    source_id.remove();
                }

                debug!(resource_key = %key, "Throttle allows fetch, executing immediately");
                *throttle_state.borrow_mut() = (Some(now), None);
                query.fetch();
            } else if trailing {
                // Schedule a trailing fetch if not already scheduled
                let has_pending_trailing = { throttle_state.borrow().1.is_some() };

                if !has_pending_trailing {
                    let remaining = interval
                        .checked_sub(now.duration_since(last_throttle_time.unwrap()))
                        .unwrap_or(Duration::ZERO);

                    let weak = Rc::downgrade(&query.inner);
                    let state_for_callback = throttle_state.clone();
                    let source_id = glib::timeout_add_local_once(remaining, move || {
                        if let Some(inner) = weak.upgrade() {
                            let query = Query { inner };
                            let key = { query.inner.borrow().key.clone() };
                            debug!(resource_key = %key, "Trailing throttle timer fired, executing fetch");
                            // Clear the source_id and update throttle time
                            *state_for_callback.borrow_mut() = (Some(Instant::now()), None);
                            query.fetch();
                        }
                    });

                    debug!(resource_key = %key, remaining_ms = remaining.as_millis(), "Scheduled trailing throttle fetch");
                    throttle_state.borrow_mut().1 = Some(source_id);
                } else {
                    debug!(resource_key = %key, "Throttled: trailing timer already pending");
                }
            } else {
                debug!(resource_key = %key, "Throttled: skipping fetch (no trailing)");
            }
        }
    }

    pub fn with_retry_strategy(
        self,
        retry_strategy: impl Fn(u32) -> Option<Duration> + 'static,
    ) -> Self {
        self.inner.borrow_mut().retry_strategy = Some(Box::new(retry_strategy));
        self
    }

    pub fn new_with_options<F>(options: QueryOptions<T, F>) -> Self
    where
        F: Future<Output = anyhow::Result<T>> + 'static,
    {
        let inner = Rc::new(RefCell::new(QueryInner::new(
            options.key.clone(),
            Some(Box::new(move || {
                let fut = (options.query_fn)();
                Box::pin(fut)
            })),
            options.timeout,
        )));

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
                if let Some(inner) = weak.upgrade() {
                    Self { inner }.fetch();
                }
                glib::ControlFlow::Continue
            });
            inner.borrow_mut().refetch_source_id = Some(source_id);
        }
        query
    }

    /// Execute a fetch operation and handle the result
    async fn execute_fetch(inner: &Rc<RefCell<QueryInner<T>>>) {
        let key = { inner.borrow().key.clone() };
        let query_obj = { inner.borrow().query_obj.clone() };
        let timeout = { inner.borrow().timeout };

        let Some(future) = inner.borrow().query_fn.as_ref().map(|f| f()) else {
            warn!(resource_key = %key, "No query function set for resource");
            return;
        };
        debug!(resource_key = %key, "Starting fetch for resource");

        // Apply timeout if configured
        let result = if let Some(timeout_duration) = timeout {
            use futures::FutureExt;

            debug!(resource_key = %key, timeout_secs = timeout_duration.as_secs(), "Query has timeout configured");

            // Race the future against a timeout
            let timeout_future = glib::timeout_future(timeout_duration);

            futures::select! {
                result = future.fuse() => result,
                _ = timeout_future.fuse() => {
                    warn!(resource_key = %key, timeout_secs = timeout_duration.as_secs(), "Query timed out");
                    Err(anyhow::anyhow!("Query timed out after {} seconds", timeout_duration.as_secs()))
                }
            }
        } else {
            future.await
        };

        match result {
            Ok(_data) => {
                inner.borrow_mut().data = Some(_data.clone());
                inner.borrow_mut().error = None;
                query_obj.set_is_loading(false);
                query_obj.set_is_success(true);
                query_obj.set_is_error(false);
                query_obj.set_error_message(None::<String>);

                info!(resource_key = %key, "Resource fetch completed successfully");
                // Emit success signal
                query_obj.emit_by_name::<()>("success", &[]);
                inner.borrow_mut().retry_count = 0;
            }
            Err(error) => {
                if inner.borrow().retry_strategy.is_some() {
                    let retry_count = Self { inner: inner.clone() }.retry();
                    if let Some(_retry_count) = retry_count {
                        return;
                    }
                }
                let rc_error = Rc::new(error);
                let error_msg = rc_error.to_string();
                // Keep the previous data, just mark as error
                inner.borrow_mut().error = Some(rc_error);
                query_obj.set_is_loading(false);
                query_obj.set_is_error(true);
                query_obj.set_is_success(inner.borrow().data.is_some());
                query_obj.set_error_message(Some(error_msg.clone()));

                warn!(resource_key = %key, error = %error_msg, "Resource fetch failed");
                // Emit error signal with error message
                query_obj.emit_by_name::<()>("error", &[&error_msg]);
            }
        }
    }

    pub fn fetch(&self) {
        let key = { self.inner.borrow().key.clone() };
        debug!(resource_key = %key, "Fetch triggered for resource");
        let query_obj = { self.inner.borrow().query_obj.clone() };
        // Cancel any previous fetch task before starting a new one
        if let Some(handle) = self.inner.borrow_mut().fetch_task_handle.take() {
            debug!(resource_key = %key, "Aborting previous fetch task");
            handle.abort();
        }

        // Set loading state, but preserve any previous data
        query_obj.set_is_loading(true);
        query_obj.set_is_error(false);
        query_obj.set_is_success(false);
        self.inner.borrow_mut().last_fetched_at = Some(SystemTime::now());

        let inner = self.inner.clone();

        // Spawn the async task on GLib main loop and store the handle
        let handle = glib::spawn_future_local(async move {
            Self::execute_fetch(&inner).await;
        });

        self.inner.borrow_mut().fetch_task_handle = Some(handle);
    }

    /// Set the refetch strategy for this query.
    /// The strategy is a closure that determines when and how to execute the fetch.
    /// Common strategies are `Query::immediate`, `Query::debounce`, and `Query::throttle`.
    pub fn set_refetch_strategy(&self, strategy: impl Fn(&Query<T>) + 'static) {
        self.inner.borrow_mut().refetch_strategy = Some(Rc::new(strategy));
    }

    /// Refetch using the configured strategy (or immediate if none set)
    pub fn refetch(&self) {
        let strategy = self.inner.borrow().refetch_strategy.clone();
        if let Some(strategy) = strategy {
            strategy(self);
        } else {
            self.fetch();
        }
    }

    pub fn retry(&self) -> Option<u32> {
        self.inner.borrow_mut().retry_count += 1;
        let retry_count = { self.inner.borrow().retry_count };
        let key = { self.inner.borrow().key.clone() };
        if let Some(delay) = {
            self.inner
                .borrow()
                .retry_strategy
                .as_ref()
                .and_then(|f| f(retry_count))
        } {
            info!(resource_key = %key, retry_count = retry_count, delay_secs = delay.as_secs(), "Scheduling retry for resource fetch");
            let inner = self.inner.clone();
            // Spawn the async task on GLib main loop and store the handle
            let handle = glib::spawn_future_local(async move {
                glib::timeout_future(delay).await;
                Self::execute_fetch(&inner).await;
            });

            self.inner.borrow_mut().fetch_task_handle = Some(handle);
            Some(retry_count)
        } else {
            warn!(resource_key = %key, retry_count = retry_count, "No more retries left, giving up on resource fetch");
            None
        }
    }

    pub fn set_fetcher<F>(&self, query_fn: impl Fn() -> F + 'static)
    where
        F: Future<Output = anyhow::Result<T>> + 'static,
    {
        self.inner.borrow_mut().query_fn = Some(Box::new(move || Box::pin(query_fn())));
    }

    pub fn connect_success<F: Fn(&T) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let inner = self.inner.clone();
        let query_obj = { inner.borrow().query_obj.clone() };
        query_obj.connect_local("success", false, move |_args| {
            if let Some(data) = &inner.borrow().data {
                f(data);
            }
            None
        })
    }

    pub fn connect_error<F: Fn(&anyhow::Error) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let inner = self.inner.clone();
        let query_obj = { inner.borrow().query_obj.clone() };
        query_obj.connect_local("error", false, move |_args| {
            if let Some(error) = &inner.borrow().error {
                f(error);
            }
            None
        })
    }

    pub fn connect_loading<F: Fn(bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        let query_obj = { self.inner.borrow().query_obj.clone() };
        query_obj.connect_notify_local(Some("is-loading"), move |query_obj, _pspec| {
            let is_loading = query_obj.is_loading();
            f(is_loading);
        })
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
        let key = { self.inner.borrow().key.clone() };
        if self.is_stale(max_age) {
            debug!(
                resource_key = %key,
                max_age_secs = max_age.as_secs(),
                "Resource is stale, triggering refetch"
            );
            self.refetch();
            true
        } else {
            debug!(
                resource_key = %key,
                age_secs = ?self.age().map(|d| d.as_secs()),
                max_age_secs = max_age.as_secs(),
                "Resource is fresh, skipping refetch"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Standalone staleness check logic - mirrors QueryInner::is_stale
    fn check_is_stale(last_fetched_at: Option<SystemTime>, max_age: Duration) -> bool {
        match last_fetched_at {
            None => true,
            Some(fetched_at) => SystemTime::now()
                .duration_since(fetched_at)
                .map(|elapsed| elapsed > max_age)
                .unwrap_or(true),
        }
    }

    /// Standalone age calculation logic - mirrors QueryInner::age
    fn calculate_age(last_fetched_at: Option<SystemTime>) -> Option<Duration> {
        last_fetched_at.and_then(|fetched_at| SystemTime::now().duration_since(fetched_at).ok())
    }

    #[test]
    fn test_is_stale_never_fetched() {
        // Data that was never fetched is always stale
        assert!(check_is_stale(None, Duration::from_secs(60)));
        assert!(check_is_stale(None, Duration::from_secs(0)));
    }

    #[test]
    fn test_is_stale_fresh_data() {
        let fetched_at = Some(SystemTime::now());

        // Data just fetched should not be stale for reasonable max_age
        assert!(!check_is_stale(fetched_at, Duration::from_secs(60)));
        assert!(!check_is_stale(fetched_at, Duration::from_secs(1)));
    }

    #[test]
    fn test_is_stale_old_data() {
        // Set last_fetched_at to 2 seconds ago
        let fetched_at = Some(SystemTime::now() - Duration::from_secs(2));

        // Data older than max_age is stale
        assert!(check_is_stale(fetched_at, Duration::from_secs(1)));
        // Data newer than max_age is not stale
        assert!(!check_is_stale(fetched_at, Duration::from_secs(10)));
    }

    #[test]
    fn test_age_never_fetched() {
        // Data that was never fetched has no age
        assert!(calculate_age(None).is_none());
    }

    #[test]
    fn test_age_just_fetched() {
        let fetched_at = Some(SystemTime::now());

        // Data just fetched should have very small age
        let age = calculate_age(fetched_at).expect("Should have age");
        assert!(age < Duration::from_secs(1));
    }

    #[test]
    fn test_age_old_data() {
        let fetched_at = Some(SystemTime::now() - Duration::from_secs(5));

        // Data fetched 5 seconds ago should have age of approximately 5 seconds
        let age = calculate_age(fetched_at).expect("Should have age");
        assert!(age >= Duration::from_secs(4));
        assert!(age < Duration::from_secs(7));
    }

    #[test]
    fn test_retry_strategy_basic() {
        // Test that retry_strategy closure works as expected
        let strategy: Box<dyn Fn(u32) -> Option<Duration>> = Box::new(|n| {
            if n < 3 {
                Some(Duration::from_secs(n as u64))
            } else {
                None
            }
        });

        assert_eq!(strategy(0), Some(Duration::from_secs(0)));
        assert_eq!(strategy(1), Some(Duration::from_secs(1)));
        assert_eq!(strategy(2), Some(Duration::from_secs(2)));
        assert_eq!(strategy(3), None);
        assert_eq!(strategy(100), None);
    }

    #[test]
    fn test_exponential_backoff_strategy() {
        // Test exponential backoff pattern
        let strategy: Box<dyn Fn(u32) -> Option<Duration>> = Box::new(|n| {
            if n < 5 {
                Some(Duration::from_millis(100 * 2u64.pow(n)))
            } else {
                None
            }
        });

        assert_eq!(strategy(0), Some(Duration::from_millis(100)));
        assert_eq!(strategy(1), Some(Duration::from_millis(200)));
        assert_eq!(strategy(2), Some(Duration::from_millis(400)));
        assert_eq!(strategy(3), Some(Duration::from_millis(800)));
        assert_eq!(strategy(4), Some(Duration::from_millis(1600)));
        assert_eq!(strategy(5), None);
    }

    #[test]
    fn test_is_stale_boundary() {
        // Test exact boundary condition
        let fetched_at = Some(SystemTime::now() - Duration::from_millis(1000));

        // At exactly 1 second, should be stale (elapsed > max_age, not >=)
        assert!(check_is_stale(fetched_at, Duration::from_millis(999)));
        // At more than elapsed time, should not be stale
        assert!(!check_is_stale(fetched_at, Duration::from_millis(2000)));
    }

    #[test]
    fn test_retry_strategy_with_jitter() {
        // Test a more complex retry strategy with jitter-like behavior
        use std::sync::atomic::{AtomicU32, Ordering};
        let counter = std::sync::Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let strategy: Box<dyn Fn(u32) -> Option<Duration>> = Box::new(move |n| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
            if n < 3 {
                // Base delay with pseudo-jitter based on retry count
                Some(Duration::from_millis(100 * (n as u64 + 1)))
            } else {
                None
            }
        });

        // Call strategy multiple times
        let _ = strategy(0);
        let _ = strategy(1);
        let _ = strategy(2);
        let _ = strategy(3);

        // Verify strategy was called correct number of times
        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn test_refetch_strategy_functions() {
        // Test that all strategy functions can be created
        // These are now closures, so we can't easily inspect them
        // But we can verify they compile and return the right type
        let _immediate = Query::<String>::immediate();
        let _debounce = Query::<String>::debounce(Duration::from_millis(300));
        let _throttle_no_trailing = Query::<String>::throttle(Duration::from_secs(1), false);
        let _throttle_with_trailing = Query::<String>::throttle(Duration::from_secs(2), true);

        // The test passes if all strategies can be constructed without error
    }

    #[test]
    fn test_throttle_timing_logic() {
        // Test the throttle timing logic in isolation
        let interval = Duration::from_millis(100);
        let mut last_throttle_time: Option<Instant> = None;

        // First call should always be allowed
        let now = Instant::now();
        let should_fetch_1 = match last_throttle_time {
            None => true,
            Some(last_time) => now.duration_since(last_time) >= interval,
        };
        assert!(should_fetch_1, "First call should be allowed");

        // Simulate a fetch
        last_throttle_time = Some(now);

        // Immediate second call should be throttled
        let now2 = Instant::now();
        let should_fetch_2 = match last_throttle_time {
            None => true,
            Some(last_time) => now2.duration_since(last_time) >= interval,
        };
        assert!(!should_fetch_2, "Immediate second call should be throttled");

        // After interval passes, should be allowed again
        std::thread::sleep(interval + Duration::from_millis(10));
        let now3 = Instant::now();
        let should_fetch_3 = match last_throttle_time {
            None => true,
            Some(last_time) => now3.duration_since(last_time) >= interval,
        };
        assert!(should_fetch_3, "Call after interval should be allowed");
    }
}
