//! The part of `~/.config/coquille/config.toml` the dock reads, and what it
//! writes back when the user changes it with the mouse (favorites, theme,
//! size of the launcher). The `coquille-config` crate does the writing,
//! which leaves the rest of the file as it was, comments included.

use std::collections::BTreeMap;
use std::path::Path;

use coquille_config::{
    Array, Change, DockPlace, PerScreen, Settings, ShellMode, Value, apply, docks_for_mode,
    with_value,
};
pub use coquille_config::{default_path, update};
use coquille_ipc::ThemeName;
use coquille_model::OutputRole;
use toml_edit::{DocumentMut, Item};

use crate::model::{Orientation, Side};

/// What the dock reads from the configuration. Anything else in the file
/// belongs to the compositor and is ignored here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub theme: Option<ThemeName>,
    /// `#rrggbb`.
    pub accent: Option<String>,
    /// The applications of the bar, in order.
    pub favorites: Vec<String>,
    /// What the file says of the dock of each screen; `None` where it says
    /// nothing. [`Config::docks`] tells which screens have one.
    pub places: PerScreen<Option<DockPlace>>,
    /// The pinned applications of the drawer; the favorites when absent.
    pub pinned: Vec<String>,
    /// The width the user gave the drawer by dragging its edge.
    pub drawer_width: Option<u32>,
    /// The height the user gave the horizontal launcher.
    pub drawer_height: Option<u32>,
    /// The icon size of the dock bar.
    pub icon_size: Option<u32>,
    pub audio_hidden: Vec<String>,
    pub audio_nicks: BTreeMap<String, String>,
    /// Whether the dock moves (`[animations] enabled`), as the compositor.
    pub animations: bool,
    /// Desktop mode: classic columns + floating dock, or spatial maximized + vertical sidebar.
    pub mode: ShellMode,
    /// API token for the spatial launcher's daily tasks widget (`[spatial] tasks_token`).
    pub tasks_token: Option<String>,
}

impl Config {
    pub fn icon_size(&self) -> u32 {
        self.icon_size.unwrap_or(crate::ui::bar::DEFAULT_ICON_SIZE)
    }

    pub fn is_audio_sink_visible(&self, name: &str) -> bool {
        !self.audio_hidden.iter().any(|h| h == name)
    }

    pub fn audio_sink_nick<'a>(&'a self, name: &str) -> Option<&'a str> {
        self.audio_nicks.get(name).map(String::as_str)
    }

    /// The screens that have a dock, and which way each lies, when these
    /// screens are `present` (all three when none is known yet).
    #[cfg(test)]
    pub fn docks(&self, present: &[OutputRole]) -> Vec<(OutputRole, Orientation)> {
        self.docks_in_mode(present, self.mode)
    }

    /// Like [`Self::docks`], using an explicit [`ShellMode`] (e.g. when the
    /// compositor overrides the file via `--spatial`).
    pub fn docks_in_mode(
        &self,
        present: &[OutputRole],
        mode: ShellMode,
    ) -> Vec<(OutputRole, Orientation)> {
        let places = docks_for_mode(&self.places, present, mode);
        PerScreen::<()>::ROLES
            .into_iter()
            .filter(|role| present.is_empty() || present.contains(role))
            .filter_map(|role| {
                let orientation = match *places.get(role) {
                    DockPlace::Vertical => Orientation::Vertical(Side::Left),
                    DockPlace::VerticalRight => Orientation::Vertical(Side::Right),
                    DockPlace::Horizontal => Orientation::Horizontal,
                    DockPlace::Off => return None,
                };
                Some((role, orientation))
            })
            .collect()
    }
}

