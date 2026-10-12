//! Where the icon of an application is, in the icon theme of the system.

use std::path::{Path, PathBuf};

/// The icon theme of the user, as GNOME settings name it; Adwaita when
/// they do not say.
pub fn icon_theme() -> String {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .output();
    let answer = output
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_default();
    theme_name(&answer).unwrap_or_else(|| "Adwaita".to_owned())
}

/// The name in an answer of `gsettings get` (`'Adwaita'`, with its quotes).
fn theme_name(answer: &str) -> Option<String> {
    let name = answer.trim().trim_matches('\'');
    (!name.is_empty()).then(|| name.to_owned())
}

/// The file of an icon, as a desktop entry names it: a path as it is, a
/// name through `theme`, then through the theme every other one falls back
/// on. This reads directories: not something to do for each frame.
pub fn find_icon(name: &str, theme: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let as_path = Path::new(name);
    if as_path.is_absolute() {
        return as_path.is_file().then(|| as_path.to_owned());
    }
    let lookup = |theme: &str| {
        freedesktop_icons::lookup(name)
            .with_theme(theme)
            .with_size(48)
            .with_cache()
            .find()
    };
    lookup(theme).or_else(|| lookup("hicolor"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_of_the_theme_is_read_without_its_quotes() {
        assert_eq!(
            theme_name("'Papirus-Dark'\n").as_deref(),
            Some("Papirus-Dark")
        );
        assert_eq!(theme_name("Adwaita").as_deref(), Some("Adwaita"));
        assert_eq!(theme_name("''\n"), None);
        assert_eq!(theme_name(""), None);
    }

    #[test]
    fn an_icon_given_as_a_path_is_that_file_when_it_exists() {
        let file = std::env::temp_dir().join(format!("coquille-icon-{}.png", std::process::id()));
        std::fs::write(&file, b"png").unwrap();
        let path = file.to_string_lossy().into_owned();

        assert_eq!(find_icon(&path, "Adwaita"), Some(file.clone()));
        std::fs::remove_file(&file).unwrap();
        assert_eq!(find_icon(&path, "Adwaita"), None);
    }

    #[test]
    fn no_name_gives_no_icon() {
        assert_eq!(find_icon("", "Adwaita"), None);
    }
}
