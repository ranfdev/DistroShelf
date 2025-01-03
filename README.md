# DistroHome - A GUI for Distrobox Containers

DistroHome is a graphical interface for managing [Distrobox](https://distrobox.it/) containers on Linux. It provides an easy way to:

- Create and manage containers
- View container status and details
- Install packages
- Manage exported applications
- Open terminal sessions
- Upgrade containers
- Clone and delete containers

![Screenshot](screenshot.png) *Screenshot placeholder*

## Features

- **Container Management**
  - Create new containers with custom images
  - Start/stop containers
  - View container status (running/stopped)
  - Delete containers
  - Clone existing containers

- **Package Management**
  - Install packages using the container's native package manager
  - Supported package formats: .deb, .rpm, etc.

- **Application Exporting**
  - Manage desktop applications exported from containers
  - Export/unexport applications to host system

- **Terminal Integration**
  - Open terminal sessions in containers
  - Configurable terminal emulator support

- **System Integration**
  - Flatpak support
  - Automatic container status updates
  - Task progress tracking

## Installation

### Requirements
- Distrobox installed and configured
- GTK 4 and libadwaita
- Supported terminal emulator (GNOME Terminal, Konsole, etc.)

### Flatpak (Recommended)
```bash
flatpak install flathub com.ranfdev.DistroHome
```

### From Source
1. Clone the repository:
```bash
git clone https://github.com/ranfdev/DistroHome.git
cd DistroHome
```

2. Build and install:
```bash
meson build --prefix=/usr
ninja -C build
sudo ninja -C build install
```

## Usage

1. Launch DistroHome from your application menu
2. The sidebar shows your existing containers
3. Select a container to view details and manage it
4. Use the buttons to perform actions like:
   - Opening a terminal
   - Installing packages
   - Exporting applications
   - Upgrading the container
   - Deleting the container

## Configuration

You can configure your preferred terminal emulator in the Preferences dialog.

Supported terminals:
- GNOME Terminal
- Konsole
- Xfce Terminal
- Tilix
- Alacritty
- And more...

## Contributing

Contributions are welcome! Please open an issue or pull request on GitHub.

## License

DistroHome is licensed under the GPL-3.0-or-later license. See [LICENSE](LICENSE) for details.

## Credits

- Distro icons from [font-logos](https://github.com/lukas-w/font-logos)
- Inspired by [Distrobox](https://distrobox.it/)
