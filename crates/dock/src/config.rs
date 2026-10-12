//! The part of `~/.config/coquille/config.toml` the dock reads, and how it
//! writes back what the user changes with the mouse (favorites, theme)
//! without touching the rest of the file, comments included.

use std::path::{Path, PathBuf};

use coquille_ipc::ThemeName;
use toml_edit::{Array, DocumentMut, Item, Table, Value};

/// What the dock reads from the configuration. Anything else in the file
/// belongs to the compositor and is ignored here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub theme: Option<ThemeName>,
    /// `#rrggbb`.
    pub accent: Option<String>,
    /// The applications of the bar, in order.
    pub favorites: Vec<String>,
    /// The pinned applications of the drawer; the favorites when absent.
    pub pinned: Vec<String>,
    /// The width the user gave the drawer by dragging its edge.
    pub drawer_width: Option<u32>,
}

/// Reads the configuration. A file that is not valid TOML gives the
/// defaults: the compositor is the one that reports it.
pub fn parse(text: &str) -> Config {
    let Ok(document) = text.parse::<DocumentMut>() else {
        return Config::default();
    };
    let names = |section: &str, key: &str| -> Option<Vec<String>> {
        let array = document.get(section)?.get(key)?.as_array()?;
        let names = array.iter().filter_map(|name| name.as_str());
        Some(names.map(str::to_owned).collect())
    };
    let text_of = |key: &str| document.get(key).and_then(Item::as_str);
    let favorites = names("dock", "favorites").unwrap_or_default();
    Config {
        theme: match text_of("theme") {
            Some("dark") => Some(ThemeName::Dark),
            Some("light") => Some(ThemeName::Light),
            _ => None,
        },
        accent: text_of("accent").map(str::to_owned),
        pinned: names("drawer", "pinned").unwrap_or_else(|| favorites.clone()),
        drawer_width: document
            .get("drawer")
            .and_then(|drawer| drawer.get("width")?.as_integer())
            .and_then(|width| u32::try_from(width).ok()),
        favorites,
    }
}

/// The file with other favorites; everything else is kept as it was.
/// `None` when the file is not valid TOML: it is then left alone.
pub fn with_favorites(text: &str, favorites: &[String]) -> Option<String> {
    with_names(text, "dock", "favorites", favorites)
}

/// The file with another width for the drawer.
pub fn with_drawer_width(text: &str, width: u32) -> Option<String> {
    with_value(text, "drawer", "width", Value::from(i64::from(width)))
}

fn with_names(text: &str, section: &str, key: &str, names: &[String]) -> Option<String> {
    let array: Array = names.iter().map(String::as_str).collect();
    with_value(text, section, key, Value::Array(array))
}

/// The file with another value for a setting of a section, which is added
/// at the end when the file has none.
fn with_value(text: &str, section: &str, key: &str, new: Value) -> Option<String> {
    let mut document = text.parse::<DocumentMut>().ok()?;
    let table = document
        .entry(section)
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()?;
    set(table, key, new);
    Some(document.to_string())
}

/// Gives a setting another value. The comments written above it and at the
/// end of its line stay where they are.
fn set(table: &mut Table, key: &str, mut new: Value) {
    match table.get_mut(key).and_then(Item::as_value_mut) {
        Some(old) => {
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        None => {
            table.insert(key, Item::Value(new));
        }
    }
}

/// The file with another theme.
pub fn with_theme(text: &str, theme: ThemeName) -> Option<String> {
    let mut document = text.parse::<DocumentMut>().ok()?;
    let name = match theme {
        ThemeName::Light => "light",
        ThemeName::Dark => "dark",
    };
    set(document.as_table_mut(), "theme", Value::from(name));
    Some(document.to_string())
}

/// Where the configuration is, unless a path is given.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?;
    Some(base.join("coquille/config.toml"))
}

pub fn load(path: &Path) -> Config {
    parse(&std::fs::read_to_string(path).unwrap_or_default())
}

/// Rewrites the file through `change`, which gets its text and returns the
/// new one. A file that cannot be read as TOML is not touched.
pub fn update(path: &Path, change: impl Fn(&str) -> Option<String>) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let Some(changed) = change(&text) else {
        return Err(std::io::Error::other("the configuration is not valid TOML"));
    };
    if changed == text {
        return Ok(());
    }
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)?;
    }
    // Written next to the file, then moved over it: never half a file.
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, changed)?;
    std::fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r##"# Ma configuration.
theme = "dark"          # sombre le soir
accent = "#7aa7ff"

[outputs]
left = "HDMI-A-1"

[dock]
# Mes applis de tous les jours.
favorites = ["firefox", "discord"]

