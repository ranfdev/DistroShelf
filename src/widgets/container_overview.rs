use crate::container::Container;
use crate::widgets::DistroShelfWindow;

use crate::gtk_utils::reaction;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::clone;
use gtk::{self, gdk, glib, pango};
use std::cell::RefCell;

mod imp {
    use super::*;
    use gtk::glib::{Properties, derived_properties};

    // Object holding the state
    #[derive(Default, Properties)]
    #[properties(wrapper_type=super::ContainerOverview)]
    pub struct ContainerOverview {
        #[property(get, set=Self::set_container)]
        pub container: RefCell<Container>,
    }

    impl ContainerOverview {
        fn set_container(&self, value: &Container) {
            self.container.replace(value.clone());

            self.obj()
                .set_child(Some(&self.obj().build_main_content(value)));
        }
    }

    // The central trait for subclassing a GObject
    #[glib::object_subclass]
    impl ObjectSubclass for ContainerOverview {
        const NAME: &'static str = "ContainerOverview";
        type Type = super::ContainerOverview;
        type ParentType = adw::Bin;

        fn new() -> Self {
            Self {
                container: Default::default(),
            }
        }
    }

    #[derived_properties]
    impl ObjectImpl for ContainerOverview {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for ContainerOverview {}
    impl BinImpl for ContainerOverview {}
}

glib::wrapper! {
    pub struct ContainerOverview(ObjectSubclass<imp::ContainerOverview>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ContainerOverview {
    pub fn new(container: &Container) -> Self {
        let obj: Self = glib::Object::builder().build();
        obj.set_container(container);
        obj
    }

    pub fn build_main_content(&self, container: &Container) -> gtk::Widget {
        let clamp = adw::Clamp::new();

        // Main container with scrolling
        let scrolled_window = gtk::ScrolledWindow::new();
        scrolled_window.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled_window.set_vexpand(true);

        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 24);
        main_box.set_margin_start(12);
        main_box.set_margin_end(12);
        main_box.set_margin_top(12);
        main_box.set_margin_bottom(24);

        // Status Group
        let status_group = adw::PreferencesGroup::new();
        status_group.set_title("Container Status");

        let status_row = adw::ActionRow::new();
        status_group.add_css_class("property");
        status_row.set_title("Status");

        let status_child = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        status_child.set_valign(gtk::Align::Center);

        let stop_btn = gtk::Button::from_icon_name("media-playback-stop-symbolic");
        stop_btn.set_tooltip_text(Some("Stop"));
        stop_btn.set_action_name(Some("win.stop-container"));
        status_child.append(&stop_btn);

        let terminal_btn = gtk::Button::from_icon_name("terminal-symbolic");
        terminal_btn.set_tooltip_text(Some("Open Terminal"));
        terminal_btn.set_action_name(Some("win.open-terminal"));
        status_child.append(&terminal_btn);

        status_row.add_suffix(&status_child);
        status_group.add(&status_row);

        // Usage stats row
        let usage_row = adw::ActionRow::new();
        usage_row.set_title("Resources");
        usage_row.set_subtitle(&format!("CPU: 0.0% • Mem: 0 (0%)"));
        status_group.add(&usage_row);

        let usage_query = container.usage();
        usage_query.connect_success(clone!(
            #[weak]
            usage_row,
            move |usage| {
                usage_row.set_subtitle(&format!(
                    "CPU: {} • Mem: {} ({})",
                    usage.cpu_perc, usage.mem_usage, usage.mem_perc
                ));
            }
        ));

        reaction! {
            (container.status_detail(), container.status_tag()),
            move |(detail, tag): (String, String)| {
                let text = format!("{tag}: {detail}");
                status_row.set_subtitle(&text);
                stop_btn.set_visible(tag == "up");
                if tag == "up" {
                    usage_query.fetch();
                }
            }
        };

        // Quick Actions Group
        let actions_group = adw::PreferencesGroup::new();
        actions_group.set_title("Quick Actions");

        let upgrade_row = self.create_button_row(
            "Upgrade Container",
            "software-update-available-symbolic",
            "Update all packages",
            "win.upgrade-container",
        );
        actions_group.add(&upgrade_row);

        let apps_row = self.create_button_row(
            "Applications",
            "view-list-bullet-symbolic",
            "Manage exportable applications",
            "win.view-exportable-apps",
        );
        actions_group.add(&apps_row);

        if let Some(distro) = container.distro() {
            if let Some(installable_file) = distro.package_manager().installable_file() {
                let install_package_row = self.create_button_row(
                    &format!("Install {} Package", installable_file),
                    "package-symbolic",
                    "Install packages into container",
                    "win.install-package",
                );
                actions_group.add(&install_package_row);
            }
        }

        let clone_row = self.create_button_row(
            "Clone Container",
            "edit-copy-symbolic",
            "Create a copy of this container",
            "win.clone-container",
        );
        actions_group.add(&clone_row);

        // Danger Zone Group
        let danger_group = adw::PreferencesGroup::new();
        danger_group.set_title("Danger Zone");
        danger_group.add_css_class("danger-group");

        let delete_row = self.create_button_row(
            "Delete Container",
            "user-trash-symbolic",
            "Permanently remove this container and all its data",
            "win.delete-container",
        );
        delete_row.add_css_class("error");

        danger_group.add(&delete_row);

        // Add all groups to main box
        main_box.append(&self.build_container_header(container));
        main_box.append(&status_group);
        main_box.append(&actions_group);
        main_box.append(&danger_group);

        // Finish layout: the Overview page with Clamp inside ScrolledWindow
        clamp.set_child(Some(&main_box));
        scrolled_window.set_child(Some(&clamp));

        scrolled_window.upcast()
    }

    pub fn build_container_header(&self, container: &Container) -> gtk::Box {
        // Create labels for the title and subtitle
        let title_label = gtk::Label::new(Some(&container.name()));
        title_label.set_xalign(0.0);
        title_label.add_css_class("title-1");

        // Create image URL label
        let subtitle_label = gtk::Label::new(Some(&container.image()));
        subtitle_label.set_xalign(0.0);
        subtitle_label.set_ellipsize(pango::EllipsizeMode::Middle);
        subtitle_label.add_css_class("subtitle");

        // Create a copy button next to the image URL label
        let copy_btn = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy_btn.set_tooltip_text(Some("Copy image URL"));
        copy_btn.add_css_class("flat");
        copy_btn.add_css_class("xs");
        // Capture a clone of the image URL for the closure
        let image_url = container.image().to_string();
        let this = self.clone();
        copy_btn.connect_clicked(move |_| {
            if let Some(display) = gdk::Display::default() {
                let clipboard = display.primary_clipboard();
                clipboard.set_text(&image_url);

                let parent_window: Option<DistroShelfWindow> = this
                    .ancestor(DistroShelfWindow::static_type())
                    .and_then(|w| w.downcast::<DistroShelfWindow>().ok());
                if let Some(win) = parent_window {
                    win.add_toast(adw::Toast::new("Image URL copied"));
                }
            }
        });

        // Create a horizontal box that contains the subtitle label and copy button
        let subtitle_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        subtitle_box.append(&subtitle_label);
        subtitle_box.append(&copy_btn);

        // Create a vertical box and add the title and the new subtitle_box
        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        text_box.append(&title_label);
        text_box.append(&subtitle_box);

        // Add the text box and status label to the header box
        let icon = gtk::Image::new();
        icon.add_css_class("header-icon");
        icon.set_icon_size(gtk::IconSize::Large);

        let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header_box.add_css_class("distro-header");

        header_box.append(&icon);
        header_box.append(&text_box);

        if let Some(distro) = container.distro() {
            header_box.add_css_class(format!("distro-{}", &distro.name()).as_str());
            icon.set_icon_name(Some(&distro.name()));
        }

        header_box
    }

    fn create_button_row(
        &self,
        title: &str,
        icon_name: &str,
        subtitle: &str,
        action_name: &str,
    ) -> adw::ActionRow {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle(subtitle);

        let icon = gtk::Image::from_icon_name(icon_name);
        row.add_prefix(&icon);
        row.set_activatable(true);
        row.set_action_name(Some(action_name));
        row
    }
}
