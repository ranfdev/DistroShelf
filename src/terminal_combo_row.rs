// Generated by RustGObjectGenerator
// This file is licensed under the same terms as the project it belongs to

use crate::root_store::RootStore;
use crate::{supported_terminals, supported_terminals::SUPPORTED_TERMINALS};
use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::clone;
use glib::subclass::Signal;
use glib::Properties;
use gtk::glib;
use std::cell::RefCell;
use std::sync::OnceLock;

mod imp {
    use super::*;

    #[derive(Properties, Default)]
    #[properties(wrapper_type = super::TerminalComboRow)]
    pub struct TerminalComboRow {
        #[property(get, set)]
        root_store: RefCell<RootStore>,
    }

    #[glib::derived_properties]
    impl ObjectImpl for TerminalComboRow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            obj.set_title("Preferred Terminal");
            obj.set_use_subtitle(true);

            let terminals = SUPPORTED_TERMINALS
                .iter()
                .map(|x| x.name.as_ref())
                .collect::<Vec<&str>>();
            let selected_position = terminals.iter().position(|x| {
                Some(x)
                    == obj
                        .root_store()
                        .selected_terminal()
                        .as_ref()
                        .map(|x| x.name.as_str())
                        .as_ref()
            });

            let terminal_list = gtk::StringList::new(&terminals);
            obj.set_model(Some(&terminal_list));
            if let Some(selected_position) = selected_position {
                obj.set_selected(selected_position as u32);
            }
            obj.connect_selected_item_notify(clone!(
                #[weak]
                obj,
                move |combo| {
                    let selected: gtk::StringObject = combo.selected_item().and_downcast().unwrap();
                    supported_terminals::terminal_by_name(&selected.string())
                        .map(|x| obj.root_store().set_selected_terminal_program(&x.program));
                }
            ));
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // No signals defined
                ]
            })
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TerminalComboRow {
        const NAME: &'static str = "TerminalComboRow";
        type Type = super::TerminalComboRow;
        type ParentType = adw::ComboRow;
    }

    impl WidgetImpl for TerminalComboRow {
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.parent_snapshot(snapshot);
        }
    }

    // Generate Impl blocks for each parent class
    impl ComboRowImpl for TerminalComboRow {
        // Default implementations that forward to parent
    }

    impl ActionRowImpl for TerminalComboRow {
        // Default implementations that forward to parent
    }

    impl PreferencesRowImpl for TerminalComboRow {
        // Default implementations that forward to parent
    }

    impl ListBoxRowImpl for TerminalComboRow {
        // Default implementations that forward to parent
    }

    impl TerminalComboRow {
        // No template callbacks defined
    }
}

glib::wrapper! {
    pub struct TerminalComboRow(ObjectSubclass<imp::TerminalComboRow>)
        @extends adw::ComboRow, adw::ActionRow, adw::PreferencesRow, gtk::ListBoxRow, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TerminalComboRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn new_with_params(root_store: RootStore) -> Self {
        glib::Object::builder()
            .property("root-store", root_store)
            .build()
    }
}