[drawer]
pinned = ["firefox", "mpv"]
"##;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn the_settings_of_the_dock_are_read() {
        let config = parse(FILE);

        assert_eq!(config.theme, Some(ThemeName::Dark));
        assert_eq!(config.accent.as_deref(), Some("#7aa7ff"));
        assert_eq!(config.favorites, ["firefox", "discord"]);
        assert_eq!(config.pinned, ["firefox", "mpv"]);
    }

    #[test]
    fn the_sample_file_of_the_project_gives_favorites() {
        let config = parse(include_str!("../../../config/config.toml"));

        assert_eq!(
            config.favorites.first().map(String::as_str),
            Some("firefox")
        );
        // No pinned application of its own: the drawer shows the favorites.
        assert_eq!(config.pinned, config.favorites);
        assert_eq!(config.drawer_width, None);
    }

    #[test]
    fn the_width_of_the_drawer_is_read() {
        let config = parse("[drawer]\nwidth = 720\n");

        assert_eq!(config.drawer_width, Some(720));
        assert_eq!(parse(FILE).drawer_width, None);
        assert_eq!(parse("[drawer]\nwidth = \"wide\"\n").drawer_width, None);
        assert_eq!(parse("[drawer]\nwidth = -3\n").drawer_width, None);
    }

    #[test]
    fn resizing_the_drawer_keeps_the_rest_of_the_file() {
        let changed = with_drawer_width(FILE, 812).unwrap();

        assert_eq!(parse(&changed).drawer_width, Some(812));
        assert_eq!(parse(&changed).pinned, ["firefox", "mpv"]);
        assert!(changed.contains("# Mes applis de tous les jours."));
    }

    #[test]
    fn the_width_of_the_drawer_is_added_to_a_file_without_drawer_section() {
        let changed = with_drawer_width("theme = \"dark\"\n", 700).unwrap();

        assert_eq!(parse(&changed).drawer_width, Some(700));
        assert_eq!(parse(&changed).theme, Some(ThemeName::Dark));
        assert_eq!(with_drawer_width("= 3", 700), None);
    }

    #[test]
    fn an_empty_file_gives_an_empty_dock() {
        assert_eq!(parse(""), Config::default());
    }

    #[test]
    fn without_pinned_applications_the_drawer_shows_the_favorites() {
        let config = parse("[dock]\nfavorites = [\"kitty\"]\n");

        assert_eq!(config.pinned, ["kitty"]);
    }

    #[test]
    fn a_broken_file_gives_the_defaults() {
        assert_eq!(parse("theme = \n[dock"), Config::default());
    }

    #[test]
    fn what_is_not_a_name_is_skipped() {
        let config = parse("theme = 3\n[dock]\nfavorites = [\"kitty\", 4, \"mpv\"]\n");

        assert_eq!(config.theme, None);
        assert_eq!(config.favorites, ["kitty", "mpv"]);
    }

    #[test]
    fn changing_the_favorites_keeps_the_rest_of_the_file() {
        let changed = with_favorites(FILE, &names(&["discord", "kitty", "firefox"])).unwrap();

        assert_eq!(parse(&changed).favorites, ["discord", "kitty", "firefox"]);
        assert_eq!(parse(&changed).pinned, ["firefox", "mpv"]);
        for kept in [
            "# Ma configuration.",
            "# sombre le soir",
            "# Mes applis de tous les jours.",
            "left = \"HDMI-A-1\"",
        ] {
            assert!(changed.contains(kept), "lost `{kept}` in:\n{changed}");
        }
    }

    #[test]
    fn favorites_are_added_to_a_file_that_had_none() {
        let changed = with_favorites("theme = \"light\"\n", &names(&["kitty"])).unwrap();

        assert_eq!(parse(&changed).favorites, ["kitty"]);
        assert_eq!(parse(&changed).theme, Some(ThemeName::Light));
    }

    #[test]
    fn changing_the_theme_keeps_its_comment_and_its_place() {
        let changed = with_theme(FILE, ThemeName::Light).unwrap();

        assert_eq!(parse(&changed).theme, Some(ThemeName::Light));
        assert!(changed.starts_with("# Ma configuration.\ntheme = \"light\""));
        assert!(changed.contains("[dock]"));
    }

    #[test]
    fn a_theme_is_added_before_the_sections() {
        let changed = with_theme("[outputs]\nleft = \"DP-1\"\n", ThemeName::Dark).unwrap();

        // A general setting written after a section would belong to it.
        assert!(changed.find("theme").unwrap() < changed.find("[outputs]").unwrap());
        assert_eq!(parse(&changed).theme, Some(ThemeName::Dark));
    }

    #[test]
    fn a_broken_file_is_never_rewritten() {
        assert_eq!(with_favorites("theme = \n[dock", &names(&["kitty"])), None);
        assert_eq!(with_theme("= 3", ThemeName::Dark), None);
    }
}
