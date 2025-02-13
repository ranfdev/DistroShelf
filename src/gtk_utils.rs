use std::collections::HashMap;
use std::hash::Hash;

use gtk::prelude::*;
use gtk::{gio, glib};

pub fn reconcile_properties<T: IsA<glib::Object>>(dest: &T, src: &T, properties: &[&str]) {
    for prop in dest.list_properties() {
        let name = prop.name();
        if !properties.contains(&name) {
            continue;
        }
        let other_prop = src.property::<glib::Value>(name);
        let this_prop = dest.property::<glib::Value>(name);
        // We should check if we can compare the values in other ways
        if this_prop.as_ptr() != other_prop.as_ptr() {
            dest.set_property(name, other_prop);
        }
    }
}
pub fn reconcile_list_by_key<T: IsA<glib::Object>, K: Hash + std::cmp::Eq>(
    list: gio::ListStore,
    other: &[T],
    key_fn: impl Fn(&T) -> K,
    properties: &[&str],
) {
    let mut other_map: HashMap<K, (&T, bool)> =
        other.into_iter().map(|v| (key_fn(v), (v, false))).collect();
    list.retain(|item| {
        let item = item.downcast_ref().unwrap();
        let key = key_fn(item);
        if let Some(other) = other_map.get_mut(&key) {
            reconcile_properties(item, other.0, properties);
            other.1 = true;
            true
        } else {
            false
        }
    });
    for (_key, (val, already_found)) in other_map {
        if !already_found {
            list.append(val);
        }
    }
}
macro_rules! reaction {
    ($obj:ident . $prop:ident(), $closure:expr) => {
        let cloned = $obj.clone();
        let extractor = move || {
            cloned.$prop()
        };
        let closure = {
            let $obj = $obj.clone();
            $closure
        };
        closure(extractor());

        let prop_name = stringify!($prop).replace("_", "-");
        $obj.connect_notify_local(
            Some(&prop_name),
            move |_, _| {
                closure(extractor());
            }
        );
    };
    (($($obj:ident . $prop:ident()),+), $closure:expr) => {
        let extractor = {
            $(let $obj = $obj.clone();)+
            std::rc::Rc::new(move || ($($obj.$prop()),+))
        };
        let shared_closure = {
            $(let $obj = $obj.clone();)+
            std::rc::Rc::new($closure)
        };
        shared_closure.clone()(extractor.clone()());
        $(
            let c = shared_closure.clone();
            let e = extractor.clone();
            let shared_closure_with_extractor = move || {
                c(e())
            };
            let prop_name = stringify!($prop).replace("_", "-");
            $obj.connect_notify_local(
                Some(&prop_name),
                move |_,_| {
                    shared_closure_with_extractor();
                }
            );
        )+
    };
}
pub(crate) use reaction;
