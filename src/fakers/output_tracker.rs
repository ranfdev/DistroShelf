use std::{cell::RefCell, rc::Rc};

#[derive(Default, Clone, Debug)]
pub struct OutputTracker<T> {
    store: Rc<RefCell<Option<Vec<T>>>>,
}

impl<T> OutputTracker<T> {
    pub fn new() -> Self {
        OutputTracker {
            store: Rc::new(RefCell::new(None)),
        }
    }
    pub fn len(&self) -> usize {
        if let Some(v) = &*self.store.borrow() {
            v.len()
        } else {
            0
        }
    }
}

impl<T: Clone + std::fmt::Debug> OutputTracker<T> {
    pub fn enable(&self) {
        let mut inner = self.store.borrow_mut();
        if inner.is_none() {
            *inner = Some(vec![]);
        }
    }
    pub fn push(&self, item: T) {
        if let Some(v) = &mut *self.store.borrow_mut() {
            v.push(item);
        }
    }
    pub fn items(&self) -> Vec<T> {
        if let Some(v) = &*self.store.borrow() {
            v.clone()
        } else {
            vec![]
        }
    }
}