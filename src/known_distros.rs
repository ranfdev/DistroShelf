use crate::distrobox::Command;
use std::path::Path;
use std::{
    cell::{LazyCell, OnceCell},
    collections::HashMap,
};

pub const DISTROS: LazyCell<HashMap<String, KnownDistro>, fn() -> HashMap<String, KnownDistro>> =
    LazyCell::new(|| {
        [
            ("alma", "#dadada", Some(PackageManager::Dnf)),
            ("alpine", "#2147ea", None),
            ("amazon", "#de5412", Some(PackageManager::Dnf)),
            ("arch", "#12aaff", None),
            ("centos", "#ff6600", Some(PackageManager::Dnf)),
            ("clearlinux", "#56bbff", None),
            ("crystal", "#8839ef", None),
            ("debian", "#da5555", Some(PackageManager::Apt)),
            ("deepin", "#0050ff", Some(PackageManager::Apt)),
            ("fedora", "#3b6db3", Some(PackageManager::Dnf)),
            ("gentoo", "#daaada", None),
            ("kali", "#000000", Some(PackageManager::Apt)),
            ("mageia", "#b612b6", Some(PackageManager::Dnf)),
            ("mint", "#6fbd20", Some(PackageManager::Apt)),
            ("neon", "#27ae60", Some(PackageManager::Apt)),
            ("opensuse", "#daff00", Some(PackageManager::Dnf)),
            ("oracle", "#ff0000", Some(PackageManager::Dnf)),
            ("redhat", "#ff6662", Some(PackageManager::Dnf)),
            ("rhel", "#ff6662", Some(PackageManager::Dnf)),
            ("rocky", "#91ff91", Some(PackageManager::Dnf)),
            ("slackware", "#6145a7", None),
            ("ubuntu", "#FF4400", Some(PackageManager::Apt)),
            ("vanilla", "#7f11e0", None),
            ("void", "#abff12", None),
        ]
        .iter()
        .map(|(name, color, package_manager)| {
            (
                name.to_string(),
                KnownDistro {
                    name,
                    color,
                    package_manager: package_manager.clone(),
                },
            )
        })
        .collect()
    });

#[derive(Debug, Clone, Hash, PartialEq)]
pub enum PackageManager {
    Apt,
    Dnf,
}

impl PackageManager {
    pub fn install_cmd(&self, file: &Path) -> Command {
        match self {
            PackageManager::Apt => apt_install_cmd(file),
            PackageManager::Dnf => dnf_install_cmd(file),
        }
    }
    pub fn installable_file(&self) -> &str {
        match self {
            PackageManager::Apt => ".deb",
            PackageManager::Dnf => ".rpm",
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

#[derive(Debug, Clone, Hash, PartialEq)]
pub struct KnownDistro {
    pub name: &'static str,
    pub color: &'static str,
    pub package_manager: Option<PackageManager>,
}

impl KnownDistro {
    pub fn icon_name(&self) -> String {
        format!("{}-symbolic", self.name)
    }
    pub fn default_icon_name() -> &'static str {
        "tux-symbolic"
    }
}

pub fn known_distro_by_image(url: &str) -> Option<KnownDistro> {
    DISTROS
        .values()
        .find(|distro| url.contains(distro.name))
        .cloned()
}

pub fn generate_css() -> String {
    let mut out = String::new();
    for distro in DISTROS.values() {
        let name = distro.name;
        let color = distro.color;
        out.push_str(&format!(
            ".{name} {{
    --distro-color: {color};
}}"
        ))
    }
    out
}
