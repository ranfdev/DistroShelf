/// Terminal color scheme utilities for VTE4 with libadwaita theme support
/// Provides ANSI color palettes that respect the system light/dark theme preference
use gtk::gdk;
use vte4::prelude::*;

/// ANSI color palette entry
#[derive(Clone, Copy)]
pub struct ColorPalette {
    /// Foreground text color
    pub foreground: gdk::RGBA,
    /// Background color
    pub background: gdk::RGBA,
    /// Standard ANSI colors (0-15)
    pub palette: [gdk::RGBA; 16],
}

impl ColorPalette {
    /// Create a color palette from RGB components
    fn color(r: f32, g: f32, b: f32) -> gdk::RGBA {
        gdk::RGBA::new(r, g, b, 1.0)
    }

    /// Get the dark theme color palette (similar to GNOME Terminal/Ptyxis dark)
    /// This palette respects libadwaita's dark theme colors
    pub fn dark() -> Self {
        Self {
            // Adwaita dark: text on dark background
            foreground: Self::color(0.92, 0.92, 0.92), // #ebebeb
            background: Self::color(0.1, 0.1, 0.1),    // #1a1a1a
            palette: [
                // Standard colors (0-7)
                Self::color(0.2, 0.2, 0.2), // 0: black (darker than bg for contrast)
                Self::color(0.89, 0.35, 0.36), // 1: red
                Self::color(0.37, 0.76, 0.36), // 2: green
                Self::color(0.87, 0.75, 0.29), // 3: yellow
                Self::color(0.36, 0.62, 0.89), // 4: blue
                Self::color(0.76, 0.51, 0.85), // 5: magenta
                Self::color(0.36, 0.78, 0.85), // 6: cyan
                Self::color(0.82, 0.82, 0.82), // 7: white (lighter)
                // Bright colors (8-15)
                Self::color(0.5, 0.5, 0.5),    // 8: bright black (gray)
                Self::color(1.0, 0.55, 0.56),  // 9: bright red
                Self::color(0.56, 0.93, 0.56), // 10: bright green
                Self::color(1.0, 0.93, 0.56),  // 11: bright yellow
                Self::color(0.56, 0.8, 1.0),   // 12: bright blue
                Self::color(0.94, 0.71, 1.0),  // 13: bright magenta
                Self::color(0.56, 0.96, 1.0),  // 14: bright cyan
                Self::color(1.0, 1.0, 1.0),    // 15: bright white
            ],
        }
    }

    /// Get the light theme color palette (similar to GNOME Terminal/Ptyxis light)
    /// This palette respects libadwaita's light theme colors
    pub fn light() -> Self {
        Self {
            // Adwaita light: dark text on light background
            foreground: Self::color(0.2, 0.2, 0.2), // #333333
            background: Self::color(0.98, 0.98, 0.98), // #fafafa (nearly white)
            palette: [
                // Standard colors (0-7)
                Self::color(0.2, 0.2, 0.2),    // 0: black
                Self::color(0.8, 0.0, 0.0),    // 1: red
                Self::color(0.0, 0.6, 0.0),    // 2: green
                Self::color(0.8, 0.62, 0.0),   // 3: yellow
                Self::color(0.13, 0.34, 0.76), // 4: blue
                Self::color(0.76, 0.27, 0.76), // 5: magenta
                Self::color(0.0, 0.6, 0.76),   // 6: cyan
                Self::color(0.7, 0.7, 0.7),    // 7: white (gray)
                // Bright colors (8-15)
                Self::color(0.5, 0.5, 0.5),    // 8: bright black (gray)
                Self::color(1.0, 0.0, 0.0),    // 9: bright red
                Self::color(0.0, 1.0, 0.0),    // 10: bright green
                Self::color(1.0, 1.0, 0.0),    // 11: bright yellow
                Self::color(0.0, 0.0, 1.0),    // 12: bright blue
                Self::color(1.0, 0.0, 1.0),    // 13: bright magenta
                Self::color(0.0, 1.0, 1.0),    // 14: bright cyan
                Self::color(0.99, 0.99, 0.99), // 15: bright white
            ],
        }
    }

    /// Get the appropriate palette based on the current adwaita theme
    /// Checks the system style manager to determine if dark or light theme is active
    pub fn current() -> Self {
        let style_manager = adw::StyleManager::default();
        if style_manager.is_dark() {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// Apply this color palette to a VTE terminal
    pub fn apply_to_terminal(&self, terminal: &vte4::Terminal) {
        let palette_refs: Vec<&gdk::RGBA> = self.palette.iter().collect();
        terminal.set_colors(
            Some(&self.foreground),
            Some(&self.background),
            &palette_refs,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_palette() {
        let palette = ColorPalette::dark();
        assert_ne!(palette.foreground, palette.background);
        assert_eq!(palette.palette.len(), 16);
    }

    #[test]
    fn test_light_palette() {
        let palette = ColorPalette::light();
        assert_ne!(palette.foreground, palette.background);
        assert_eq!(palette.palette.len(), 16);
    }
}
