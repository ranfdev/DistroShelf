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

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_tracker_new() {
        let tracker: OutputTracker<String> = OutputTracker::new();
        assert_eq!(tracker.len(), 0);
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_output_tracker_disabled_by_default() {
        let tracker: OutputTracker<String> = OutputTracker::new();

        // Push should be no-op when disabled
        tracker.push("test".to_string());

        assert_eq!(tracker.len(), 0);
        assert!(tracker.items().is_empty());
    }

    #[test]
    fn test_output_tracker_enable() {
        let tracker: OutputTracker<String> = OutputTracker::new();
        tracker.enable();

        // After enabling, push should work
        tracker.push("item1".to_string());
        tracker.push("item2".to_string());

        assert_eq!(tracker.len(), 2);
        assert!(!tracker.is_empty());
    }

    #[test]
    fn test_output_tracker_items() {
        let tracker: OutputTracker<i32> = OutputTracker::new();
        tracker.enable();

        tracker.push(1);
        tracker.push(2);
        tracker.push(3);

        let items = tracker.items();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test]
    fn test_output_tracker_clone() {
        let tracker1: OutputTracker<String> = OutputTracker::new();
        tracker1.enable();
        tracker1.push("before_clone".to_string());

        let tracker2 = tracker1.clone();

        // Clones share the same underlying storage
        tracker2.push("after_clone".to_string());

        assert_eq!(tracker1.len(), 2);
        assert_eq!(tracker2.len(), 2);
        assert_eq!(tracker1.items(), tracker2.items());
    }

    #[test]
    fn test_output_tracker_enable_idempotent() {
        let tracker: OutputTracker<String> = OutputTracker::new();
        tracker.enable();
        tracker.push("item1".to_string());

        // Enable again should not clear existing items
        tracker.enable();
        tracker.push("item2".to_string());

        assert_eq!(tracker.len(), 2);
        assert_eq!(tracker.items(), vec!["item1", "item2"]);
    }

    #[test]
    fn test_output_tracker_with_custom_type() {
        #[derive(Clone, Debug, PartialEq)]
        struct Event {
            id: u32,
            name: String,
        }

        let tracker: OutputTracker<Event> = OutputTracker::new();
        tracker.enable();

        tracker.push(Event {
            id: 1,
            name: "first".to_string(),
        });
        tracker.push(Event {
            id: 2,
            name: "second".to_string(),
        });

        let items = tracker.items();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, 1);
        assert_eq!(items[1].name, "second");
    }

    #[test]
    fn test_output_tracker_default() {
        let tracker: OutputTracker<String> = OutputTracker::default();
        assert_eq!(tracker.len(), 0);
        assert!(tracker.is_empty());
    }
}
