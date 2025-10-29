# Flatpak Terminal Discovery

## Overview

DistroShelf automatically discovers and supports terminal emulators installed via Flatpak. This allows users to seamlessly use Flatpak-installed terminals for opening container shells without manual configuration.

## How It Works

### Discovery Process

1. **Querying Installed Apps**: When the preferences dialog opens, DistroShelf queries the system for installed Flatpak applications using:
   ```bash
   flatpak list --app --columns=application
   ```

2. **Matching Known Terminals**: The application ID list is compared against a curated list of known terminal emulator Flatpak IDs (e.g., `app.devsuite.Ptyxis`, `org.gnome.Console`).

3. **Extracting Command Info**: For each matched terminal, DistroShelf runs:
   ```bash
   flatpak info <app-id>
   ```
   This provides metadata including the command that the Flatpak executes (e.g., `/app/bin/ptyxis`).

4. **Creating Terminal Entries**: Discovered terminals are added to the terminal list with:
   - Name: Base terminal name with variant suffix (e.g., "Ptyxis (Flatpak)")
   - Program: Flatpak run command (e.g., `flatpak run app.devsuite.Ptyxis`)
   - Separator arg: The command separator for that terminal (e.g., `--` or `-e`)
   - Read-only: Always `true` for auto-discovered terminals

### Variant Detection

The system detects two types of variants:

- **Standard Flatpak**: Applications without `.Devel` suffix
  - Displayed as: `<Terminal Name> (Flatpak)`
  - Example: `Ptyxis (Flatpak)`

- **Developer Versions**: Applications with `.Devel` suffix
  - Displayed as: `<Terminal Name> (Flatpak, Devel)`
  - Example: `Ptyxis (Flatpak, Devel)`

This allows users to distinguish between stable and development versions when both are installed.

## Supported Flatpak Terminals

The following terminal Flatpak app IDs are supported:

| App ID | Terminal Name | Separator |
|--------|--------------|-----------|
| `org.gnome.Console` | GNOME Console | `--` |
| `org.gnome.Console.Devel` | GNOME Console | `--` |
| `org.gnome.Terminal` | GNOME Terminal | `--` |
| `org.kde.konsole` | Konsole | `-e` |
| `org.xfce.Terminal` | Xfce Terminal | `-x` |
| `com.gexperts.Tilix` | Tilix | `-e` |
| `io.github.kovidgoyal.kitty` | Kitty | `--` |
| `io.alacritty.Alacritty` | Alacritty | `-e` |
| `org.wezfurlong.wezterm` | WezTerm | `-e` |
| `io.elementary.terminal` | elementary Terminal | `--` |
| `app.devsuite.Ptyxis` | Ptyxis | `--` |
| `app.devsuite.Ptyxis.Devel` | Ptyxis | `--` |
| `org.codeberg.dnkl.foot` | Foot | `-e` |
| `com.system76.CosmicTerm` | COSMIC Terminal | `-e` |
| `com.mitchellh.ghostty` | Ghostty | `-e` |
| `com.gexperts.Terminator` | Terminator | `-x` |
| `org.lxqt.QTerminal` | QTerminal | `-e` |

## Adding New Terminal Support

To add support for a new Flatpak terminal:

1. Open `src/supported_terminals.rs`
2. Add the terminal's Flatpak app ID to the `FLATPAK_TERMINAL_MAPPINGS` static:
   ```rust
   ("com.example.Terminal", "Example Terminal", "--"),
   ```
3. The format is: `(app_id, display_name, separator_argument)`

## Technical Implementation

### Key Components

- **`FLATPAK_TERMINAL_MAPPINGS`**: Static list of known terminal Flatpak app IDs and their metadata
- **`discover_flatpak_terminals()`**: Async method that performs the discovery process
- **`get_flatpak_command()`**: Extracts the command from flatpak info output
- **`reload_with_flatpak_discovery()`**: Public method to trigger discovery and reload the terminal list

### Command Runner Pattern

The implementation uses the `CommandRunner` abstraction, which allows:
- **Production**: Real commands executed via `flatpak` CLI
- **Testing**: Mocked responses using `NullCommandRunnerBuilder`

This pattern enables comprehensive unit testing without requiring Flatpak to be installed.

## User Experience

### In Preferences Dialog

When users open the Preferences dialog:
1. The terminal list is reloaded asynchronously
2. Flatpak discovery runs in the background
3. The UI updates with discovered terminals
4. Users see both system and Flatpak variants side-by-side

Example terminal list:
```
- Alacritty
- GNOME Console (Flatpak)
- Konsole
- Ptyxis
- Ptyxis (Flatpak)
- Ptyxis (Flatpak, Devel)
- Xterm
```

### Selecting Flatpak Terminals

Users can select Flatpak terminals just like system terminals. When selected, DistroShelf will use the command:
```bash
flatpak run <app-id> -- <container-command>
```

This ensures the terminal runs with proper Flatpak sandboxing while still opening the distrobox container shell.

## Troubleshooting

### Flatpak Terminal Not Detected

If a Flatpak terminal is installed but not showing up:

1. Verify it's installed: `flatpak list --app | grep -i terminal`
2. Check if the app ID is in `FLATPAK_TERMINAL_MAPPINGS`
3. Ensure `flatpak info <app-id>` returns valid output
4. Check DistroShelf logs for any error messages

### Performance Considerations

- Discovery runs asynchronously to avoid blocking the UI
- Results are cached in memory until the next reload
- The discovery process typically completes in under a second
- Failed `flatpak` commands are gracefully handled and logged
