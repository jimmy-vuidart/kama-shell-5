//! Light or dark for the applications: what GTK, libadwaita, Qt and the
//! portals read, written where they read it. GNOME settings carry the color
//! scheme (relayed by the gtk portal to libadwaita, Firefox and Electron) and
//! the GTK theme (read by GTK and by Qt's gtk3 platform theme); the
//! `settings.ini` of GTK serve X11 applications, and must not hold a dark
//! preference left by another desktop.

use std::path::{Path, PathBuf};

use coquille_theme::ThemeKind;

use crate::catalog::data_directories;

const SCHEMA: &str = "org.gnome.desktop.interface";
/// The GTK themes that look like libadwaita, used when the theme of the user
/// has no variant for the other brightness.
const ADW_GTK3: &str = "adw-gtk3";
/// Built into GTK 3, in both brightnesses: always there.
const ADWAITA: &str = "Adwaita";

/// Tells the applications the theme of the shell. Blocking (it runs
/// `gsettings` and writes files): to call off the compositor's thread.
pub fn tell_applications(kind: ThemeKind) {
    let dark = kind == ThemeKind::Dark;
    let scheme = if dark { "prefer-dark" } else { "prefer-light" };
    set_setting("color-scheme", scheme);

    let current = get_setting("gtk-theme");
    let gtk_theme = gtk_theme_for(current.as_deref(), dark, &|name| gtk_installed(name));
    set_setting("gtk-theme", &gtk_theme);

    // An icon theme in two brightnesses follows the theme; any other one is
    // left as the user chose it.
    let icon_theme = get_setting("icon-theme")
        .and_then(|current| variant(&current, dark, &|name| icons_installed(name)));
    if let Some(icon_theme) = &icon_theme {
        set_setting("icon-theme", icon_theme);
    }

    let mut values = vec![
        // The theme carries the brightness: a preference here would keep
        // running GTK 3 applications dark, as they read this file only once.
        ("gtk-application-prefer-dark-theme", "false"),
        ("gtk-theme-name", gtk_theme.as_str()),
    ];
    if let Some(icon_theme) = &icon_theme {
        values.push(("gtk-icon-theme-name", icon_theme));
    }
    for version in ["gtk-3.0", "gtk-4.0"] {
        if let Some(path) = config_home().map(|home| home.join(version).join("settings.ini")) {
            if let Err(error) = write_settings_ini(&path, &values) {
                tracing::warn!(path = %path.display(), %error, "cannot tell GTK the theme");
            }
        }
    }
    tracing::info!(
        ?kind,
        gtk_theme,
        ?icon_theme,
        "theme given to the applications"
    );
}

/// The GTK theme for a brightness: the variant of the user's own theme,
/// else adw-gtk3, else Adwaita.
fn gtk_theme_for(current: Option<&str>, dark: bool, installed: &dyn Fn(&str) -> bool) -> String {
    current
        .and_then(|name| variant(name, dark, installed))
        .or_else(|| variant(ADW_GTK3, dark, installed))
        .unwrap_or_else(|| {
            let builtin = |name: &str| name == ADWAITA || name == "Adwaita-dark";
            variant(ADWAITA, dark, &builtin).unwrap_or_else(|| ADWAITA.to_owned())
        })
}

/// The theme of the same family in the wanted brightness, when installed:
/// `Breeze` and `Breeze-Dark`, `Reversal-purple` and `Reversal-purple-dark`.
fn variant(name: &str, dark: bool, installed: &dyn Fn(&str) -> bool) -> Option<String> {
    let base = strip_dark(name);
    let candidates = if dark {
        vec![format!("{base}-dark"), format!("{base}-Dark")]
    } else {
        vec![base.to_owned()]
    };
    candidates
        .into_iter()
        .find(|candidate| installed(candidate))
}

fn strip_dark(name: &str) -> &str {
    let cut = name.len().saturating_sub("-dark".len());
    match name.get(cut..) {
        Some(suffix) if suffix.eq_ignore_ascii_case("-dark") => &name[..cut],
        _ => name,
    }
}

/// A GTK theme with styles for GTK 3, in one of the directories GTK reads.
fn gtk_installed(name: &str) -> bool {
    theme_directories("themes", ".themes")
        .iter()
        .any(|directory| directory.join(name).join("gtk-3.0").is_dir())
}

fn icons_installed(name: &str) -> bool {
    theme_directories("icons", ".icons")
        .iter()
        .any(|directory| directory.join(name).join("index.theme").is_file())
}

fn theme_directories(kind: &str, legacy: &str) -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(legacy));
    let data = data_directories().into_iter().map(|data| data.join(kind));
    home.into_iter().chain(data).collect()
}

fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))
}

fn get_setting(key: &str) -> Option<String> {
    let output = std::process::Command::new("gsettings")
        .args(["get", SCHEMA, key])
        .output()
        .ok()?;
    let answer = String::from_utf8(output.stdout).ok()?;
    let name = answer.trim().trim_matches('\'');
    (output.status.success() && !name.is_empty()).then(|| name.to_owned())
}

fn set_setting(key: &str, value: &str) {
    let status = std::process::Command::new("gsettings")
        .args(["set", SCHEMA, key, value])
        .status();
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => tracing::warn!(key, value, %status, "cannot change a GNOME setting"),
        Err(error) => tracing::warn!(key, value, %error, "cannot run gsettings"),
    }
}

