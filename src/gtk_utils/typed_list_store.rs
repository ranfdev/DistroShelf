use gtk::{gio, glib, prelude::*};
use std::marker::PhantomData;

/// A type-safe wrapper around `gio::ListStore` that provides ergonomic access
/// to items without manual downcasting.
///
/// # Example
/// ```ignore
/// use gtk_utils::TypedListStore;
///
/// let store = TypedListStore::<Container>::new();
/// store.append(&container);
///
/// // Iterate with type safety
/// for container in store.iter() {
///     println!("{}", container.name());
/// }
///
/// // Get item by index without manual downcasting
/// if let Some(first) = store.get(0) {
///     println!("First: {}", first.name());
/// }
/// ```
#[derive(Clone, Debug)]
pub struct TypedListStore<T: IsA<glib::Object>> {
    inner: gio::ListStore,
    _phantom: PhantomData<T>,
}

impl<T: IsA<glib::Object>> TypedListStore<T> {
    /// Creates a new `TypedListStore` for the given type.
    pub fn new() -> Self {
        Self {
            inner: gio::ListStore::new::<T>(),
            _phantom: PhantomData,
        }
    }

    /// Creates a `TypedListStore` from an existing `gio::ListStore`.
    ///
    /// # Safety
    /// The caller must ensure that the `ListStore` only contains items of type `T`.
    pub fn from_list_store(list_store: gio::ListStore) -> Self {
        Self {
            inner: list_store,
            _phantom: PhantomData,
        }
    }

    /// Returns a reference to the underlying `gio::ListStore`.
    pub fn inner(&self) -> &gio::ListStore {
        &self.inner
    }

    /// Consumes this wrapper and returns the underlying `gio::ListStore`.
    pub fn into_inner(self) -> gio::ListStore {
        self.inner
    }

    /// Appends an item to the store.
    pub fn append(&self, item: &T) {
        self.inner.append(item);
    }

    /// Inserts an item at the given position.
    pub fn insert(&self, position: u32, item: &T) {
        self.inner.insert(position, item);
    }

    /// Removes the item at the given position.
    pub fn remove(&self, position: u32) {
        self.inner.remove(position);
    }

    /// Removes all items from the store.
    pub fn remove_all(&self) {
        self.inner.remove_all();
    }

    /// Returns the number of items in the store.
    pub fn len(&self) -> u32 {
        self.inner.n_items()
    }

    /// Returns `true` if the store contains no items.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the item at the given position, or `None` if out of bounds.
    /// The item is automatically downcast to the correct type.
    pub fn get(&self, position: u32) -> Option<T> {
        self.inner
            .item(position)
            .and_then(|obj| obj.downcast::<T>().ok())
    }

    /// Returns the position of the first item equal to the given item,
    /// or `None` if no such item exists.
    pub fn find(&self, item: &T) -> Option<u32> {
        self.inner.find(item)
    }

    /// Appends all items from an iterator to the store.
    pub fn extend<I>(&self, items: I)
    where
        I: IntoIterator<Item = T>,
    {
        for item in items {
            self.append(&item);
        }
    }

    /// Returns an iterator over all items in the store.
    pub fn iter(&self) -> TypedListStoreIter<T> {
        TypedListStoreIter {
            store: self.clone(),
            position: 0,
            len: self.len(),
        }
    }

    /// Retains only the items for which the predicate returns `true`.
    pub fn retain<F>(&self, mut predicate: F)
    where
        F: FnMut(&T) -> bool,
    {
        let mut i = self.len();
        while i > 0 {
            i -= 1;
            if let Some(item) = self.get(i)
                && !predicate(&item)
            {
                self.remove(i);
            }
        }
    }

    /// Sorts the items in the store using the given comparison function.
    pub fn sort<F>(&self, compare: F)
    where
        F: Fn(&T, &T) -> std::cmp::Ordering + 'static,
    {
        self.inner.sort(move |a, b| {
            let a = a.downcast_ref::<T>().unwrap();
            let b = b.downcast_ref::<T>().unwrap();
            compare(a, b)
        });
    }

    /// Searches for the first item matching the predicate.
    pub fn find_with<F>(&self, mut predicate: F) -> Option<(u32, T)>
    where
        F: FnMut(&T) -> bool,
    {
        for i in 0..self.len() {
            if let Some(item) = self.get(i)
                && predicate(&item)
            {
                return Some((i, item));
            }
        }
        None
    }

