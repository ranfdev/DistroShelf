#[derive(Debug, Default, Clone)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub icon: String,
}

pub fn parse_desktop_file(content: &str) -> anyhow::Result<DesktopEntry> {
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut in_desktop_entry = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }

        if !in_desktop_entry || !trimmed.contains('=') {
            continue;
        }

        let mut parts = trimmed.splitn(2, '=');
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            match key.trim() {
                "Name" => name = Some(value.trim().to_string()),
                "Exec" => exec = Some(value.trim().to_string()),
                "Icon" => icon = Some(value.trim().to_string()),
                _ => {}
            }
        }

        if name.is_some() && exec.is_some() && icon.is_some() {
            break; // Exit early if we have all required fields
        }
    }

    let name = name.ok_or_else(|| anyhow::anyhow!("Missing Name key"))?;
    let exec = exec.ok_or_else(|| anyhow::anyhow!("Missing Exec key"))?;
    let icon = icon.unwrap_or_default();

    Ok(DesktopEntry {
        name,
        icon,
        exec,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_desktop_entry() {
        let content = r#"
[Desktop Entry]
Name=Firefox
Exec=/usr/bin/firefox %u
Icon=firefox
        "#;
        let entry = parse_desktop_file(content).unwrap();
        assert_eq!(&entry.name, "Firefox");
        assert_eq!(&entry.exec, "/usr/bin/firefox %u");
        assert_eq!(&entry.icon, "firefox");
    }

    #[test]
    fn test_missing_desktop_entry_section() {
        let content = r#"
[Some Other Section]
Name=Firefox
Exec=/usr/bin/firefox %u
Icon=firefox
        "#;
        let result = parse_desktop_file(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_required_fields() {
        let content = r#"
[Desktop Entry]
Icon=firefox
        "#;
        let result = parse_desktop_file(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_sections() {
        let content = r#"
[Desktop Action NewWindow]
Name=New Window
Exec=firefox --new-window

[Desktop Entry]
Name=Firefox
Exec=/usr/bin/firefox
Icon=firefox

[Desktop Action NewPrivateWindow]
Name=New Private Window
Exec=firefox --private-window
        "#;
        let entry = parse_desktop_file(content).unwrap();
        assert_eq!(&entry.name, "Firefox");
        assert_eq!(&entry.exec, "/usr/bin/firefox");
    }

    #[test]
    fn test_fields_with_equals_in_value() {
        let content = r#"
[Desktop Entry]
Name=Test=App
Exec=/usr/bin/test --param=value
Icon=test-icon
        "#;
        let entry = parse_desktop_file(content).unwrap();
        assert_eq!(&entry.name, "Test=App");
        assert_eq!(&entry.exec, "/usr/bin/test --param=value");
    }
}
