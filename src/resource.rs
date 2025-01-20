use std::{cell::RefCell, future::Future, rc::Rc};

use gtk::glib::{self, JoinHandle};

#[derive(Debug, Default)]
pub enum Resource<T, E> {
    #[default]
    Unitialized,
    Loading(Option<T>),
    Error(Rc<E>, Option<T>),
    Loaded(T),
}

impl<T, E> Resource<T, E> {
    pub fn is_unitialized(&self) -> bool {
        matches!(self, Resource::Unitialized)
    }
    pub fn is_loading(&self) -> bool {
        matches!(self, Resource::Loading(_))
    }
    pub fn is_loaded(&self) -> bool {
        matches!(self, Resource::Loaded(_))
    }
    pub fn is_error(&self) -> bool {
        matches!(self, Resource::Error(_, _))
    }

    pub fn data(&self) -> Option<&T> {
        match self {
            Resource::Loaded(data)
            | Resource::Loading(Some(data))
            | Resource::Error(_, Some(data)) => Some(data),
            _ => None,
        }
    }

    pub fn error(&self) -> Option<&E> {
        if let Resource::Error(err, _) = self {
            Some(err)
        } else {
            None
        }
    }

    pub fn result(&self) -> Result<Option<&T>, &E> {
        match self {
            Resource::Loaded(data) => Ok(Some(data)),
            Resource::Loading(data) => Ok(data.as_ref()),
            Resource::Error(err, _) => Err(err),
            Resource::Unitialized => Ok(None),
        }
    }
}

impl<T, E> From<Result<T, E>> for Resource<T, E> {
    fn from(value: Result<T, E>) -> Self {
        match value {
            Ok(v) => Resource::Loaded(v),
            Err(e) => Resource::Error(Rc::new(e), None),
        }
    }
}

impl<T: Clone, E> Clone for Resource<T, E> {
    fn clone(&self) -> Self {
        match self {
            Resource::Unitialized => Resource::Unitialized,
            Resource::Loading(data) => Resource::Loading(data.clone()),
            Resource::Error(err, data) => Resource::Error(err.clone(), data.clone()),
            Resource::Loaded(data) => Resource::Loaded(data.clone()),
        }
    }
}

struct SharedResourceInner<T, E> {
    resource: Resource<T, E>,
    handle: Option<JoinHandle<()>>,
    callback: Option<Box<dyn Fn(Resource<T, E>)>>,
}
pub struct SharedResource<T, E> {
    inner: Rc<RefCell<SharedResourceInner<T, E>>>,
}

impl<T: Clone + 'static, E: 'static> Default for SharedResource<T, E> {
    fn default() -> Self {
        Self::new(|_| {})
    }
}

impl<T: Clone + 'static, E: 'static> SharedResource<T, E> {
    // creates a new SharedResource with an unitialized Resource

    pub fn new(callback: impl Fn(Resource<T, E>) + 'static) -> Self {
        let inner = SharedResourceInner {
            resource: Resource::Unitialized,
            handle: None,
            callback: Some(Box::new(callback)),
        };
        Self {
            inner: Rc::new(RefCell::new(inner)),
        }
    }

    pub fn set_callback(&self, callback: impl Fn(Resource<T, E>) + 'static) {
        self.inner.borrow_mut().callback = Some(Box::new(callback));
    }

    // spawns a future returning a result and calls the callback with the resource
    pub fn load<F: Future<Output = Result<T, E>> + 'static>(&self, future: F) {
        let inner_clone = self.inner.clone();
        {
            self.inner.borrow_mut().resource = Resource::Loading(None);
        }
        {
            let inner = inner_clone.borrow();
            if let Some(ref cb) = inner.callback {
                cb(inner.resource.clone());
            }
        }
        let handle = glib::MainContext::ref_thread_default().spawn_local(async move {
            let result = future.await;
            {
                let mut inner = inner_clone.borrow_mut();
                inner.resource = match result {
                    Ok(data) => Resource::Loaded(data),
                    Err(err) => Resource::Error(Rc::new(err), None),
                };
            }
            let inner = inner_clone.borrow();

            if let Some(ref cb) = inner.callback {
                cb(inner.resource.clone());
            }
        });
        self.inner.borrow_mut().handle = Some(handle);
    }

    pub fn resource(&self) -> Resource<T, E> {
        self.inner.borrow().resource.clone()
    }
}
