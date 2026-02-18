---
name: gnome-sdk-docs
description: "Browse GNOME SDK documentation including GObject Introspection (.gir) files, D-Bus interfaces or icons that can be useful for GNOME apps development."
---

# Browsing GNOME SDK Documentation

This skill provides access to various GNOME SDK resources for development purposes.

## GObject Introspection Files

GObject Introspection (`.gir`) files are XML files describing the API of GNOME libraries. They are located at `/usr/share/gir-1.0/`.

The libraries used by this project and their corresponding files:

- `/usr/share/gir-1.0/Gtk-4.0.gir`
- `/usr/share/gir-1.0/Adw-1.gir`
- `/usr/share/gir-1.0/Gio-2.0.gir`
- `/usr/share/gir-1.0/GLib-2.0.gir`
- `/usr/share/gir-1.0/GObject-2.0.gir`
- `/usr/share/gir-1.0/Soup-3.0.gir`

These files are large XML documents. Use Grep to search for specific class names, method names, property names, or signal names rather than reading entire files. For example:

- To find a class: `Grep` for `<class name="Button"` in the relevant `.gir` file.
- To find a method: `Grep` for `<method name="set_label"`.
- To find properties of a class: `Grep` for `<property name=` near the class definition.
- To find signals: `Grep` for `<glib:signal name=`.

## D-Bus Interfaces

D-Bus interface definitions are XML files describing D-Bus services and their methods, signals, and properties. They are located at `/usr/share/dbus-1/interfaces/`.

Use Grep to search for specific interface names, method names, or signal names. For example:

- To find an interface: `Grep` for `<interface name="org.freedesktop.portal.Notification"` in the relevant interface file.

## Icons

GNOME icons are stored in `/usr/share/icons/`. This directory contains icon themes like Adwaita, with icons in various sizes and formats.

Use file listing tools to browse available icons by theme and size. For example:

- List themes: `ls /usr/share/icons/`
- List icons in a theme: `ls /usr/share/icons/Adwaita/`

Remember to use symbolic-icons for UI elements, unless showing the icon of a specific app