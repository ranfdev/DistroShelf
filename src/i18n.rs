// i18n.rs - Helper module for internationalization
//
// This module re-exports the gettext functions used throughout the application.
// All user-visible strings should use the gettext() function for translation.

pub use gettextrs::gettext;

/// Translate a string with formatting arguments.
/// Use this macro like: `gettext_f("Hello, {}!", &[("name", name)])`
#[macro_export]
macro_rules! gettext_f {
    ($msg:expr, $($key:expr => $val:expr),+ $(,)?) => {{
        let mut s = $crate::i18n::gettext($msg);
        $(
            s = s.replace(concat!("{", $key, "}"), &$val.to_string());
        )+
        s
    }};
}
