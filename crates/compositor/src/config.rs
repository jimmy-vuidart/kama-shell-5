//! The configuration file, `~/.config/coquille/config.toml`.

use std::path::{Path, PathBuf};

use coquille_model::{LayoutKind, OutputRole, Scale};
use serde::Deserialize;
use tracing::{info, warn};

use crate::outputs::RoleConfig;
use crate::theme::{Rgba, ThemeKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellMode {
    #[default]
    Classic,
    Spatial,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Which connector is the left, center and right screen.
    pub outputs: RoleConfig,
    pub keyboard: KeyboardConfig,
    pub mouse: MouseConfig,
    pub touchpad: TouchpadConfig,
    /// How each screen arranges the windows tidied into it.
    pub layout: LayoutConfig,
    /// How much bigger each screen draws everything.
    pub scale: ScaleConfig,
    pub theme: Option<ThemeKind>,
    /// The accent color, as `#rrggbb`; by default, the one of the theme.
    pub accent: Option<String>,
    /// Window management mode: classic (slots) or spatial (always-maximized).
    pub mode: Option<ShellMode>,
    /// Sections of the dock, which reads the same file: not for the
    /// compositor, but no reason to reject the whole file either.
    pub dock: Ignored,
    pub drawer: Ignored,
    pub audio: Ignored,
    pub widgets: Ignored,
    pub spatial: Ignored,
    /// Command of the terminal opened by the shortcut.
    pub terminal: Option<String>,
    /// Whether X11 applications can run, through XWayland; they can unless
    /// this says otherwise.
    pub xwayland: Option<bool>,
    pub glass: GlassConfig,
    pub animations: AnimationsConfig,
    pub game: GameConfig,
    pub wallpaper: WallpaperConfig,
}

impl Config {
    pub fn is_spatial(&self) -> bool {
        self.mode == Some(ShellMode::Spatial)
    }

    /// The accent the user chose, when it is a color.
    pub fn accent(&self) -> Option<Rgba> {
        let text = self.accent.as_deref()?;
        let accent = Rgba::parse(text);
        if accent.is_none() {
            warn!(
                accent = text,
                "not a color (#rrggbb): using the accent of the theme"
            );
        }
        accent
    }
}

/// A section meant for another program of the shell: anything goes in it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Ignored;

impl<'de> Deserialize<'de> for Ignored {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer).map(|_| Self)
    }
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

/// The layout of each screen; the default of its role where none is named.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayoutConfig {
    pub left: Option<LayoutKind>,
    pub center: Option<LayoutKind>,
    pub right: Option<LayoutKind>,
}

impl LayoutConfig {
    pub fn of(&self, role: OutputRole) -> LayoutKind {
        let named = match role {
            OutputRole::Left => self.left,
            OutputRole::Center => self.center,
            OutputRole::Right => self.right,
        };
        named.unwrap_or(LayoutKind::default_for(role))
    }
}

/// The scale of each screen; 100 % where none is named.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScaleConfig {
    pub left: Option<Scale>,
    pub center: Option<Scale>,
    pub right: Option<Scale>,
}

impl ScaleConfig {
    /// Every screen at `scale`.
    pub fn everywhere(scale: Scale) -> Self {
        Self {
            left: Some(scale),
            center: Some(scale),
            right: Some(scale),
        }
    }

