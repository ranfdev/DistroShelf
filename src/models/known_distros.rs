use glib::subclass::prelude::*;
use gtk::glib;
use gtk::glib::Properties;
use gtk::glib::derived_properties;
use gtk::glib::prelude::*;
use std::cell::LazyCell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use crate::fakers::Command;

pub const DISTROS: LazyCell<HashMap<String, KnownDistro>, fn() -> HashMap<String, KnownDistro>> =
    LazyCell::new(|| {
        [
            ("alma", "#dadada", PackageManager::Dnf),
            ("alpine", "#2147ea", PackageManager::Apk),
            ("amazon", "#de5412", PackageManager::Dnf),
            ("arch", "#12aaff", PackageManager::Pacman),
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
            ("opensuse", "#daff00", PackageManager::Zypper),
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
    Pacman,
    Apk,
    Zypper,
}

impl PackageManager {
    pub fn install_cmd(&self, file: &Path) -> Option<Command> {
        match self {
            PackageManager::Apt => Some(apt_install_cmd(file)),
            PackageManager::Dnf => Some(dnf_install_cmd(file)),
            PackageManager::Pacman => Some(pacman_install_cmd(file)),
            PackageManager::Apk => Some(apk_install_cmd(file)),
            PackageManager::Zypper => Some(zypper_install_cmd(file)),
            PackageManager::Unknown => None,
        }
    }
    pub fn installable_file(&self) -> Option<&str> {
        match self {
            PackageManager::Apt => Some(".deb"),
            PackageManager::Dnf => Some(".rpm"),
            PackageManager::Pacman => Some(".pkg.tar.zst"),
            PackageManager::Apk => Some(".apk"),
            PackageManager::Zypper => Some(".rpm"),
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

fn pacman_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("pacman");
    cmd.arg("-U").arg(file);
    cmd
}

fn apk_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("apk");
    cmd.arg("add").arg("--allow-untrusted").arg(file);
    cmd
}

fn zypper_install_cmd(file: &Path) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.arg("zypper");
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
            ".distro-{name} {{
    --distro-color: {color};
}}\n"
        ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_package_manager_installable_file() {
        assert_eq!(PackageManager::Apt.installable_file(), Some(".deb"));
        assert_eq!(PackageManager::Dnf.installable_file(), Some(".rpm"));
        assert_eq!(
            PackageManager::Pacman.installable_file(),
            Some(".pkg.tar.zst")
        );
        assert_eq!(PackageManager::Apk.installable_file(), Some(".apk"));
        assert_eq!(PackageManager::Zypper.installable_file(), Some(".rpm"));
        assert_eq!(PackageManager::Unknown.installable_file(), None);
    }

    #[test]
    fn test_package_manager_install_cmd_apt() {
        let file = PathBuf::from("/tmp/package.deb");
        let cmd = PackageManager::Apt.install_cmd(&file).unwrap();

        assert_eq!(cmd.program.to_string_lossy(), "sudo");
        assert_eq!(cmd.args[0].to_string_lossy(), "apt-get");
        assert_eq!(cmd.args[1].to_string_lossy(), "install");
        assert_eq!(cmd.args[2].to_string_lossy(), "/tmp/package.deb");
    }

    #[test]
    fn test_package_manager_install_cmd_dnf() {
        let file = PathBuf::from("/tmp/package.rpm");
        let cmd = PackageManager::Dnf.install_cmd(&file).unwrap();

        assert_eq!(cmd.program.to_string_lossy(), "sudo");
        assert_eq!(cmd.args[0].to_string_lossy(), "dnf");
        assert_eq!(cmd.args[1].to_string_lossy(), "install");
    }

    #[test]
    fn test_package_manager_install_cmd_pacman() {
        let file = PathBuf::from("/tmp/package.pkg.tar.zst");
        let cmd = PackageManager::Pacman.install_cmd(&file).unwrap();

        assert_eq!(cmd.program.to_string_lossy(), "sudo");
        assert_eq!(cmd.args[0].to_string_lossy(), "pacman");
        assert_eq!(cmd.args[1].to_string_lossy(), "-U");
    }

    #[test]
    fn test_package_manager_install_cmd_apk() {
        let file = PathBuf::from("/tmp/package.apk");
        let cmd = PackageManager::Apk.install_cmd(&file).unwrap();

        assert_eq!(cmd.program.to_string_lossy(), "sudo");
        assert_eq!(cmd.args[0].to_string_lossy(), "apk");
        assert_eq!(cmd.args[1].to_string_lossy(), "add");
        assert_eq!(cmd.args[2].to_string_lossy(), "--allow-untrusted");
    }

    #[test]
    fn test_package_manager_install_cmd_zypper() {
        let file = PathBuf::from("/tmp/package.rpm");
        let cmd = PackageManager::Zypper.install_cmd(&file).unwrap();

        assert_eq!(cmd.program.to_string_lossy(), "sudo");
        assert_eq!(cmd.args[0].to_string_lossy(), "zypper");
        assert_eq!(cmd.args[1].to_string_lossy(), "install");
    }

    #[test]
    fn test_package_manager_install_cmd_unknown() {
        let file = PathBuf::from("/tmp/package");
        assert!(PackageManager::Unknown.install_cmd(&file).is_none());
    }

    #[test]
    fn test_known_distro_by_image_ubuntu() {
        let distro = known_distro_by_image("docker.io/library/ubuntu:latest");
        assert!(distro.is_some());
        assert_eq!(distro.unwrap().name(), "ubuntu");
    }

    #[test]
    fn test_known_distro_by_image_fedora() {
        let distro = known_distro_by_image("ghcr.io/ublue-os/fedora-toolbox:latest");
        assert!(distro.is_some());
        assert_eq!(distro.unwrap().name(), "fedora");
    }

    #[test]
    fn test_known_distro_by_image_arch() {
        let distro = known_distro_by_image("docker.io/library/archlinux:latest");
        assert!(distro.is_some());
        assert_eq!(distro.unwrap().name(), "arch");
    }

    #[test]
    fn test_known_distro_by_image_unknown() {
        let distro = known_distro_by_image("docker.io/library/unknown-distro:latest");
        assert!(distro.is_none());
    }

    #[test]
    fn test_generate_css() {
        let css = generate_css();

        // Check that CSS contains distro classes
        assert!(css.contains(".distro-ubuntu"));
        assert!(css.contains(".distro-fedora"));
        assert!(css.contains("--distro-color:"));
    }

    #[test]
    fn test_distros_map_contains_common_distros() {
        assert!(DISTROS.contains_key("ubuntu"));
        assert!(DISTROS.contains_key("fedora"));
        assert!(DISTROS.contains_key("arch"));
        assert!(DISTROS.contains_key("debian"));
        assert!(DISTROS.contains_key("alpine"));
    }

    #[test]
    fn test_package_manager_default() {
        let pm: PackageManager = Default::default();
        assert_eq!(pm, PackageManager::Unknown);
    }

    #[test]
    fn test_known_distro_default_icon_name() {
        assert_eq!(KnownDistro::default_icon_name(), "tux-symbolic");
    }
}