/// Gives `values` to the `[Settings]` group of a GTK `settings.ini`, which
/// is created if needed; everything else in it stays.
fn write_settings_ini(path: &Path, values: &[(&str, &str)]) -> std::io::Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let changed = with_settings(&text, values);
    if changed == text {
        return Ok(());
    }
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)?;
    }
    // Written beside, then moved: an application starting meanwhile reads
    // the old file or the new one, never half of one.
    let temporary = path.with_extension("ini.coquille");
    std::fs::write(&temporary, changed)?;
    std::fs::rename(&temporary, path)
}

/// `text` with each key of `values` set in its `[Settings]` group: replaced
/// where it is, added at the end of the group otherwise.
fn with_settings(text: &str, values: &[(&str, &str)]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    let group = lines.iter().position(|line| line.trim() == "[Settings]");
    let start = match group {
        Some(index) => index + 1,
        None => {
            if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                lines.push(String::new());
            }
            lines.push("[Settings]".to_owned());
            lines.len()
        }
    };
    let mut end = lines[start..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map_or(lines.len(), |offset| start + offset);
    for (key, value) in values {
        let line = format!("{key}={value}");
        let found = lines[start..end]
            .iter()
            .position(|existing| existing.split('=').next().map(str::trim) == Some(key));
        match found {
            Some(offset) => lines[start + offset] = line,
            None => {
                // After the last setting, before the blank lines that end
                // the group.
                let mut at = end;
                while at > start && lines[at - 1].trim().is_empty() {
                    at -= 1;
                }
                lines.insert(at, line);
                end += 1;
            }
        }
    }
    let mut changed = lines.join("\n");
    changed.push('\n');
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn among<'a>(names: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
        move |name| names.contains(&name)
    }

    #[test]
    fn a_theme_finds_its_other_brightness() {
        let installed = among(&[
            "Breeze",
            "Breeze-Dark",
            "Reversal-purple",
            "Reversal-purple-dark",
        ]);

        assert_eq!(
            variant("Breeze", true, &installed).as_deref(),
            Some("Breeze-Dark")
        );
        assert_eq!(
            variant("Breeze-Dark", false, &installed).as_deref(),
            Some("Breeze")
        );
        assert_eq!(
            variant("Reversal-purple-dark", false, &installed).as_deref(),
            Some("Reversal-purple")
        );
        assert_eq!(
            variant("Reversal-purple", true, &installed).as_deref(),
            Some("Reversal-purple-dark")
        );
        assert_eq!(
            variant("Breeze-Dark", true, &installed).as_deref(),
            Some("Breeze-Dark")
        );
    }

    #[test]
    fn a_theme_in_one_brightness_has_no_variant() {
        let installed = among(&["Adwaita", "deepin-dark"]);

        assert_eq!(variant("Adwaita", true, &installed), None);
        assert_eq!(variant("deepin-dark", false, &installed), None);
    }

    #[test]
    fn the_gtk_theme_falls_back_on_adw_gtk3_then_adwaita() {
        let with_adw = among(&["adw-gtk3", "adw-gtk3-dark", "deepin-dark"]);
        assert_eq!(
            gtk_theme_for(Some("deepin-dark"), false, &with_adw),
            "adw-gtk3"
        );
        assert_eq!(gtk_theme_for(None, true, &with_adw), "adw-gtk3-dark");

        let nothing = among(&[]);
        assert_eq!(
            gtk_theme_for(Some("deepin-dark"), true, &nothing),
            "Adwaita-dark"
        );
        assert_eq!(gtk_theme_for(None, false, &nothing), "Adwaita");
    }

    #[test]
    fn settings_are_replaced_where_they_are() {
        let text = "[Settings]\ngtk-application-prefer-dark-theme=true\ngtk-modules=appmenu\n\
                    gtk-theme-name = deepin-dark\n";

        let changed = with_settings(
            text,
            &[
                ("gtk-application-prefer-dark-theme", "false"),
                ("gtk-theme-name", "adw-gtk3"),
            ],
        );

        assert_eq!(
            changed,
            "[Settings]\ngtk-application-prefer-dark-theme=false\ngtk-modules=appmenu\n\
             gtk-theme-name=adw-gtk3\n"
        );
    }

    #[test]
    fn missing_settings_end_their_group() {
        let text = "[Settings]\ngtk-modules=appmenu\n\n[Other]\ngtk-theme-name=kept\n";

        let changed = with_settings(text, &[("gtk-theme-name", "adw-gtk3")]);

        assert_eq!(
            changed,
            "[Settings]\ngtk-modules=appmenu\ngtk-theme-name=adw-gtk3\n\n[Other]\n\
             gtk-theme-name=kept\n"
        );
    }

    #[test]
    fn a_file_without_the_group_receives_it() {
        assert_eq!(
            with_settings("", &[("gtk-theme-name", "Adwaita")]),
            "[Settings]\ngtk-theme-name=Adwaita\n"
        );
        assert_eq!(
            with_settings("# kept\n", &[("gtk-theme-name", "Adwaita")]),
            "# kept\n\n[Settings]\ngtk-theme-name=Adwaita\n"
        );
    }

    #[test]
    fn a_file_already_right_is_left_as_it_is() {
        let text = "[Settings]\ngtk-theme-name=adw-gtk3\n";

        assert_eq!(with_settings(text, &[("gtk-theme-name", "adw-gtk3")]), text);
    }
}
