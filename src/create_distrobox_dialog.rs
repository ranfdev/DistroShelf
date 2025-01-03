use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib::SignalHandlerId;
use gtk::{gio, glib};

use crate::distrobox::{self, CreateArgName, CreateArgs, Error, ExportableApp};
use crate::distrobox_service::DistroboxService;
use crate::resource::{Resource, SharedResource};

use glib::subclass::Signal;
use std::{cell::RefCell, rc::Rc};

use glib::clone;
use std::sync::OnceLock;

mod imp {

    use std::cell::{LazyCell, OnceCell};

    use crate::distro_combo_row_item;

    use super::*;

    #[derive(Default)]
    pub struct CreateDistroboxDialog {
        pub dialog: adw::Dialog,
        pub toolbar_view: adw::ToolbarView,
        pub content: gtk::Box,
        pub name_row: adw::EntryRow,
        pub image_row: adw::ComboRow,
        pub home_row: adw::SwitchRow,
        pub nvidia_row: adw::SwitchRow,
        pub init_row: adw::SwitchRow,
        pub volume_rows: Rc<RefCell<Vec<adw::EntryRow>>>,
        pub scrolled_window: gtk::ScrolledWindow,
        pub shared_resource: SharedResource<Vec<distrobox::ExportableApp>, anyhow::Error>,
        pub distrobox_service: OnceCell<DistroboxService>,
        pub current_create_args: RefCell<CreateArgs>,
    }

    impl ObjectImpl for CreateDistroboxDialog {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("create-requested").build()])
        }
        fn constructed(&self) {
            self.obj().set_title("Create a Distrobox");
            self.obj().set_content_width(480);

            let toolbar_view = adw::ToolbarView::new();
            let header = adw::HeaderBar::new();
        
            // Add view switcher
            let view_switcher = adw::ViewSwitcher::new();
            let view_stack = adw::ViewStack::new();
        
            // Create main content box
            let main_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
            main_content.set_margin_start(12);
            main_content.set_margin_end(12);
            main_content.set_margin_top(12);
            main_content.set_margin_bottom(12);

            // Create GUI creation page
            let gui_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
            let preferences_group = adw::PreferencesGroup::new();
            preferences_group.set_title("Settings");

            self.name_row.set_title("Name");

            self.image_row
                .set_expression(Some(&gtk::PropertyExpression::new(
                    gtk::StringObject::static_type(),
                    None::<gtk::Expression>,
                    "string",
                )));
            let item_factory = gtk::SignalListItemFactory::new();
            item_factory.connect_setup(|_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                item.set_child(Some(&distro_combo_row_item::DistroComboRowItem::new()));
            });
            item_factory.connect_bind(|_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().unwrap();
                let image = item
                    .item()
                    .and_downcast::<gtk::StringObject>()
                    .unwrap()
                    .string();
                let child = item.child();
                let child: &distro_combo_row_item::DistroComboRowItem =
                    child.and_downcast_ref().unwrap();
                child.set_image(&image);
            });
            self.image_row.set_factory(Some(&item_factory));
            self.image_row.set_enable_search(true);
            self.image_row
                .set_search_match_mode(gtk::StringFilterMatchMode::Substring);
            self.image_row.set_title("Base Image");
            self.image_row.set_use_subtitle(true);

            self.home_row.set_title("Mount Home Directory");

            let file_chooser_fun = clone!(move || {
                // let file_chooser = gtk::FileChooserDialog::new(
                //     Some("Select Home Directory"),
                //     Some(&obj),
                //     gtk::FileChooserAction::SelectFolder,
                //     &[
                //         ("Cancel", gtk::ResponseType::Cancel),
                //         ("Select", gtk::ResponseType::Accept),
                //     ],
                // );

                // file_chooser.connect_response(
                //     clone!(@weak home_row => move |file_chooser, response| {
                //     if response == gtk::ResponseType::Accept {
                //     if let Some(folder) = file_chooser.file() {
                //     home_row.set_subtitle(&folder.path().unwrap().display().to_string());
                //     }
                //     }
                //     file_chooser.close();
                //     }),
                // );

                // file_chooser.show();
            });

            let home_path_button = gtk::Button::from_icon_name("folder-symbolic");
            home_path_button.set_sensitive(false);
            home_path_button.set_valign(gtk::Align::Center);
            home_path_button.set_tooltip_text(Some("Select Home Directory"));
            home_path_button.add_css_class("flat");
            let file_chooser_fun_clone = file_chooser_fun.clone();
            self.home_row.connect_active_notify(clone!(
                #[weak]
                home_path_button,
                move |row| {
                    if row.is_active() {
                        file_chooser_fun_clone();
                    }
                    home_path_button.set_sensitive(row.is_active());
                }
            ));
            home_path_button.connect_clicked(move |_| file_chooser_fun());

            self.home_row.add_suffix(&home_path_button);

            let nvidia_row = adw::SwitchRow::new();
            nvidia_row.set_title("NVIDIA Support");

            let init_row = adw::SwitchRow::new();
            init_row.set_title("Init process");

            preferences_group.add(&self.name_row);
            preferences_group.add(&self.image_row);
            preferences_group.add(&self.home_row);
            preferences_group.add(&nvidia_row);
            preferences_group.add(&init_row);

            let volumes_group = self.obj().build_volumes_group();
            self.content.append(&preferences_group);
            self.content.append(&volumes_group);

            let create_btn = gtk::Button::with_label("Create");
            create_btn.set_halign(gtk::Align::Center);

            let obj = self.obj();
            create_btn.connect_clicked(clone!(
                #[weak]
                obj,
                move |_| {
                    let res = obj.update_create_args();
                    obj.update_errors(&res);
                    match res {
                        Ok(()) => {
                            obj.emit_by_name_with_values("create-requested", &[]);
                        }
                        _ => {}
                    }
                }
            ));
            create_btn.add_css_class("suggested-action");
            create_btn.add_css_class("pill");
            create_btn.set_margin_top(12);

            self.content.append(&create_btn);

            self.scrolled_window.set_child(Some(&self.content));
            self.scrolled_window.set_propagate_natural_height(true);
            self.scrolled_window.set_vexpand(true);
            toolbar_view.set_content(Some(&self.scrolled_window));
            self.obj().set_child(Some(&toolbar_view));
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CreateDistroboxDialog {
        const NAME: &'static str = "CreateDistroboxDialog";
        type Type = super::CreateDistroboxDialog;
        type ParentType = adw::Dialog;
    }

    impl WidgetImpl for CreateDistroboxDialog {}
    impl AdwDialogImpl for CreateDistroboxDialog {}
}