/// Reads the configuration. A file that is not valid TOML gives the
/// defaults: the compositor is the one that reports it.
pub fn parse(text: &str) -> Config {
    let Ok(document) = text.parse::<DocumentMut>() else {
        return Config {
            animations: true,
            ..Config::default()
        };
    };
    let names = |section: &str, key: &str| -> Option<Vec<String>> {
        let array = document.get(section)?.get(key)?.as_array()?;
        let names = array.iter().filter_map(|name| name.as_str());
        Some(names.map(str::to_owned).collect())
    };
    let text_of = |key: &str| document.get(key).and_then(Item::as_str);
    let length = |key: &str| {
        let value = document.get("drawer")?.get(key)?.as_integer()?;
        u32::try_from(value).ok()
    };
    let dock_int = |key: &str| {
        let value = document.get("dock")?.get(key)?.as_integer()?;
        u32::try_from(value).ok()
    };
    let settings = Settings::parse(text);
    let favorites = names("dock", "favorites").unwrap_or_default();
    let audio_hidden = names("audio", "hidden").unwrap_or_default();
    let audio_nicks = document
        .get("audio")
        .and_then(Item::as_table)
        .and_then(|table| table.get("nicks"))
        .and_then(Item::as_table)
        .map(|table| {
            table
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.to_owned(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    Config {
        theme: match text_of("theme") {
            Some("dark") => Some(ThemeName::Dark),
            Some("light") => Some(ThemeName::Light),
            _ => None,
        },
        accent: text_of("accent").map(str::to_owned),
        places: settings.docks,
        pinned: names("drawer", "pinned").unwrap_or_else(|| favorites.clone()),
        drawer_width: length("width"),
        drawer_height: length("height"),
        icon_size: dock_int("icon_size"),
        favorites,
        audio_hidden,
        audio_nicks,
        animations: settings.animations(),
        mode: settings.shell_mode(),
        tasks_token: settings.tasks_token,
    }
}

/// The file with other favorites; everything else is kept as it was.
/// `None` when the file is not valid TOML: it is then left alone.
pub fn with_favorites(text: &str, favorites: &[String]) -> Option<String> {
    let names: Array = favorites.iter().map(String::as_str).collect();
    with_value(text, &["dock", "favorites"], Value::Array(names))
}

/// The file with another icon size for the dock bar.
#[cfg(test)]
pub fn with_icon_size(text: &str, size: u32) -> Option<String> {
    with_value(text, &["dock", "icon_size"], i64::from(size).into())
}

/// The file with another width for the drawer.
pub fn with_drawer_width(text: &str, width: u32) -> Option<String> {
    with_value(text, &["drawer", "width"], i64::from(width).into())
}

/// The file with another height for the horizontal launcher.
pub fn with_drawer_height(text: &str, height: u32) -> Option<String> {
    with_value(text, &["drawer", "height"], i64::from(height).into())
}

/// The file with another theme.
pub fn with_theme(text: &str, theme: ThemeName) -> Option<String> {
    apply(text, &Change::Theme(theme))
}

pub fn load(path: &Path) -> Config {
    parse(&std::fs::read_to_string(path).unwrap_or_default())
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
        assert_eq!(
            config.docks(&[]),
            [(OutputRole::Left, Orientation::Vertical(Side::Left))]
        );
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
    fn without_a_word_the_dock_stands_on_the_left_screen() {
        assert_eq!(
            parse("").docks(&[]),
            [(OutputRole::Left, Orientation::Vertical(Side::Left))]
        );
        assert_eq!(
            parse(FILE).docks(&[]),
            [(OutputRole::Left, Orientation::Vertical(Side::Left))]
        );
    }

    #[test]
    fn each_screen_has_its_dock_and_its_direction() {
        let config = parse(
            "[dock.screens]\nleft = \"off\"\ncenter = \"horizontal\"\nright = \"vertical\"\n",
        );

        assert_eq!(
            config.docks(&[]),
            [
                (OutputRole::Center, Orientation::Horizontal),
                (OutputRole::Right, Orientation::Vertical(Side::Left))
            ]
        );
    }

    #[test]
    fn an_upright_dock_stands_on_either_edge() {
        let config =
            parse("[dock.screens]\nleft = \"vertical-right\"\nright = \"vertical-right\"\n");

        assert_eq!(
            config.docks(&[]),
            [
                (OutputRole::Left, Orientation::Vertical(Side::Right)),
                (OutputRole::Right, Orientation::Vertical(Side::Right))
            ]
        );
    }

    #[test]
    fn a_screen_left_unsaid_keeps_its_usual_dock() {
        let config = parse("[dock.screens]\nright = \"horizontal\"\n");

        assert_eq!(
            config.docks(&[]),
            [
                (OutputRole::Left, Orientation::Vertical(Side::Left)),
                (OutputRole::Right, Orientation::Horizontal)
            ]
        );
    }

    #[test]
    fn a_direction_that_means_nothing_is_ignored() {
        let config = parse("[dock.screens]\nleft = \"diagonal\"\ncenter = 3\n");

        assert_eq!(
            config.docks(&[]),
            [(OutputRole::Left, Orientation::Vertical(Side::Left))]
        );
    }

    #[test]
    fn a_laptop_has_its_dock_on_its_lone_screen() {
        let center = [OutputRole::Center];

        assert_eq!(
            parse("").docks(&center),
            [(OutputRole::Center, Orientation::Vertical(Side::Left))]
        );
        assert_eq!(
            parse("[dock.screens]\ncenter = \"horizontal\"\n").docks(&center),
            [(OutputRole::Center, Orientation::Horizontal)]
        );
        assert!(
            parse("[dock.screens]\ncenter = \"off\"\n")
                .docks(&center)
                .is_empty()
        );
    }

    #[test]
    fn every_dock_can_be_turned_off() {
        assert!(
            parse("[dock.screens]\nleft = \"off\"\n")
                .docks(&[])
                .is_empty()
        );
    }

    #[test]
    fn the_height_of_the_launcher_is_read_and_written() {
        assert_eq!(parse("[drawer]\nheight = 520\n").drawer_height, Some(520));
        assert_eq!(parse(FILE).drawer_height, None);

        let changed = with_drawer_height(FILE, 512).unwrap();

        assert_eq!(parse(&changed).drawer_height, Some(512));
        assert_eq!(parse(&changed).pinned, ["firefox", "mpv"]);
    }

    #[test]
    fn an_empty_file_gives_an_empty_dock() {
        let config = parse("");

        assert!(config.favorites.is_empty() && config.pinned.is_empty());
        assert_eq!((config.theme, config.drawer_width), (None, None));
    }

    #[test]
    fn without_pinned_applications_the_drawer_shows_the_favorites() {
        let config = parse("[dock]\nfavorites = [\"kitty\"]\n");

        assert_eq!(config.pinned, ["kitty"]);
    }

    #[test]
    fn a_broken_file_gives_the_defaults() {
        assert_eq!(parse("theme = \n[dock"), parse(""));
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

    #[test]
    fn the_icon_size_of_the_dock_is_read_and_written() {
        assert_eq!(parse("[dock]\nicon_size = 32\n").icon_size, Some(32));
        assert_eq!(parse("[dock]\nicon_size = 32\n").icon_size(), 32);
        assert_eq!(parse(FILE).icon_size, None);
        assert_eq!(parse(FILE).icon_size(), 24);

        let changed = with_icon_size(FILE, 28).unwrap();
        assert_eq!(parse(&changed).icon_size, Some(28));
        assert_eq!(parse(&changed).icon_size(), 28);
        assert!(changed.contains("# Mes applis de tous les jours."));
    }
}
