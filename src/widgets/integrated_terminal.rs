use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{
    self, gio,
    glib::{self, clone},
};
use vte4::prelude::*;

use crate::{fakers::Command, models::Container};

mod imp {
    use std::cell::OnceCell;

    use gtk::glib::{Properties, derived_properties};

    use super::*;

    // Object holding the state
    #[derive(Default, Properties)]
    #[properties(wrapper_type=super::IntegratedTerminal)]
    pub struct IntegratedTerminal {
        #[property(get, set, construct)]
        pub container: OnceCell<Container>,
        pub terminal: vte4::Terminal,
        pub reload_button: gtk::Button,
        pub terminal_pid: std::cell::Cell<Option<glib::Pid>>,
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for IntegratedTerminal {
        const NAME: &'static str = "IntegratedTerminal";
        type Type = super::IntegratedTerminal;
        type ParentType = adw::Bin;

        fn new() -> Self {
            Self {
                container: OnceCell::new(),
                terminal: vte4::Terminal::new(),
                reload_button: gtk::Button::new(),
                terminal_pid: std::cell::Cell::new(None),
            }
        }
    }

    #[derived_properties]
    impl ObjectImpl for IntegratedTerminal {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for IntegratedTerminal {}
    impl BinImpl for IntegratedTerminal {}
}

glib::wrapper! {
    pub struct IntegratedTerminal(ObjectSubclass<imp::IntegratedTerminal>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl IntegratedTerminal {
    pub fn new(container: &Container) -> Self {
        let obj: Self = glib::Object::builder()
            .property("container", container)
            .build();
        obj.build_ui();
        obj
    }

    fn build_ui(&self) {
        let imp = self.imp();
        let terminal = &imp.terminal;
        let reload_button = &imp.reload_button;

        // Create context menu actions
        let action_group = gio::SimpleActionGroup::new();

        let copy_action = gio::SimpleAction::new("copy", None);
        copy_action.connect_activate(clone!(
            #[weak]
            terminal,
            move |_, _| {
                terminal.copy_clipboard_format(vte4::Format::Text);
            }
        ));
        action_group.add_action(&copy_action);

        let paste_action = gio::SimpleAction::new("paste", None);
        paste_action.connect_activate(clone!(
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
        menu_model.append(Some("Copy"), Some("terminal.copy"));
        menu_model.append(Some("Paste"), Some("terminal.paste"));

        terminal.set_context_menu_model(Some(&menu_model));

        // Create a container for the terminal with a reload button overlay
        let terminal_overlay = gtk::Overlay::new();
        terminal_overlay.set_child(Some(terminal));

        reload_button.set_icon_name("view-refresh-symbolic");
        reload_button.set_tooltip_text(Some("Reload Terminal"));
        reload_button.add_css_class("circular");
        reload_button.add_css_class("suggested-action");
        reload_button.set_halign(gtk::Align::Center);
        reload_button.set_valign(gtk::Align::Center);
        reload_button.set_visible(true);
        terminal_overlay.add_overlay(reload_button);

        // Connect to terminal child-exited signal to show reload button
        terminal.connect_child_exited(clone!(
            #[weak(rename_to=this)]
            self,
            move |_, _status| {
                this.imp().terminal_pid.set(None);
                this.imp().reload_button.set_visible(true);
            }
        ));

        // Reload button click handler
        reload_button.connect_clicked(clone!(
            #[weak(rename_to=this)]
            self,
            move |_| {
                this.spawn_terminal();
            }
        ));

        self.set_child(Some(&terminal_overlay));
    }

    pub fn spawn_terminal(&self) {
        let imp = self.imp();

        if imp.terminal_pid.get().is_some() {
            return;
        }

        imp.reload_button.set_visible(false);
        let root_store = self.container().root_store();

        // Prepare the shell command
        let shell = root_store
            .command_runner()
            .wrap_command(
                Command::new("distrobox")
                    .arg("enter")
                    .arg(self.container().name())
                    .clone(),
            )
            .to_vec();

        let fut = imp.terminal.spawn_future(
            vte4::PtyFlags::DEFAULT,
            None,
            &shell
                .iter()
                .map(|s| s.to_str().unwrap())
                .collect::<Vec<_>>(),
            &[],
            glib::SpawnFlags::DEFAULT,
            || {},
            10,
        );

        glib::MainContext::default().spawn_local(clone!(
            #[weak(rename_to=this)]
            self,
            async move {
                match fut.await {
                    Ok(pid) => {
                        this.imp().terminal_pid.set(Some(pid));
                    }
                    Err(err) => {
                        eprintln!("Failed to spawn terminal: {}", err);
                        this.imp().reload_button.set_visible(true);
                    }
                }
            }
        ));
    }
}