glib::wrapper! {
    pub struct CreateDistroboxDialog(ObjectSubclass<imp::CreateDistroboxDialog>)
        @extends adw::Dialog, gtk::Widget;
}
impl CreateDistroboxDialog {
    pub fn new(distrobox_service: DistroboxService) -> Self {
        let this: Self = glib::Object::builder().build();

        distrobox_service.connect_images_changed(clone!(
            #[weak]
            this,
            move |service| {
                let string_list = gtk::StringList::new(&[]);
                if let Resource::Loaded(images) = service.images() {
                    for image in images {
                        string_list.append(&image);
                    }
                } else {
                    dbg!("Loading images...");
                }
                this.imp().image_row.set_model(Some(&string_list));
            }
        ));
        distrobox_service.load_images();

        this.imp().distrobox_service.set(distrobox_service).unwrap();
        this
    }

    pub fn update_create_args(&self) -> Result<(), Error> {
        let imp = self.imp();
        let image = imp
            .image_row
            .selected_item()
            .unwrap()
            .downcast_ref::<gtk::StringObject>()
            .unwrap()
            .string();
        let volumes = imp
            .volume_rows
            .borrow()
            .iter()
            .filter_map(|entry| {
                if !entry.text().is_empty() {
                    Some(entry.text().to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let name = CreateArgName::new(&imp.name_row.text())?;

        let create_args = CreateArgs {
            name,
            image: image.to_string(),
            nvidia: imp.nvidia_row.is_active(),
            home_path: imp.home_row.subtitle().map(|s| s.to_string()).unwrap(), // TODO: handle None,
            init: imp.init_row.is_active(),
            volumes,
        };

        self.imp().current_create_args.replace(create_args);
        Ok(())
    }

    pub fn build_volumes_group(&self) -> adw::PreferencesGroup {
        let volumes_group = adw::PreferencesGroup::new();
        volumes_group.set_title("Volumes");
        volumes_group.set_description(Some("Specify volumes in the format 'dest_dir:source_dir'"));

        let add_volume_button = adw::ButtonRow::builder().title("Add Volume").build();
        add_volume_button.connect_activated(clone!(
            #[weak(rename_to=this)]
            self,
            #[weak]
            volumes_group,
            move |_| {
                let volume_row = adw::EntryRow::new();
                volume_row.set_title("Volume");

                let remove_button = gtk::Button::from_icon_name("user-trash-symbolic");
                remove_button.set_tooltip_text(Some("Remove Volume"));
                remove_button.add_css_class("flat");
                remove_button.set_valign(gtk::Align::Center);
                remove_button.add_css_class("destructive-action");
                remove_button.connect_clicked(clone!(
                    #[weak]
                    this,
                    #[weak]
                    volumes_group,
                    #[weak]
                    volume_row,
                    move |_| {
                        this.imp()
                            .volume_rows
                            .borrow_mut()
                            .retain(|row| row != &volume_row);
                        volumes_group.remove(&volume_row);
                    }
                ));
                volume_row.add_suffix(&remove_button);

                this.imp().volume_rows.borrow_mut().push(volume_row.clone());
                volumes_group.add(&volume_row);
            }
        ));

        volumes_group.add(&add_volume_button);

        volumes_group
    }

    fn update_errors(&self, res: &Result<(), distrobox::Error>) {
        let imp = self.imp();
        imp.name_row.remove_css_class("error");
        imp.name_row.set_tooltip_text(None);
        match res {
            Err(distrobox::Error::InvalidField(field, msg)) if field == "name" => {
                imp.name_row.add_css_class("error");
                imp.name_row.set_tooltip_text(Some(&msg));
            }
            _ => {}
        }
    }

    pub fn connect_create_requested<F: Fn(&Self, CreateArgs) + 'static>(
        &self,
        f: F,
    ) -> SignalHandlerId {
        self.connect_local("create-requested", false, move |args| {
            let obj = args[0].get::<Self>().unwrap();
            f(&obj, obj.imp().current_create_args.borrow().clone());
            None
        })
    }
}
