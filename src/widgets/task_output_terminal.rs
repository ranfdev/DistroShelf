use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use gtk::{self, gio};
use std::cell::RefCell;
use vte4::prelude::*;

use crate::gtk_utils::ColorPalette;
use crate::i18n::gettext;

mod imp {
    #![allow(unreachable_code)]
    use super::*;
    use gtk::glib::Properties;

    #[derive(Properties)]
    #[properties(wrapper_type = super::TaskOutputTerminal)]
    pub struct TaskOutputTerminal {
        pub terminal: vte4::Terminal,
        pub output_buffer: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TaskOutputTerminal {
        const NAME: &'static str = "TaskOutputTerminal";
        type Type = super::TaskOutputTerminal;
        type ParentType = adw::Bin;

        fn new() -> Self {
            Self {
                terminal: vte4::Terminal::new(),
                output_buffer: RefCell::new(String::new()),
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for TaskOutputTerminal {}

    impl WidgetImpl for TaskOutputTerminal {}
    impl BinImpl for TaskOutputTerminal {}
}

glib::wrapper! {
    pub struct TaskOutputTerminal(ObjectSubclass<imp::TaskOutputTerminal>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TaskOutputTerminal {
    pub fn new() -> Self {
        let obj: Self = glib::Object::new();
        obj.build_ui();
        obj
    }

    fn build_ui(&self) {
        let imp = self.imp();
        let terminal = &imp.terminal;

        // Configure the terminal for output display (read-only)
        terminal.set_scroll_on_output(true);
        terminal.set_scroll_on_keystroke(false);
        terminal.set_input_enabled(false);

        // Apply CSS styling: padding, rounded corners, and border
        terminal.add_css_class("task-output-terminal");

        // Apply the current theme's color palette
        let palette = ColorPalette::current();
        palette.apply_to_terminal(terminal);

        // Create context menu actions for copy/paste
        let action_group = gio::SimpleActionGroup::new();

        let copy_action = gio::SimpleAction::new("copy", None);
        copy_action.connect_activate(glib::clone!(
            #[weak]
            terminal,
            move |_, _| {
                terminal.copy_clipboard_format(vte4::Format::Text);
            }
        ));
        action_group.add_action(&copy_action);

        let paste_action = gio::SimpleAction::new("paste", None);
        paste_action.connect_activate(glib::clone!(
            #[weak]
            terminal,
            move |_, _| {
                terminal.paste_clipboard();
            }
        ));
        action_group.add_action(&paste_action);

        terminal.insert_action_group("terminal", Some(&action_group));

        // Create context menu
        let menu_model = gio::Menu::new();
        menu_model.append(Some(&gettext("Copy")), Some("terminal.copy"));
        menu_model.append(Some(&gettext("Paste")), Some("terminal.paste"));

        terminal.set_context_menu_model(Some(&menu_model));

        self.set_child(Some(terminal));
    }

    /// Write a line to the terminal with proper ANSI code handling
    pub fn write_line(&self, line: &str) {
        self.write_output(line);
        self.write_output("\r\n");
    }

    /// Write output directly to the terminal
    pub fn write_output(&self, output: &str) {
        let imp = self.imp();
        let terminal = &imp.terminal;

        // Write directly to the terminal - VTE handles ANSI codes automatically
        terminal.feed(output.as_bytes());
    }

    /// Write buffer content to the terminal (for restoring historical output)
    /// Converts LF line endings to CRLF for proper terminal display
    pub fn write_buffer(&self, text: &str) {
        // Convert \n to \r\n for proper terminal display
        let text_with_cr = text.replace("\r\n", "\n").replace("\n", "\r\n");
        self.write_output(&text_with_cr);
    }

    /// Clear the terminal output
    pub fn clear(&self) {
        let imp = self.imp();
        let terminal = &imp.terminal;
        terminal.reset(true, true);
        *imp.output_buffer.borrow_mut() = String::new();
    }

    /// Get the terminal widget for advanced operations
    pub fn terminal(&self) -> &vte4::Terminal {
        &self.imp().terminal
    }
}

impl Default for TaskOutputTerminal {
    fn default() -> Self {
        Self::new()
    }
}
