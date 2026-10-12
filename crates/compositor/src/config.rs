//! The configuration file, `~/.config/coquille/config.toml`.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::{info, warn};

use crate::outputs::RoleConfig;
use crate::theme::ThemeKind;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Which connector is the left, center and right screen.
    pub outputs: RoleConfig,
    pub keyboard: KeyboardConfig,
    pub theme: Option<ThemeKind>,
    /// Command of the terminal opened by the shortcut.
    pub terminal: Option<String>,
    /// Whether X11 applications can run, through XWayland; they can unless
    /// this says otherwise.
    pub xwayland: Option<bool>,
    pub game: GameConfig,
}

/// Game mode: what happens on a screen held by a fullscreen window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GameConfig {
    /// How long the pointer must be pushed against the edge to leave the
    /// screen of a game, in milliseconds; 0 turns sticky edges off.
    pub sticky_edges_ms: Option<u64>,
    /// Hardware backend: show the buffer of the game as it is, without
    /// drawing it again, when nothing else is on its screen. On unless this
    /// says otherwise.
    pub direct_scanout: Option<bool>,
    /// Hardware backend: let the refresh rate of the screen follow the game,
    /// where the screen can. On unless this says otherwise.
    pub vrr: Option<bool>,
}

impl GameConfig {
    pub const DEFAULT_STICKY_EDGES_MS: u64 = 300;

    pub fn direct_scanout(&self) -> bool {
        self.direct_scanout.unwrap_or(true)
    }

    pub fn vrr(&self) -> bool {
        self.vrr.unwrap_or(true)
    }

    pub fn sticky_edges_ms(&self) -> u64 {
        self.sticky_edges_ms
            .unwrap_or(Self::DEFAULT_STICKY_EDGES_MS)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeyboardConfig {
    pub layout: Option<String>,
    pub variant: Option<String>,
}

pub fn parse(text: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(text)
}

/// Where the configuration is read from when no path is given.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")))?;
    Some(base.join("coquille/config.toml"))
}

/// Reads the configuration. A missing file gives the defaults; so does a
/// broken one, with a warning, so that a typo never prevents starting.
pub fn load(path: Option<&Path>) -> Config {
    let Some(path) = path.map(Path::to_path_buf).or_else(default_path) else {
        return Config::default();
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => {
            info!(path = %path.display(), "no configuration file, using the defaults");
            return Config::default();
        }
    };
    match parse(&text) {
        Ok(config) => {
            info!(path = %path.display(), "configuration loaded");
            config
        }
        Err(error) => {
            warn!(path = %path.display(), %error, "invalid configuration, using the defaults");
            Config::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_gives_the_defaults() {
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn screens_are_named_by_connector() {
        let config = parse(
            r#"
            [outputs]
            left = "HDMI-A-1"
            center = "DP-2"
            right = "DP-1"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.outputs,
            RoleConfig {
                left: Some("HDMI-A-1".into()),
                center: Some("DP-2".into()),
                right: Some("DP-1".into()),
            }
        );
    }

    #[test]
    fn general_settings_are_read() {
        let config = parse(
            r#"
            theme = "dark"
            terminal = "kitty"

            [keyboard]
            layout = "fr"
            "#,
        )
        .unwrap();

        assert_eq!(config.theme, Some(ThemeKind::Dark));
        assert_eq!(config.terminal.as_deref(), Some("kitty"));
        assert_eq!(config.keyboard.layout.as_deref(), Some("fr"));
        assert_eq!(config.keyboard.variant, None);
    }

    #[test]
    fn sticky_edges_last_300_ms_unless_told_otherwise() {
        assert_eq!(parse("").unwrap().game.sticky_edges_ms(), 300);

        let config = parse("[game]\nsticky_edges_ms = 0").unwrap();
        assert_eq!(config.game.sticky_edges_ms(), 0);
        assert!(parse("[game]\nsticky_edges_ms = -5").is_err());
        assert!(parse("[game]\nsticky = 5").is_err());
    }

    #[test]
    fn direct_scanout_and_vrr_are_on_unless_turned_off_one_by_one() {
        let defaults = parse("").unwrap().game;
        assert!(defaults.direct_scanout());
        assert!(defaults.vrr());

        let config = parse("[game]\ndirect_scanout = false").unwrap().game;
        assert!(!config.direct_scanout());
        assert!(config.vrr());

        let config = parse("[game]\nvrr = false").unwrap().game;
        assert!(config.direct_scanout());
        assert!(!config.vrr());
    }

    #[test]
    fn xwayland_can_be_turned_off() {
        assert_eq!(parse("").unwrap().xwayland, None);
        assert_eq!(parse("xwayland = false").unwrap().xwayland, Some(false));
    }

    /// The file installed for the user, as it is and with every setting it
    /// suggests switched on.
    const SAMPLE: &str = include_str!("../../../config/config.toml");

    #[test]
    fn the_sample_file_is_valid() {
        let config = parse(SAMPLE).unwrap();

        assert_eq!(config.outputs.center.as_deref(), Some("DP-2"));
    }

    #[test]
    fn the_sample_file_stays_valid_with_every_suggestion_switched_on() {
        // A commented-out setting reads `# name = value`, a section `# [name]`;
        // explanations are plain sentences.
        let is_setting = |line: &str| {
            let name = line.split(" = ").next().unwrap_or_default();
            let named = line.contains(" = ")
                && !name.is_empty()
                && name.chars().all(|c| c.is_ascii_lowercase() || c == '_');
            named || (line.starts_with('[') && line.ends_with(']'))
        };
        let switched_on: Vec<&str> = SAMPLE
            .lines()
            .map(|line| match line.strip_prefix("# ") {
                Some(setting) if is_setting(setting) => setting,
                _ => line,
            })
            .collect();

        let config = parse(&switched_on.join("\n")).unwrap();

        assert_eq!(config.theme, Some(ThemeKind::Light));
        assert_eq!(config.terminal.as_deref(), Some("kitty"));
        assert_eq!(config.xwayland, Some(true));
        assert_eq!(config.keyboard.layout.as_deref(), Some("fr"));
        assert_eq!(config.game.sticky_edges_ms, Some(300));
        assert_eq!(config.game.direct_scanout, Some(true));
        assert_eq!(config.game.vrr, Some(true));
    }

    #[test]
    fn an_unknown_setting_is_an_error() {
        assert!(parse("themes = \"dark\"").is_err());
        assert!(parse("[outputs]\nmiddle = \"DP-2\"").is_err());
    }
}
