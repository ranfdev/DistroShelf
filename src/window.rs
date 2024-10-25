/* window.rs
 *
 * Copyright 2024 Lorenzo Miglietta
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use std::cell::Ref;
use std::collections::HashMap;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use crate::sidebar_row::SidebarRow;
use crate::distrobox::{self, Distrobox};

mod imp {
    use std::{cell::RefCell, collections::{HashMap, HashSet}};

    use distrobox::ContainerInfo;

    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/ranfdev/DistroHome/window.ui")]
    pub struct DistrohomeWindow {
        // Template widgets
        #[template_child]
        pub sidebar_content: TemplateChild<adw::Bin>,
        #[template_child]
        pub main_content: TemplateChild<adw::Bin>,
        pub distrobox: RefCell<Option<Distrobox>>,
        pub containers: RefCell<HashMap<String, ContainerInfo>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DistrohomeWindow {
        const NAME: &'static str = "DistrohomeWindow";
        type Type = super::DistrohomeWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DistrohomeWindow {}
    impl WidgetImpl for DistrohomeWindow {}
    impl WindowImpl for DistrohomeWindow {}
    impl ApplicationWindowImpl for DistrohomeWindow {}
    impl AdwApplicationWindowImpl for DistrohomeWindow {}
}

glib::wrapper! {
    pub struct DistrohomeWindow(ObjectSubclass<imp::DistrohomeWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,        @implements gio::ActionGroup, gio::ActionMap;
}

impl DistrohomeWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        let this: Self = glib::Object::builder()
            .property("application", application)
            .build();
        this.init_distrobox();
        this.build_sidebar();
        this.build_main_content();
        this
    }

    fn init_distrobox(&self) {
        let imp = self.imp();
        let distrobox = Distrobox::new();
        imp.distrobox.replace(Some(distrobox));
    }

    fn distrobox(&self) -> Ref<Distrobox> {
        Ref::map(self.imp().distrobox.borrow(), |x| x.as_ref().unwrap())
    }

    fn build_sidebar(&self) {
        // Create the sidebar list box with proper styling
        let sidebar = gtk::ListBox::builder()
            .css_classes(vec!["navigation-sidebar"]) // GNOME style class
            .selection_mode(gtk::SelectionMode::Single)
            .build();

        sidebar.connect_row_activated(move |_list_box, row| {
            
        });

        // Create a scrolled window for the sidebar
        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .min_content_width(200)
            .child(&sidebar)
            .build();

        let containers = self.distrobox().list().unwrap();
        let containers: HashMap<String, distrobox::ContainerInfo> = HashMap::from_iter(containers.into_iter().map(|x| (x.id.clone(), x)));

       
        for (id, container) in containers {
            // Create a row for each item
            let row = SidebarRow::new(&id, "", &container.name, &container.image);
            sidebar.append(&row);
        }

        self.imp().sidebar_content.set_child(Some(&scrolled_window));
    }
    pub fn build_container_header(&self) {

    }
    pub fn build_main_content(&self) {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        
        // Main container with scrolling
        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled_window.set_vexpand(true);
        scrolled_window.set_margin_top(12);
        scrolled_window.set_margin_bottom(12);
        
        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 24);
        main_box.set_margin_start(12);
        main_box.set_margin_end(12);

        // Status Group
        let status_group = adw::PreferencesGroup::new();
        status_group.set_title("Container Status");
        
        let status_row = adw::ActionRow::new();
        status_group.add_css_class("property");
        status_row.set_title("Status");
        status_row.set_subtitle("Running");
        
        let auto_start_row = adw::SwitchRow::new();
        auto_start_row.set_title("Auto Start");
        auto_start_row.set_subtitle("Start container when system boots");
        
        let init_system_row = adw::SwitchRow::new();
        init_system_row.set_title("Init System");
        init_system_row.set_subtitle("Enable systemd inside container");
        
        status_group.add(&status_row);
        status_group.add(&auto_start_row);
        status_group.add(&init_system_row);

        // Quick Actions Group
        let actions_group = adw::PreferencesGroup::new();
        actions_group.set_title("Quick Actions");
        
        let terminal_row = Self::create_button_row(
            "Open Terminal",
            "terminal-symbolic",
            "Launch an interactive shell",
        );
        
        let upgrade_row = Self::create_button_row(
            "Upgrade Container",
            "system-reboot-update-symbolic",
            "Update all packages",
        );
        
        let apps_row = Self::create_button_row(
            "Applications",
            "applications-all-symbolic",
            "Manage exportable applications",
        );
        
        actions_group.add(&terminal_row);
        actions_group.add(&upgrade_row);
        actions_group.add(&apps_row);

        // Container Configuration Group
        let config_group = adw::PreferencesGroup::new();
        config_group.set_title("Configuration");
        
        let name_row = adw::EntryRow::new();
        name_row.set_title("Container Name");
        name_row.set_text("debian-12");
        
        let image_combo = adw::ComboRow::new();
        image_combo.set_title("Base Image");
        image_combo.set_subtitle("Select the container's base distribution");
        image_combo.set_model(Some(&Self::create_distro_model()));
        
        let home_row = adw::SwitchRow::new();
        home_row.set_title("Mount Home Directory");
        home_row.set_subtitle("Share your home directory with the container");
        
        let nvidia_row = adw::SwitchRow::new();
        nvidia_row.set_title("NVIDIA Support");
        nvidia_row.set_subtitle("Enable NVIDIA GPU acceleration");
        
        config_group.add(&name_row);
        config_group.add(&image_combo);
        config_group.add(&home_row);
        config_group.add(&nvidia_row);

        // Advanced Operations Group
        let advanced_group = adw::PreferencesGroup::new();
        advanced_group.set_title("Advanced Operations");
        
        let export_expander = adw::ExpanderRow::new();
        export_expander.set_title("Export Options");
        export_expander.set_subtitle("Configure application and binary export");
        
        let export_all_row = adw::SwitchRow::new();
        export_all_row.set_title("Export All Applications");
        export_all_row.set_subtitle("Make all applications available on host");
        
        let export_path_row = adw::EntryRow::new();
        export_path_row.set_title("Export Path");
        export_path_row.set_text("/usr/local/bin");
        
        export_expander.add_row(&export_all_row);
        export_expander.add_row(&export_path_row);
        
        let install_deb = Self::create_button_row(
            "Install .deb Package",
            "package-symbolic",
            "Install Debian packages into container",
        );
        
        let clone_row = Self::create_button_row(
            "Clone Container",
            "edit-copy-symbolic",
            "Create a copy of this container",
        );
        
        advanced_group.add(&export_expander);
        advanced_group.add(&install_deb);
        advanced_group.add(&clone_row);

        // Danger Zone Group
        let danger_group = adw::PreferencesGroup::new();
        danger_group.set_title("Danger Zone");
        danger_group.add_css_class("danger-group");
        
        let delete_row = Self::create_button_row(
            "Delete Container",
            "user-trash-symbolic",
            "Permanently remove this container and all its data",
        );
        delete_row.add_css_class("error");
        
        danger_group.add(&delete_row);

        // Add all groups to main box
        main_box.append(&status_group);
        main_box.append(&actions_group);
        main_box.append(&config_group);
        main_box.append(&advanced_group);
        main_box.append(&danger_group);
        
        /* // Connect signals
        terminal_row.connect_activated(clone!(@weak widget => move |_| {
            // TODO: Launch terminal
        }));
        
        upgrade_row.connect_activated(clone!(@weak widget => move |_| {
            // TODO: Show upgrade dialog
        }));
        
        apps_row.connect_activated(clone!(@weak widget => move |_| {
            // TODO: Show applications window
        }));
        
        install_deb.connect_activated(clone!(@weak widget => move |_| {
            // TODO: Show file chooser
        }));
        
        clone_row.connect_activated(clone!(@weak widget => move |_| {
            // TODO: Show clone dialog
        }));
        
        delete_row.connect_activated(clone!(@weak widget => move |_| {
            // TODO: Show delete confirmation
        }));
        */
        // Finish layout
        scrolled_window.set_child(Some(&main_box));
        widget.append(&scrolled_window);
        
        self.imp().main_content.set_child(Some(&widget));
    }

fn create_button_row(title: &str, icon_name: &str, subtitle: &str) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(title);
    row.set_subtitle(subtitle);
    
    let icon = gtk::Image::from_icon_name(icon_name);
    row.add_prefix(&icon);

    row.set_activatable(true);
    
    row
}
fn create_distro_model() -> gtk::StringList {
    let model = gtk::StringList::new(&[
        "debian:12",
        "ubuntu:22.04",
        "fedora:39",
        "archlinux:latest",
        "opensuse/tumbleweed:latest",
    ]);
    model
}
}
