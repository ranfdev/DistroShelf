use crate::container_cli::Command;
use glib::subclass::prelude::*;
use gtk::glib;
use gtk::glib::derived_properties;
use gtk::glib::prelude::*;
use gtk::glib::Properties;
use std::cell::LazyCell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;


pub const DISTROS: LazyCell<
    HashMap<String, KnownDistro>,
    fn() -> HashMap<String, KnownDistro>,
> = LazyCell::new(|| {
    [
        ("alma", "#dadada", PackageManager::Dnf),
        ("alpine", "#2147ea", PackageManager::Unknown),
        ("amazon", "#de5412", PackageManager::Dnf),
        ("arch", "#12aaff", PackageManager::Unknown),
        ("centos", "#ff6600", PackageManager::Dnf),
        ("clearlinux", "#56bbff", PackageManager::Unknown),
        ("crystal", "#8839ef", PackageManager::Unknown),
        ("debian", "#da5555", PackageManager::Apt),
        ("deepin", "#0050ff", PackageManager::Apt),
        ("fedora", "#3b6db3", PackageManager::Dnf),
        ("gentoo", "#daaada", PackageManager::Unknown),
        ("kali", "#000000", PackageManager::Apt),
        ("mageia", "#b612b6", PackageManager::Dnf),
        ("mint", "#6fbd20", PackageManager::Apt),
        ("neon", "#27ae60", PackageManager::Apt),
        ("opensuse", "#daff00", PackageManager::Dnf),
        ("oracle", "#ff0000", PackageManager::Dnf),
        ("redhat", "#ff6662", PackageManager::Dnf),
        ("rhel", "#ff6662", PackageManager::Dnf),
        ("rocky", "#91ff91", PackageManager::Dnf),
        ("slackware", "#6145a7", PackageManager::Unknown),
        ("ubuntu", "#FF4400", PackageManager::Apt),
        ("vanilla", "#7f11e0", PackageManager::Unknown),
        ("void", "#abff12", PackageManager::Unknown),
    ]
    .iter()
    .map(|(name, color, package_manager)| {
        (
            name.to_string(),
            KnownDistro::new(name, color, *package_manager),
        )
    })
    .collect()
});

#[derive(Debug, Copy, Clone, PartialEq, Eq, glib::Enum, Default)]
#[enum_type(name = "DbxPackageManager")]
pub enum PackageManager {
    #[default]
    Unknown,
    Apt,
    Dnf,
}

impl PackageManager {
    pub fn install_cmd(&self, file: &Path) -> Option<Command> {
        match self {
            PackageManager::Apt => Some(apt_install_cmd(file)),
            PackageManager::Dnf => Some(dnf_install_cmd(file)),
            PackageManager::Unknown => None,
        }
    }
    pub fn installable_file(&self) -> Option<&str> {
        match self {
            PackageManager::Apt => Some(".deb"),
            PackageManager::Dnf => Some(".rpm"),
            PackageManager::Unknown => None,
        }
    }
}

fn apt_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("apt-get");
    cmd.arg("install").arg(file);
    cmd
}

fn dnf_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("dnf");
    cmd.arg("install").arg(file);
    cmd
}

pub fn known_distro_by_image(url: &str) -> Option<KnownDistro> {
    DISTROS
        .values()
        .find(|distro| url.contains(&distro.name()))
        .cloned()
}

pub fn generate_css() -> String {
    let mut out = String::new();
    let distros = DISTROS;
    for distro in distros.values() {
        let name = distro.name();
        let color = distro.color();
        out.push_str(&format!(
            ".{name} {{
    --distro-color: {color};
}}"
        ))
    }
    out
}

mod imp {
    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type=super::KnownDistro)]
    pub struct KnownDistro {
        #[property(get, set)]
        pub name: RefCell<String>,
        #[property(get, set)]
        pub color: RefCell<String>,
        #[property(get, set, builder(PackageManager::Unknown))]
        pub package_manager: RefCell<PackageManager>,
    }

    #[derived_properties]
    impl ObjectImpl for KnownDistro {}

    #[glib::object_subclass]
    impl ObjectSubclass for KnownDistro {
        const NAME: &'static str = "KnownDistro";
        type Type = super::KnownDistro;
    }
}

glib::wrapper! {
    pub struct KnownDistro(ObjectSubclass<imp::KnownDistro>);
}

impl Default for KnownDistro {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}

impl KnownDistro {
    pub fn new(name: &str, color: &str, package_manager: PackageManager) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("color", color)
            .property("package-manager", package_manager)
            .build()
    }
    pub fn icon_name(&self) -> String {
        format!("{}-symbolic", self.name())
    }
    pub fn default_icon_name() -> &'static str {
        "tux-symbolic"
    }
}