    /// Returns the first item in the store, or `None` if the store is empty.
    pub fn first(&self) -> Option<T> {
        self.get(0)
    }

    /// Returns the last item in the store, or `None` if the store is empty.
    pub fn last(&self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            self.get(self.len() - 1)
        }
    }

    /// Replaces the item at the given position.
    /// Returns the old item if it existed.
    pub fn replace(&self, position: u32, item: &T) -> Option<T> {
        if position >= self.len() {
            return None;
        }
        let old = self.get(position);
        self.remove(position);
        self.insert(position, item);
        old
    }

    /// Creates a new `TypedListStore` with items from an iterator.
    pub fn from_iter<I>(items: I) -> Self
    where
        I: IntoIterator<Item = T>,
    {
        let store = Self::new();
        store.extend(items);
        store
    }
}

impl<T: IsA<glib::Object>> Default for TypedListStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: IsA<glib::Object>> From<gio::ListStore> for TypedListStore<T> {
    fn from(list_store: gio::ListStore) -> Self {
        Self::from_list_store(list_store)
    }
}

impl<T: IsA<glib::Object>> From<TypedListStore<T>> for gio::ListStore {
    fn from(typed_store: TypedListStore<T>) -> Self {
        typed_store.into_inner()
    }
}

impl<T: IsA<glib::Object>> AsRef<gio::ListStore> for TypedListStore<T> {
    fn as_ref(&self) -> &gio::ListStore {
        &self.inner
    }
}

// Allow using TypedListStore anywhere a gio::ListModel is expected
impl<T: IsA<glib::Object>> AsRef<gio::ListModel> for TypedListStore<T> {
    fn as_ref(&self) -> &gio::ListModel {
        self.inner.upcast_ref()
    }
}

/// Iterator over items in a `TypedListStore`.
pub struct TypedListStoreIter<T: IsA<glib::Object>> {
    store: TypedListStore<T>,
    position: u32,
    len: u32,
}

impl<T: IsA<glib::Object>> Iterator for TypedListStoreIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position >= self.len {
            return None;
        }
        let item = self.store.get(self.position);
        self.position += 1;
        item
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.len - self.position) as usize;
        (remaining, Some(remaining))
    }
}

impl<T: IsA<glib::Object>> ExactSizeIterator for TypedListStoreIter<T> {
    fn len(&self) -> usize {
        (self.len - self.position) as usize
    }
}

impl<T: IsA<glib::Object>> DoubleEndedIterator for TypedListStoreIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.position >= self.len {
            return None;
        }
        self.len -= 1;
        self.store.get(self.len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gtk::test]
    fn test_typed_list_store() {
        let store = TypedListStore::<gtk::StringObject>::new();
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());

        let item1 = gtk::StringObject::new("Item 1");
        let item2 = gtk::StringObject::new("Item 2");
        let item3 = gtk::StringObject::new("Item 3");

        store.append(&item1);
        store.append(&item2);
        store.append(&item3);

        assert_eq!(store.len(), 3);
        assert!(!store.is_empty());

        let first = store.first().unwrap();
        assert_eq!(first.string(), "Item 1");

        let last = store.last().unwrap();
        assert_eq!(last.string(), "Item 3");

        let items: Vec<_> = store.iter().collect();
        assert_eq!(items.len(), 3);
        assert_eq!(items[1].string(), "Item 2");
    }

    #[gtk::test]
    fn test_retain() {
        let store = TypedListStore::<gtk::StringObject>::new();
        store.append(&gtk::StringObject::new("keep1"));
        store.append(&gtk::StringObject::new("remove"));
        store.append(&gtk::StringObject::new("keep2"));

        store.retain(|item| item.string().starts_with("keep"));

        assert_eq!(store.len(), 2);
        assert_eq!(store.get(0).unwrap().string(), "keep1");
        assert_eq!(store.get(1).unwrap().string(), "keep2");
    }

    #[gtk::test]
    fn test_find_with() {
        let store = TypedListStore::<gtk::StringObject>::new();
        store.append(&gtk::StringObject::new("first"));
        store.append(&gtk::StringObject::new("second"));
        store.append(&gtk::StringObject::new("third"));

        let (pos, item) = store
            .find_with(|item| item.string().contains("sec"))
            .unwrap();

        assert_eq!(pos, 1);
        assert_eq!(item.string(), "second");
    }
}