    pub fn of(&self, role: OutputRole) -> Scale {
        let named = match role {
            OutputRole::Left => self.left,
            OutputRole::Center => self.center,
            OutputRole::Right => self.right,
        };
        named.unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeyboardConfig {
    pub layout: Option<String>,
    pub variant: Option<String>,
    /// Num Lock on when the session starts; on unless told otherwise.
    pub numlock: Option<bool>,
}

impl KeyboardConfig {
    pub fn numlock(&self) -> bool {
        self.numlock.unwrap_or(true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseAccelProfile {
    Flat,
    Adaptive,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MouseConfig {
    pub accel_speed: Option<f64>,
    pub accel_profile: Option<MouseAccelProfile>,
    pub natural_scroll: Option<bool>,
    pub left_handed: Option<bool>,
}

/// What makes a touchpad right-click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum TouchpadClick {
    /// Two fingers anywhere.
    #[serde(rename = "clickfinger")]
    Fingers,
    /// A press in the bottom right corner.
    #[serde(rename = "areas")]
    Areas,
}

/// The touchpad of a laptop; the mouse keeps its own settings.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TouchpadConfig {
    pub tap: Option<bool>,
    pub tap_drag: Option<bool>,
    pub disable_while_typing: Option<bool>,
    pub natural_scroll: Option<bool>,
    pub accel_speed: Option<f64>,
    pub click_method: Option<TouchpadClick>,
}

impl TouchpadConfig {
    pub fn tap(&self) -> bool {
        self.tap.unwrap_or(true)
    }

    pub fn tap_drag(&self) -> bool {
        self.tap_drag.unwrap_or(true)
    }

    pub fn disable_while_typing(&self) -> bool {
        self.disable_while_typing.unwrap_or(true)
    }

    pub fn natural_scroll(&self) -> bool {
        self.natural_scroll.unwrap_or(true)
    }

    pub fn click_method(&self) -> TouchpadClick {
        self.click_method.unwrap_or(TouchpadClick::Fingers)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperMode {
    #[default]
    Fill,
    Fit,
    Center,
    Tile,
}

/// Which version of the default « glass » wallpaper a 16:9 screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
pub enum WallpaperDefinition {
    /// The one that suits the definition of the screen.
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "1080p")]
    Hd,
    #[serde(rename = "4k")]
    Uhd,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WallpaperConfig {
    pub path: Option<String>,
    pub left: Option<String>,
    pub center: Option<String>,
    pub right: Option<String>,
    pub mode: Option<WallpaperMode>,
    pub definition: Option<WallpaperDefinition>,
}

impl WallpaperConfig {
    pub fn mode(&self) -> WallpaperMode {
        self.mode.unwrap_or_default()
    }

    pub fn definition(&self) -> WallpaperDefinition {
        self.definition.unwrap_or_default()
    }
}

/// Whether windows and panels ease from one place to the next.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnimationsConfig {
    /// On unless this says otherwise.
    pub enabled: Option<bool>,
}

impl AnimationsConfig {
    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
}

/// Liquid glass rendering configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlassConfig {
    pub blur: bool,
    pub strength: u8,
}

impl Default for GlassConfig {
    fn default() -> Self {
        Self {
            blur: true,
            strength: 3,
        }
    }
}

impl GlassConfig {
    /// Strength clamped to 1..=5.
    pub fn strength(&self) -> u8 {
        self.strength.clamp(1, 5)
    }
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

/// Reads the configuration again, while the compositor runs. `None` when
/// the file is gone or broken: what was read before then stays in use.
pub fn reload(path: Option<&Path>) -> Option<Config> {
    let path = path.map(Path::to_path_buf).or_else(default_path)?;
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            warn!(path = %path.display(), %error, "cannot read the configuration again");
            return None;
        }
    };
    match parse(&text) {
        Ok(config) => Some(config),
        Err(error) => {
            warn!(path = %path.display(), %error, "invalid configuration: nothing changes");
            None
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
    fn the_accent_is_read() {
        let config = parse("accent = \"#7aa7ff\"").unwrap();

        assert_eq!(config.accent.as_deref(), Some("#7aa7ff"));
    }

    #[test]
    fn the_sections_of_the_dock_are_accepted_whatever_they_hold() {
        let config = parse(
            r#"
            theme = "dark"

            [dock]
            favorites = ["firefox", "discord"]

            [drawer]
            pinned = ["firefox"]
            columns = 6

            [game]
            vrr = false
            "#,
        )
        .unwrap();

        assert_eq!(config.theme, Some(ThemeKind::Dark));
        assert!(!config.game.vrr());
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
    fn num_lock_is_on_unless_turned_off() {
        assert!(parse("").unwrap().keyboard.numlock());
        assert!(
            !parse("[keyboard]\nnumlock = false")
                .unwrap()
                .keyboard
                .numlock()
        );
        assert!(parse("[keyboard]\nnumlock = \"oui\"").is_err());
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
        assert_eq!(config.accent.as_deref(), Some("#5b63e6"));
        assert_eq!(config.terminal.as_deref(), Some("kitty"));
        assert_eq!(config.xwayland, Some(true));
        assert_eq!(config.keyboard.layout.as_deref(), Some("fr"));
        assert_eq!(config.game.sticky_edges_ms, Some(300));
        assert_eq!(config.game.direct_scanout, Some(true));
        assert_eq!(config.game.vrr, Some(true));
        assert_eq!(config.mouse.accel_speed, Some(0.0));
        assert_eq!(
            config.mouse.accel_profile,
            Some(MouseAccelProfile::Adaptive)
        );
        assert_eq!(config.mouse.natural_scroll, Some(false));
        assert_eq!(config.mouse.left_handed, Some(false));
        assert_eq!(
            config.wallpaper.path.as_deref(),
            Some("/chemin/vers/image.png")
        );
        assert_eq!(
            config.wallpaper.left.as_deref(),
            Some("/chemin/vers/gauche.png")
        );
        assert_eq!(config.wallpaper.mode, Some(WallpaperMode::Fill));
    }

    #[test]
    fn what_the_settings_screen_writes_is_accepted() {
        use coquille_config::{Change, DockPlace, PerScreen, ShellMode as ConfigShellMode, apply};
        use coquille_ipc::ThemeName;

        let changes = [
            Change::Theme(ThemeName::Dark),
            Change::ShellMode(ConfigShellMode::Spatial),
            Change::Accent(Some("#ff8800".into())),
            Change::Terminal(Some("foot".into())),
            Change::Xwayland(false),
            Change::Outputs(PerScreen {
                left: Some("DP-1".into()),
                center: Some("DP-2".into()),
                right: None,
            }),
            Change::Keyboard {
                layout: Some("fr".into()),
                variant: Some("bepo".into()),
            },
            Change::StickyEdges(450),
            Change::DirectScanout(false),
            Change::Vrr(false),
            Change::Animations(false),
            Change::Dock(OutputRole::Center, DockPlace::Horizontal),
            Change::Dock(OutputRole::Right, DockPlace::VerticalRight),
            Change::DockIconSize(32),
            Change::Layout(OutputRole::Left, LayoutKind::MainSide),
            Change::Layout(OutputRole::Center, LayoutKind::ThreeColumns),
            Change::Scale(OutputRole::Center, Scale::new(1.75)),
            Change::MouseAccelSpeed(0.2),
            Change::MouseAccelProfile(coquille_config::MouseAccelProfile::Flat),
            Change::MouseNaturalScroll(true),
            Change::MouseLeftHanded(true),
            Change::TouchpadTap(false),
            Change::TouchpadTapDrag(false),
            Change::TouchpadDisableWhileTyping(false),
            Change::TouchpadNaturalScroll(false),
            Change::TouchpadAccelSpeed(-0.3),
            Change::TouchpadClick(coquille_config::TouchpadClick::Areas),
            Change::WallpaperPath(Some("/path/to/wall.png".into())),
            Change::Wallpaper(OutputRole::Left, Some("/path/to/left.png".into())),
            Change::WallpaperMode(coquille_config::WallpaperMode::Fit),
            Change::WallpaperDefinition(coquille_config::WallpaperDefinition::Uhd),
            Change::AudioNick {
                device: "alsa_output.headphones".into(),
                nick: Some("Casque".into()),
            },
            Change::AudioVisibility {
                device: "alsa_output.hdmi".into(),
                visible: false,
            },
            Change::TasksToken(Some("kama_test_token".into())),
            Change::FoundryToken(Some("kma_test_token".into())),
        ];
        // Exhaustive match so adding a new `Change` variant reminds us to
        // cover it in `changes` above and verify the compositor accepts it.
        for change in &changes {
            match change {
                Change::Theme(_)
                | Change::Accent(_)
                | Change::Terminal(_)
                | Change::Xwayland(_)
                | Change::ShellMode(_)
                | Change::Outputs(_)
                | Change::Keyboard { .. }
                | Change::StickyEdges(_)
                | Change::DirectScanout(_)
                | Change::Vrr(_)
                | Change::Animations(_)
                | Change::Dock(_, _)
                | Change::Layout(_, _)
                | Change::Scale(_, _)
                | Change::DockIconSize(_)
                | Change::MouseAccelSpeed(_)
                | Change::MouseAccelProfile(_)
                | Change::MouseNaturalScroll(_)
                | Change::MouseLeftHanded(_)
                | Change::TouchpadTap(_)
                | Change::TouchpadTapDrag(_)
                | Change::TouchpadDisableWhileTyping(_)
                | Change::TouchpadNaturalScroll(_)
                | Change::TouchpadAccelSpeed(_)
                | Change::TouchpadClick(_)
                | Change::WallpaperPath(_)
                | Change::Wallpaper(_, _)
                | Change::WallpaperMode(_)
                | Change::WallpaperDefinition(_)
                | Change::AudioNick { .. }
                | Change::AudioVisibility { .. }
                | Change::TasksToken(_)
                | Change::FoundryToken(_) => {}
            }
        }
        let mut text = SAMPLE.to_owned();
        for change in &changes {
            text = apply(&text, change).unwrap();
        }

        let config = parse(&text).unwrap();

        assert_eq!(config.theme, Some(ThemeKind::Dark));
        assert!(config.is_spatial());
        assert_eq!(config.accent(), Some(Rgba::rgb(0xff_88_00)));
        assert_eq!(config.terminal.as_deref(), Some("foot"));
        assert_eq!(config.xwayland, Some(false));
        assert_eq!(config.outputs.left.as_deref(), Some("DP-1"));
        assert_eq!(config.outputs.right, None);
        assert_eq!(config.keyboard.variant.as_deref(), Some("bepo"));
        assert_eq!(config.game.sticky_edges_ms(), 450);
        assert!(!config.game.direct_scanout() && !config.game.vrr());
        assert!(!config.animations.enabled());
        assert_eq!(config.mouse.accel_speed, Some(0.2));
        assert_eq!(config.mouse.accel_profile, Some(MouseAccelProfile::Flat));
        assert_eq!(config.mouse.natural_scroll, Some(true));
        assert_eq!(config.mouse.left_handed, Some(true));
        assert_eq!(config.wallpaper.path.as_deref(), Some("/path/to/wall.png"));
        assert_eq!(config.wallpaper.left.as_deref(), Some("/path/to/left.png"));
        assert_eq!(config.wallpaper.mode(), WallpaperMode::Fit);
        assert_eq!(config.wallpaper.definition(), WallpaperDefinition::Uhd);
        assert_eq!(config.layout.of(OutputRole::Left), LayoutKind::MainSide);
        assert_eq!(
            config.layout.of(OutputRole::Center),
            LayoutKind::ThreeColumns
        );
        assert_eq!(config.layout.of(OutputRole::Right), LayoutKind::TwoColumns);
        assert_eq!(config.scale.of(OutputRole::Center).get(), 1.75);
        assert_eq!(config.scale.of(OutputRole::Left), Scale::ONE);
        assert!(!config.touchpad.tap() && !config.touchpad.tap_drag());
        assert!(!config.touchpad.disable_while_typing() && !config.touchpad.natural_scroll());
        assert_eq!(config.touchpad.accel_speed, Some(-0.3));
        assert_eq!(config.touchpad.click_method(), TouchpadClick::Areas);
    }

    #[test]
    fn a_touchpad_taps_and_scrolls_naturally_unless_told_otherwise() {
        let config = parse("").unwrap();

        assert!(config.touchpad.tap() && config.touchpad.natural_scroll());
        assert_eq!(config.touchpad.click_method(), TouchpadClick::Fingers);
        assert!(parse("[touchpad]\nclick_method = \"corner\"\n").is_err());
    }

    #[test]
    fn a_scale_is_rounded_to_its_step_and_kept_within_range() {
        let config = parse("[scale]\nleft = 2\ncenter = 1.3\nright = 7.5\n").unwrap();

        assert_eq!(config.scale.of(OutputRole::Left).get(), 2.0);
        assert_eq!(config.scale.of(OutputRole::Center).get(), 1.25);
        assert_eq!(config.scale.of(OutputRole::Right).get(), 3.0);
        assert!(parse("[scale]\nmiddle = 2.0\n").is_err());
    }

    #[test]
    fn each_screen_has_the_layout_of_its_role_unless_told_otherwise() {
        let config = parse("[layout]\nright = \"full\"\n").unwrap();

        assert_eq!(config.layout.of(OutputRole::Left), LayoutKind::TwoColumns);
        assert_eq!(config.layout.of(OutputRole::Center), LayoutKind::Full);
        assert_eq!(config.layout.of(OutputRole::Right), LayoutKind::Full);
        assert!(parse("[layout]\nleft = \"rows\"\n").is_err());
    }

    #[test]
    fn an_accent_that_is_not_a_color_is_ignored() {
        assert_eq!(parse("accent = \"blue\"").unwrap().accent(), None);
        assert_eq!(parse("").unwrap().accent(), None);
    }

    #[test]
    fn an_unknown_setting_is_an_error() {
        assert!(parse("themes = \"dark\"").is_err());
        assert!(parse("[outputs]\nmiddle = \"DP-2\"").is_err());
    }

    #[test]
    fn glass_settings_are_read_with_defaults_and_clamping() {
        let empty = parse("").unwrap();
        assert_eq!(empty.glass, GlassConfig::default());
        assert!(empty.glass.blur);
        assert_eq!(empty.glass.strength(), 3);

        let custom = parse(
            r#"
            [glass]
            blur = false
            strength = 2
            "#,
        )
        .unwrap();
        assert!(!custom.glass.blur);
        assert_eq!(custom.glass.strength(), 2);

        let low = parse("[glass]\nstrength = 0").unwrap();
        assert_eq!(low.glass.strength(), 1);

        let high = parse("[glass]\nstrength = 99").unwrap();
        assert_eq!(high.glass.strength(), 5);
    }

    #[test]
    fn mode_and_widgets_settings_are_parsed() {
        let default_cfg = parse("").unwrap();
        assert!(!default_cfg.is_spatial());
        assert_eq!(default_cfg.mode, None);

        let classic = parse("mode = \"classic\"").unwrap();
        assert!(!classic.is_spatial());
        assert_eq!(classic.mode, Some(ShellMode::Classic));

        let spatial = parse(
            "mode = \"spatial\"\n[widgets]\nclock = true\n[spatial]\ntasks_token = \"kama_abc\"\nfoundry_token = \"kma_def\"\n",
        )
        .unwrap();
        assert!(spatial.is_spatial());
        assert_eq!(spatial.mode, Some(ShellMode::Spatial));
    }
}
