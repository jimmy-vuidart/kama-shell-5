//! Every setting of the file a settings screen shows, read leniently (what
//! is missing or means nothing is simply not set), and how each is written
//! back.

use std::collections::BTreeMap;

use coquille_ipc::ThemeName;
use coquille_model::{LayoutKind, OutputRole, Scale};
use toml_edit::{DocumentMut, Item, Value};

use crate::edit::{with_value, without};

/// One thing for each of the three screens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PerScreen<T> {
    pub left: T,
    pub center: T,
    pub right: T,
}

impl<T> PerScreen<T> {
    pub const ROLES: [OutputRole; 3] = [OutputRole::Left, OutputRole::Center, OutputRole::Right];

    pub fn get(&self, role: OutputRole) -> &T {
        match role {
            OutputRole::Left => &self.left,
            OutputRole::Center => &self.center,
            OutputRole::Right => &self.right,
        }
    }

    pub fn get_mut(&mut self, role: OutputRole) -> &mut T {
        match role {
            OutputRole::Left => &mut self.left,
            OutputRole::Center => &mut self.center,
            OutputRole::Right => &mut self.right,
        }
    }

    pub fn from_roles(of: impl Fn(OutputRole) -> T) -> Self {
        Self {
            left: of(OutputRole::Left),
            center: of(OutputRole::Center),
            right: of(OutputRole::Right),
        }
    }
}

/// The name of a screen in the file.
fn role_name(role: OutputRole) -> &'static str {
    match role {
        OutputRole::Left => "left",
        OutputRole::Center => "center",
        OutputRole::Right => "right",
    }
}

/// Whether a screen has a dock, and which way it lies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockPlace {
    /// A column on the left edge.
    Vertical,
    /// A column on the right edge.
    VerticalRight,
    /// A row on the bottom edge.
    Horizontal,
    Off,
}

impl DockPlace {
    pub const ALL: [Self; 4] = [
        Self::Vertical,
        Self::VerticalRight,
        Self::Horizontal,
        Self::Off,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::VerticalRight => "vertical-right",
            Self::Horizontal => "horizontal",
            Self::Off => "off",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|place| place.name() == name)
    }

    /// What a screen has when the file says nothing of it: a dock on the
    /// left screen, upright on its left edge, none elsewhere.
    pub fn usual(role: OutputRole) -> Self {
        match role {
            OutputRole::Left => Self::Vertical,
            OutputRole::Center | OutputRole::Right => Self::Off,
        }
    }
}

/// The dock of each screen, given what the file says of each (`None`
/// where it says nothing) and which screens are `present`. A screen the file
/// says nothing of has its usual dock (one on the left screen). When the
/// file says nothing of any present screen and none of them would have a
/// dock (a laptop: its lone screen is the center), the first present screen
/// (the center first) gets an upright dock on its left edge. Once the file
/// names a present screen, it is taken at its word. With no screen known,
/// every screen counts as present.
pub fn docks_for(
    said: &PerScreen<Option<DockPlace>>,
    present: &[OutputRole],
) -> PerScreen<DockPlace> {
    let mut docks = PerScreen::from_roles(|role| said.get(role).unwrap_or(DockPlace::usual(role)));
    let present: Vec<OutputRole> = if present.is_empty() {
        PerScreen::<()>::ROLES.to_vec()
    } else {
        present.to_vec()
    };
    let has_one = present
        .iter()
        .any(|role| *docks.get(*role) != DockPlace::Off);
    let unsaid = present.iter().all(|role| said.get(*role).is_none());
    if !has_one && unsaid {
        let order = [OutputRole::Center, OutputRole::Left, OutputRole::Right];
        if let Some(role) = order.into_iter().find(|role| present.contains(role)) {
            *docks.get_mut(role) = DockPlace::Vertical;
        }
    }
    docks
}

/// A mouse pointer acceleration profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAccelProfile {
    Adaptive,
    Flat,
}

impl MouseAccelProfile {
    pub const ALL: [Self; 2] = [Self::Adaptive, Self::Flat];

    pub fn name(self) -> &'static str {
        match self {
            Self::Adaptive => "adaptive",
            Self::Flat => "flat",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|profile| profile.name() == name)
    }
}

/// What makes a touchpad right-click: two fingers anywhere, or a press in
/// its bottom right corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchpadClick {
    Fingers,
    Areas,
}

impl TouchpadClick {
    pub const ALL: [Self; 2] = [Self::Fingers, Self::Areas];

    pub fn name(self) -> &'static str {
        match self {
            Self::Fingers => "clickfinger",
            Self::Areas => "areas",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|click| click.name() == name)
    }
}

/// How a wallpaper image is scaled or fitted to the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WallpaperMode {
    #[default]
    Fill,
    Fit,
    Center,
    Tile,
}

impl WallpaperMode {
    pub const ALL: [Self; 4] = [Self::Fill, Self::Fit, Self::Center, Self::Tile];

    pub fn name(self) -> &'static str {
        match self {
            Self::Fill => "fill",
            Self::Fit => "fit",
            Self::Center => "center",
            Self::Tile => "tile",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }
}

/// Which version of the default « glass » wallpaper a 16:9 screen shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WallpaperDefinition {
    /// The one that suits the definition of the screen.
    #[default]
    Auto,
    /// 1920 × 1080.
    Hd,
    /// 3840 × 2160.
    Uhd,
}

impl WallpaperDefinition {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Hd, Self::Uhd];

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Hd => "1080p",
            Self::Uhd => "4k",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|definition| definition.name() == name)
    }
}

/// What the file says. `None` is a setting left to its default.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Settings {
    pub theme: Option<ThemeName>,
    /// `#rrggbb`.
    pub accent: Option<String>,
    pub terminal: Option<String>,
    pub xwayland: Option<bool>,
    /// The connector of each screen.
    pub outputs: PerScreen<Option<String>>,
    pub keyboard_layout: Option<String>,
    pub keyboard_variant: Option<String>,
    pub sticky_edges_ms: Option<u64>,
    pub direct_scanout: Option<bool>,
    pub vrr: Option<bool>,
    /// Windows and panels ease from one place to the next.
    pub animations: Option<bool>,
    pub docks: PerScreen<Option<DockPlace>>,
    /// How each screen arranges the windows tidied into it.
    pub layouts: PerScreen<Option<LayoutKind>>,
    /// How much bigger each screen draws everything.
    pub scales: PerScreen<Option<Scale>>,
    pub dock_icon_size: Option<u32>,
    pub mouse_accel_speed: Option<f64>,
    pub mouse_accel_profile: Option<MouseAccelProfile>,
    pub mouse_natural_scroll: Option<bool>,
    pub mouse_left_handed: Option<bool>,
    pub touchpad_tap: Option<bool>,
    pub touchpad_tap_drag: Option<bool>,
    pub touchpad_disable_while_typing: Option<bool>,
    pub touchpad_natural_scroll: Option<bool>,
    pub touchpad_accel_speed: Option<f64>,
    pub touchpad_click: Option<TouchpadClick>,
    pub wallpaper_path: Option<String>,
    pub wallpapers: PerScreen<Option<String>>,
    pub wallpaper_mode: Option<WallpaperMode>,
    pub wallpaper_definition: Option<WallpaperDefinition>,
    pub audio_hidden: Vec<String>,
    pub audio_nicks: BTreeMap<String, String>,
}

impl Settings {
    pub const DEFAULT_STICKY_EDGES_MS: u64 = 300;
    pub const DEFAULT_DOCK_ICON_SIZE: u32 = 24;

    /// Reads the file. One that is not valid TOML says nothing.
    pub fn parse(text: &str) -> Self {
        let Ok(document) = text.parse::<DocumentMut>() else {
            return Self::default();
        };
        let item = |path: &[&str]| {
            let mut item = Some(document.as_item());
            for name in path {
                item = item.and_then(|item| item.get(name));
            }
            item
        };
        let text_of = |path: &[&str]| item(path).and_then(Item::as_str).map(str::to_owned);
        // An empty name is no name: `variant = ""` asks for the plain layout.
        let name_of = |path: &[&str]| text_of(path).filter(|name| !name.is_empty());
        let switch = |path: &[&str]| item(path).and_then(Item::as_bool);
        let float_of = |path: &[&str]| {
            item(path).and_then(|item| {
                item.as_float()
                    .or_else(|| item.as_integer().map(|i| i as f64))
            })
        };
        let audio_hidden = item(&["audio", "hidden"])
            .and_then(Item::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let audio_nicks = item(&["audio", "nicks"])
            .and_then(Item::as_table)
            .map(|table| {
                table
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.to_owned(), s.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            theme: match text_of(&["theme"]).as_deref() {
                Some("dark") => Some(ThemeName::Dark),
                Some("light") => Some(ThemeName::Light),
                _ => None,
            },
            accent: text_of(&["accent"]).filter(|accent| is_color(accent)),
            terminal: name_of(&["terminal"]),
            xwayland: switch(&["xwayland"]),
            outputs: PerScreen::from_roles(|role| name_of(&["outputs", role_name(role)])),
            keyboard_layout: name_of(&["keyboard", "layout"]),
            keyboard_variant: name_of(&["keyboard", "variant"]),
            sticky_edges_ms: item(&["game", "sticky_edges_ms"])
                .and_then(Item::as_integer)
                .and_then(|delay| u64::try_from(delay).ok()),
            direct_scanout: switch(&["game", "direct_scanout"]),
            vrr: switch(&["game", "vrr"]),
            animations: switch(&["animations", "enabled"]),
            docks: PerScreen::from_roles(|role| {
                let said = text_of(&["dock", "screens", role_name(role)])?;
                DockPlace::named(&said)
            }),
            layouts: PerScreen::from_roles(|role| {
                let said = text_of(&["layout", role_name(role)])?;
                LayoutKind::named(&said)
            }),
            scales: PerScreen::from_roles(|role| {
                float_of(&["scale", role_name(role)]).map(Scale::new)
            }),
            dock_icon_size: item(&["dock", "icon_size"])
                .and_then(Item::as_integer)
                .and_then(|size| u32::try_from(size).ok()),
            mouse_accel_speed: float_of(&["mouse", "accel_speed"]),
            mouse_accel_profile: text_of(&["mouse", "accel_profile"])
                .as_deref()
                .and_then(MouseAccelProfile::named),
            mouse_natural_scroll: switch(&["mouse", "natural_scroll"]),
            mouse_left_handed: switch(&["mouse", "left_handed"]),
            touchpad_tap: switch(&["touchpad", "tap"]),
            touchpad_tap_drag: switch(&["touchpad", "tap_drag"]),
            touchpad_disable_while_typing: switch(&["touchpad", "disable_while_typing"]),
            touchpad_natural_scroll: switch(&["touchpad", "natural_scroll"]),
            touchpad_accel_speed: float_of(&["touchpad", "accel_speed"]),
            touchpad_click: text_of(&["touchpad", "click_method"])
                .as_deref()
                .and_then(TouchpadClick::named),
            wallpaper_path: name_of(&["wallpaper", "path"]),
            wallpapers: PerScreen::from_roles(|role| name_of(&["wallpaper", role_name(role)])),
            wallpaper_mode: text_of(&["wallpaper", "mode"])
                .as_deref()
                .and_then(WallpaperMode::named),
            wallpaper_definition: text_of(&["wallpaper", "definition"])
                .as_deref()
                .and_then(WallpaperDefinition::named),
            audio_hidden,
            audio_nicks,
        }
    }

    pub fn theme(&self) -> ThemeName {
        self.theme.unwrap_or_default()
    }

    pub fn xwayland(&self) -> bool {
        self.xwayland.unwrap_or(true)
    }

    pub fn sticky_edges_ms(&self) -> u64 {
        self.sticky_edges_ms
            .unwrap_or(Self::DEFAULT_STICKY_EDGES_MS)
    }

    pub fn direct_scanout(&self) -> bool {
        self.direct_scanout.unwrap_or(true)
    }

    pub fn vrr(&self) -> bool {
        self.vrr.unwrap_or(true)
    }

    pub fn animations(&self) -> bool {
        self.animations.unwrap_or(true)
    }

    pub fn dock(&self, role: OutputRole) -> DockPlace {
        self.docks.get(role).unwrap_or(DockPlace::usual(role))
    }

    /// The dock each screen has when these screens are `present` (see
    /// [`docks_for`]).
    pub fn docks_on(&self, present: &[OutputRole]) -> PerScreen<DockPlace> {
        docks_for(&self.docks, present)
    }

    pub fn layout(&self, role: OutputRole) -> LayoutKind {
        self.layouts
            .get(role)
            .unwrap_or(LayoutKind::default_for(role))
    }

    pub fn scale(&self, role: OutputRole) -> Scale {
        self.scales.get(role).unwrap_or_default()
    }

    pub fn dock_icon_size(&self) -> u32 {
        self.dock_icon_size.unwrap_or(Self::DEFAULT_DOCK_ICON_SIZE)
    }

    pub fn wallpaper(&self, role: OutputRole) -> Option<&str> {
        self.wallpapers
            .get(role)
            .as_deref()
            .or(self.wallpaper_path.as_deref())
    }

    pub fn wallpaper_mode(&self) -> WallpaperMode {
        self.wallpaper_mode.unwrap_or_default()
    }

    pub fn wallpaper_definition(&self) -> WallpaperDefinition {
        self.wallpaper_definition.unwrap_or_default()
    }

    pub fn mouse_accel_speed(&self) -> f64 {
        self.mouse_accel_speed.unwrap_or(0.0)
    }

    pub fn mouse_accel_profile(&self) -> MouseAccelProfile {
        self.mouse_accel_profile
            .unwrap_or(MouseAccelProfile::Adaptive)
    }

    pub fn mouse_natural_scroll(&self) -> bool {
        self.mouse_natural_scroll.unwrap_or(false)
    }

    pub fn mouse_left_handed(&self) -> bool {
        self.mouse_left_handed.unwrap_or(false)
    }

    /// A tap is a click, unless the file says otherwise.
    pub fn touchpad_tap(&self) -> bool {
        self.touchpad_tap.unwrap_or(true)
    }

    pub fn touchpad_tap_drag(&self) -> bool {
        self.touchpad_tap_drag.unwrap_or(true)
    }

    pub fn touchpad_disable_while_typing(&self) -> bool {
        self.touchpad_disable_while_typing.unwrap_or(true)
    }

    /// The content follows the fingers, as on a phone, unless the file says
    /// otherwise.
    pub fn touchpad_natural_scroll(&self) -> bool {
        self.touchpad_natural_scroll.unwrap_or(true)
    }

    pub fn touchpad_accel_speed(&self) -> f64 {
        self.touchpad_accel_speed.unwrap_or(0.0)
    }

    pub fn touchpad_click(&self) -> TouchpadClick {
        self.touchpad_click.unwrap_or(TouchpadClick::Fingers)
    }

    pub fn is_audio_sink_visible(&self, name: &str) -> bool {
        !self.audio_hidden.iter().any(|h| h == name)
    }

    pub fn audio_sink_nick<'a>(&'a self, name: &str) -> Option<&'a str> {
        self.audio_nicks.get(name).map(String::as_str)
    }

    pub fn audio_sink_display_name<'a>(&'a self, name: &str, fallback: &'a str) -> &'a str {
        self.audio_sink_nick(name).unwrap_or(fallback)
    }
}

/// Whether a text is a color as the file writes them: `#rrggbb`.
pub(crate) fn is_color(text: &str) -> bool {
    let digits = text.strip_prefix('#').unwrap_or_default();
    digits.len() == 6 && digits.bytes().all(|digit| digit.is_ascii_hexdigit())
}

/// Something the user changed on the settings screen.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Theme(ThemeName),
    /// `None` goes back to the accent of the theme.
    Accent(Option<String>),
    /// `None` goes back to the first terminal found.
    Terminal(Option<String>),
    Xwayland(bool),
    /// The connector of each screen.
    Outputs(PerScreen<Option<String>>),
    /// `None` goes back to the layout of the system.
    Keyboard {
        layout: Option<String>,
        variant: Option<String>,
    },
    /// In milliseconds; 0 turns sticky edges off.
    StickyEdges(u64),
    DirectScanout(bool),
    Vrr(bool),
    Animations(bool),
    Dock(OutputRole, DockPlace),
    Layout(OutputRole, LayoutKind),
    Scale(OutputRole, Scale),
    DockIconSize(u32),
    MouseAccelSpeed(f64),
    MouseAccelProfile(MouseAccelProfile),
    MouseNaturalScroll(bool),
    MouseLeftHanded(bool),
    TouchpadTap(bool),
    TouchpadTapDrag(bool),
    TouchpadDisableWhileTyping(bool),
    TouchpadNaturalScroll(bool),
    TouchpadAccelSpeed(f64),
    TouchpadClick(TouchpadClick),
    WallpaperPath(Option<String>),
    Wallpaper(OutputRole, Option<String>),
    WallpaperMode(WallpaperMode),
    /// `Auto` removes the line.
    WallpaperDefinition(WallpaperDefinition),
    AudioNick {
        device: String,
        nick: Option<String>,
    },
    AudioVisibility {
        device: String,
        visible: bool,
    },
}

/// The file with a change made. `None` when the file is not valid TOML: it
/// is then left alone.
pub fn apply(text: &str, change: &Change) -> Option<String> {
    let text_or_nothing = |text: &str, path: &[&str], value: &Option<String>| match value {
        Some(value) => with_value(text, path, Value::from(value.as_str())),
        None => without(text, path),
    };
    match change {
        Change::Theme(theme) => {
            let name = match theme {
                ThemeName::Light => "light",
                ThemeName::Dark => "dark",
            };
            with_value(text, &["theme"], name.into())
        }
        Change::Accent(accent) => {
            let accent = accent.clone().filter(|accent| is_color(accent));
            text_or_nothing(
                text,
                &["accent"],
                &accent.map(|accent| accent.to_lowercase()),
            )
        }
        Change::Terminal(terminal) => text_or_nothing(text, &["terminal"], terminal),
        Change::Xwayland(on) => with_value(text, &["xwayland"], (*on).into()),
        Change::Outputs(outputs) => {
            let mut text = text.to_owned();
            for role in PerScreen::<()>::ROLES {
                let path = ["outputs", role_name(role)];
                text = text_or_nothing(&text, &path, outputs.get(role))?;
            }
            Some(text)
        }
        Change::Keyboard { layout, variant } => {
            let text = text_or_nothing(text, &["keyboard", "layout"], layout)?;
            // A variant belongs to its layout: none without one.
            let variant = variant.clone().filter(|_| layout.is_some());
            text_or_nothing(&text, &["keyboard", "variant"], &variant)
        }
        Change::StickyEdges(delay) => {
            let delay = i64::try_from(*delay).unwrap_or(i64::MAX);
            with_value(text, &["game", "sticky_edges_ms"], delay.into())
        }
        Change::DirectScanout(on) => with_value(text, &["game", "direct_scanout"], (*on).into()),
        Change::Vrr(on) => with_value(text, &["game", "vrr"], (*on).into()),
        Change::Animations(on) => with_value(text, &["animations", "enabled"], (*on).into()),
        Change::Dock(role, place) => {
            let path = ["dock", "screens", role_name(*role)];
            with_value(text, &path, place.name().into())
        }
        Change::Layout(role, kind) => {
            let path = ["layout", role_name(*role)];
            with_value(text, &path, kind.name().into())
        }
        Change::Scale(role, scale) => {
            let path = ["scale", role_name(*role)];
            with_value(text, &path, scale.get().into())
        }
        Change::DockIconSize(size) => {
            let path = ["dock", "icon_size"];
            with_value(text, &path, i64::from(*size).into())
        }
        Change::MouseAccelSpeed(speed) => {
            let speed = (speed * 100.0).round() / 100.0;
            with_value(text, &["mouse", "accel_speed"], speed.into())
        }
        Change::MouseAccelProfile(profile) => {
            with_value(text, &["mouse", "accel_profile"], profile.name().into())
        }
        Change::MouseNaturalScroll(on) => {
            with_value(text, &["mouse", "natural_scroll"], (*on).into())
        }
        Change::MouseLeftHanded(on) => with_value(text, &["mouse", "left_handed"], (*on).into()),
        Change::TouchpadTap(on) => with_value(text, &["touchpad", "tap"], (*on).into()),
        Change::TouchpadTapDrag(on) => with_value(text, &["touchpad", "tap_drag"], (*on).into()),
        Change::TouchpadDisableWhileTyping(on) => {
            with_value(text, &["touchpad", "disable_while_typing"], (*on).into())
        }
        Change::TouchpadNaturalScroll(on) => {
            with_value(text, &["touchpad", "natural_scroll"], (*on).into())
        }
        Change::TouchpadAccelSpeed(speed) => {
            let speed = (speed * 100.0).round() / 100.0;
            with_value(text, &["touchpad", "accel_speed"], speed.into())
        }
        Change::TouchpadClick(click) => {
            with_value(text, &["touchpad", "click_method"], click.name().into())
        }
        Change::WallpaperPath(path) => text_or_nothing(text, &["wallpaper", "path"], path),
        Change::Wallpaper(role, path) => {
            let path_key = ["wallpaper", role_name(*role)];
            text_or_nothing(text, &path_key, path)
        }
        Change::WallpaperMode(mode) => with_value(text, &["wallpaper", "mode"], mode.name().into()),
        Change::WallpaperDefinition(WallpaperDefinition::Auto) => {
            without(text, &["wallpaper", "definition"])
        }
        Change::WallpaperDefinition(definition) => {
            with_value(text, &["wallpaper", "definition"], definition.name().into())
        }
        Change::AudioNick { device, nick } => {
            let nick = nick.as_deref().map(str::trim).filter(|n| !n.is_empty());
            match nick {
                Some(nick) => with_value(text, &["audio", "nicks", device], Value::from(nick)),
                None => without(text, &["audio", "nicks", device]),
            }
        }
        Change::AudioVisibility { device, visible } => {
            let current = Settings::parse(text).audio_hidden;
            let mut updated: Vec<String> = current.into_iter().filter(|d| d != device).collect();
            if !*visible {
                updated.push(device.clone());
            }
            if updated.is_empty() {
                without(text, &["audio", "hidden"])
            } else {
                let array: toml_edit::Array = updated.iter().map(String::as_str).collect();
                with_value(text, &["audio", "hidden"], Value::Array(array))
            }
        }
    }
}

/// The connectors of the screens once `connector` is given to `role`. The
/// screen that had it takes the one `role` had: two screens never share a
/// connector.
pub fn assign(
    outputs: &PerScreen<Option<String>>,
    role: OutputRole,
    connector: &str,
) -> PerScreen<Option<String>> {
    let mut assigned = outputs.clone();
    let previous = assigned.get_mut(role).replace(connector.to_owned());
    for other in PerScreen::<()>::ROLES {
        if other != role && assigned.get(other).as_deref() == Some(connector) {
            *assigned.get_mut(other) = previous.clone();
        }
    }
    assigned
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file installed for the user.
    const SAMPLE: &str = include_str!("../../../config/config.toml");

    fn screens(left: &str, center: &str, right: &str) -> PerScreen<Option<String>> {
        let name = |name: &str| (!name.is_empty()).then(|| name.to_owned());
        PerScreen {
            left: name(left),
            center: name(center),
            right: name(right),
        }
    }

    #[test]
    fn an_empty_file_leaves_everything_to_its_default() {
        let settings = Settings::parse("");

        assert_eq!(settings, Settings::default());
        assert_eq!(settings.theme(), ThemeName::Light);
        assert!(settings.xwayland() && settings.direct_scanout() && settings.vrr());
        assert!(settings.animations());
        assert_eq!(settings.sticky_edges_ms(), 300);
        assert_eq!(settings.dock(OutputRole::Left), DockPlace::Vertical);
        assert_eq!(settings.dock(OutputRole::Center), DockPlace::Off);
        assert_eq!(settings.dock(OutputRole::Right), DockPlace::Off);
        assert_eq!(settings.mouse_accel_speed(), 0.0);
        assert_eq!(settings.mouse_accel_profile(), MouseAccelProfile::Adaptive);
        assert!(!settings.mouse_natural_scroll() && !settings.mouse_left_handed());
    }

    #[test]
    fn every_setting_is_read() {
        let settings = Settings::parse(
            r##"
            theme = "dark"
            accent = "#FF8800"
            terminal = "foot"
            xwayland = false

            [outputs]
            left = "HDMI-A-1"
            center = "DP-2"

            [keyboard]
            layout = "fr"
            variant = "bepo"

            [game]
            sticky_edges_ms = 0
            direct_scanout = false
            vrr = false

            [dock.screens]
            left = "off"
            center = "vertical-right"
            right = "horizontal"

            [mouse]
            accel_speed = 0.2
            accel_profile = "flat"
            natural_scroll = true
            left_handed = true
            "##,
        );

        assert_eq!(settings.theme, Some(ThemeName::Dark));
        assert_eq!(settings.accent.as_deref(), Some("#FF8800"));
        assert_eq!(settings.terminal.as_deref(), Some("foot"));
        assert!(!settings.xwayland());
        assert_eq!(settings.outputs, screens("HDMI-A-1", "DP-2", ""));
        assert_eq!(settings.keyboard_layout.as_deref(), Some("fr"));
        assert_eq!(settings.keyboard_variant.as_deref(), Some("bepo"));
        assert_eq!(settings.sticky_edges_ms(), 0);
        assert!(!settings.direct_scanout() && !settings.vrr());
        assert_eq!(settings.dock(OutputRole::Left), DockPlace::Off);
        assert_eq!(settings.dock(OutputRole::Center), DockPlace::VerticalRight);
        assert_eq!(settings.dock(OutputRole::Right), DockPlace::Horizontal);
        assert_eq!(settings.mouse_accel_speed, Some(0.2));
        assert_eq!(settings.mouse_accel_profile, Some(MouseAccelProfile::Flat));
        assert!(settings.mouse_natural_scroll() && settings.mouse_left_handed());
    }

    #[test]
    fn what_means_nothing_is_not_set() {
        let settings = Settings::parse(
            "theme = 3\naccent = \"blue\"\nxwayland = \"no\"\n[game]\nsticky_edges_ms = -5\n\
             [keyboard]\nvariant = \"\"\n[dock.screens]\nleft = \"diagonal\"\n\
             [mouse]\naccel_profile = \"diagonal\"\n",
        );

        assert_eq!(settings, Settings::default());
        assert_eq!(Settings::parse("theme = \n[dock"), Settings::default());
    }

    #[test]
    fn the_sample_file_names_the_screens_and_nothing_else() {
        let settings = Settings::parse(SAMPLE);

        let expected = Settings {
            outputs: screens("HDMI-A-1", "DP-2", "DP-1"),
            ..Settings::default()
        };
        assert_eq!(settings, expected);
    }

    #[test]
    fn every_change_is_read_back_from_the_sample_file() {
        let changes = [
            Change::Theme(ThemeName::Dark),
            Change::Accent(Some("#ff8800".into())),
            Change::Terminal(Some("foot".into())),
            Change::Xwayland(false),
            Change::Outputs(screens("DP-1", "DP-2", "HDMI-A-1")),
            Change::Keyboard {
                layout: Some("fr".into()),
                variant: Some("bepo".into()),
            },
            Change::StickyEdges(450),
            Change::DirectScanout(false),
            Change::Vrr(false),
            Change::Animations(false),
            Change::Dock(OutputRole::Left, DockPlace::Off),
            Change::Dock(OutputRole::Center, DockPlace::VerticalRight),
            Change::Dock(OutputRole::Right, DockPlace::Horizontal),
            Change::Layout(OutputRole::Right, LayoutKind::MainSide),
            Change::Scale(OutputRole::Center, Scale::new(2.0)),
            Change::MouseAccelSpeed(0.5),
            Change::MouseAccelProfile(MouseAccelProfile::Flat),
            Change::MouseNaturalScroll(true),
            Change::MouseLeftHanded(true),
            Change::TouchpadTap(false),
            Change::TouchpadNaturalScroll(false),
            Change::TouchpadAccelSpeed(0.25),
            Change::TouchpadClick(TouchpadClick::Areas),
            Change::WallpaperPath(Some("/home/user/wallpaper.png".into())),
            Change::WallpaperMode(WallpaperMode::Fit),
            Change::WallpaperDefinition(WallpaperDefinition::Uhd),
            Change::AudioNick {
                device: "alsa_output.headphones".into(),
                nick: Some("Mon Casque".into()),
            },
            Change::AudioVisibility {
                device: "alsa_output.hdmi".into(),
                visible: false,
            },
        ];

        let mut text = SAMPLE.to_owned();
        for change in &changes {
            text = apply(&text, change).unwrap();
        }

        let expected = Settings {
            theme: Some(ThemeName::Dark),
            accent: Some("#ff8800".into()),
            terminal: Some("foot".into()),
            xwayland: Some(false),
            outputs: screens("DP-1", "DP-2", "HDMI-A-1"),
            keyboard_layout: Some("fr".into()),
            keyboard_variant: Some("bepo".into()),
            sticky_edges_ms: Some(450),
            direct_scanout: Some(false),
            vrr: Some(false),
            animations: Some(false),
            docks: PerScreen {
                left: Some(DockPlace::Off),
                center: Some(DockPlace::VerticalRight),
                right: Some(DockPlace::Horizontal),
            },
            layouts: PerScreen {
                right: Some(LayoutKind::MainSide),
                ..PerScreen::default()
            },
            scales: PerScreen {
                center: Some(Scale::new(2.0)),
                ..PerScreen::default()
            },
            dock_icon_size: None,
            mouse_accel_speed: Some(0.5),
            mouse_accel_profile: Some(MouseAccelProfile::Flat),
            mouse_natural_scroll: Some(true),
            mouse_left_handed: Some(true),
            touchpad_tap: Some(false),
            touchpad_tap_drag: None,
            touchpad_disable_while_typing: None,
            touchpad_natural_scroll: Some(false),
            touchpad_accel_speed: Some(0.25),
            touchpad_click: Some(TouchpadClick::Areas),
            wallpaper_path: Some("/home/user/wallpaper.png".into()),
            wallpapers: PerScreen::default(),
            wallpaper_mode: Some(WallpaperMode::Fit),
            wallpaper_definition: Some(WallpaperDefinition::Uhd),
            audio_hidden: vec!["alsa_output.hdmi".into()],
            audio_nicks: BTreeMap::from([("alsa_output.headphones".into(), "Mon Casque".into())]),
        };
        assert_eq!(Settings::parse(&text), expected);
        // The favorites of the dock, which no change is about, are still there.
        assert!(
            text.contains(
                "favorites = [\"firefox\", \"discord\", \"kitty\", \"code\", \"obsidian\"]"
            )
        );
    }

    #[test]
    fn the_sample_file_keeps_each_setting_under_its_explanation() {
        let text = apply(SAMPLE, &Change::Theme(ThemeName::Dark)).unwrap();
        let text = apply(&text, &Change::Vrr(false)).unwrap();
        let text = apply(
            &text,
            &Change::Dock(OutputRole::Center, DockPlace::Horizontal),
        )
        .unwrap();

        assert!(text.contains("réécrit cette ligne.\ntheme = \"dark\"\n"));
        assert!(text.contains("En cas de scintillement, mettre `false`.\nvrr = false\n"));
        assert!(
            text.contains("# left = \"vertical\"\ncenter = \"horizontal\"\n# right = \"off\"\n")
        );
        assert!(text.starts_with("# Configuration de coquille"));
    }

    #[test]
    fn going_back_to_a_default_removes_the_line() {
        let text = "accent = \"#ff8800\"\nterminal = \"foot\"\n\n[keyboard]\nlayout = \"fr\"\nvariant = \"bepo\"\n";

        let text = apply(text, &Change::Accent(None)).unwrap();
        let text = apply(&text, &Change::Terminal(None)).unwrap();
        let keyboard = Change::Keyboard {
            layout: None,
            variant: Some("bepo".into()),
        };
        let text = apply(&text, &keyboard).unwrap();

        assert_eq!(Settings::parse(&text), Settings::default());
    }

    #[test]
    fn an_accent_that_is_not_a_color_is_never_written() {
        let text = apply(
            "accent = \"#ff8800\"\n",
            &Change::Accent(Some("orange".into())),
        );

        assert_eq!(Settings::parse(&text.unwrap()).accent, None);
        let text = apply("", &Change::Accent(Some("#FF8800".into()))).unwrap();
        assert_eq!(text, "accent = \"#ff8800\"\n");
    }

    #[test]
    fn a_broken_file_is_left_alone() {
        assert_eq!(apply("theme = \n[dock", &Change::Vrr(false)), None);
    }

    #[test]
    fn giving_a_screen_the_connector_of_another_swaps_them() {
        let outputs = screens("HDMI-A-1", "DP-2", "DP-1");

        let swapped = assign(&outputs, OutputRole::Left, "DP-1");

        assert_eq!(swapped, screens("DP-1", "DP-2", "HDMI-A-1"));
        assert_eq!(assign(&outputs, OutputRole::Left, "HDMI-A-1"), outputs);
    }

    #[test]
    fn a_connector_nobody_had_is_simply_given() {
        let outputs = screens("", "DP-2", "");

        assert_eq!(
            assign(&outputs, OutputRole::Left, "DP-3"),
            screens("DP-3", "DP-2", "")
        );
        // The screen that had the connector gets nothing in exchange.
        assert_eq!(
            assign(&outputs, OutputRole::Right, "DP-2"),
            screens("", "", "DP-2")
        );
    }

    #[test]
    fn wallpaper_settings_can_be_parsed_and_changed() {
        let text = "[wallpaper]\npath = \"/usr/share/bg.png\"\nleft = \"/usr/share/left.png\"\nmode = \"fit\"\n";
        let settings = Settings::parse(text);
        assert_eq!(
            settings.wallpaper_path.as_deref(),
            Some("/usr/share/bg.png")
        );
        assert_eq!(
            settings.wallpaper(OutputRole::Left),
            Some("/usr/share/left.png")
        );
        assert_eq!(
            settings.wallpaper(OutputRole::Center),
            Some("/usr/share/bg.png")
        );
        assert_eq!(settings.wallpaper_mode(), WallpaperMode::Fit);

        let text = apply(text, &Change::WallpaperMode(WallpaperMode::Fill)).unwrap();
        let text = apply(
            &text,
            &Change::Wallpaper(OutputRole::Right, Some("/usr/share/right.png".into())),
        )
        .unwrap();
        let settings = Settings::parse(&text);
        assert_eq!(settings.wallpaper_mode(), WallpaperMode::Fill);
        assert_eq!(
            settings.wallpaper(OutputRole::Right),
            Some("/usr/share/right.png")
        );

        let text = apply(&text, &Change::Wallpaper(OutputRole::Right, None)).unwrap();
        let settings = Settings::parse(&text);
        assert_eq!(settings.wallpapers.right, None);
    }

    #[test]
    fn the_wallpaper_definition_goes_back_to_automatic_by_removing_its_line() {
        let text = "[wallpaper]\nmode = \"fit\"\n";
        assert_eq!(
            Settings::parse(text).wallpaper_definition(),
            WallpaperDefinition::Auto
        );

        let text = apply(text, &Change::WallpaperDefinition(WallpaperDefinition::Hd)).unwrap();
        assert!(text.contains("definition = \"1080p\""));
        assert_eq!(
            Settings::parse(&text).wallpaper_definition(),
            WallpaperDefinition::Hd
        );

        let text = apply(
            &text,
            &Change::WallpaperDefinition(WallpaperDefinition::Auto),
        )
        .unwrap();
        assert!(!text.contains("definition"));
        assert!(text.contains("mode = \"fit\""));
    }

    #[test]
    fn layouts_are_read_and_written_under_their_explanation() {
        let settings = Settings::parse(SAMPLE);
        assert_eq!(settings.layout(OutputRole::Left), LayoutKind::TwoColumns);
        assert_eq!(settings.layout(OutputRole::Center), LayoutKind::Full);

        let changed = apply(
            SAMPLE,
            &Change::Layout(OutputRole::Center, LayoutKind::ThreeColumns),
        )
        .unwrap();

        let settings = Settings::parse(&changed);
        assert_eq!(
            settings.layout(OutputRole::Center),
            LayoutKind::ThreeColumns
        );
        assert_eq!(settings.layouts.left, None);
        assert!(changed.contains("center = \"three-columns\""));
        // Switched on where the file suggested it, not added again.
        assert_eq!(
            changed.matches("center = ").count(),
            SAMPLE.matches("center = ").count()
        );
        assert_eq!(
            Settings::parse("[layout]\nleft = \"rows\"\n").layouts.left,
            None
        );
    }

    #[test]
    fn scales_are_read_and_written_under_their_explanation() {
        let settings = Settings::parse(SAMPLE);
        assert_eq!(settings.scale(OutputRole::Center), Scale::ONE);

        let changed = apply(SAMPLE, &Change::Scale(OutputRole::Center, Scale::new(1.75))).unwrap();

        let settings = Settings::parse(&changed);
        assert_eq!(settings.scale(OutputRole::Center).get(), 1.75);
        assert_eq!(settings.scales.left, None);
        assert!(changed.contains("[scale]\n# left = 1.0\ncenter = 1.75\n"));
        assert_eq!(
            Settings::parse("[scale]\nleft = 2\n")
                .scale(OutputRole::Left)
                .get(),
            2.0
        );
        assert_eq!(
            Settings::parse("[scale]\nleft = 9.0\n")
                .scale(OutputRole::Left)
                .get(),
            3.0
        );
    }

    #[test]
    fn a_lone_screen_always_has_a_dock_unless_turned_off() {
        let nothing = PerScreen::default();
        let center = [OutputRole::Center];

        // The usual dock is on the left screen, which a laptop does not have.
        assert_eq!(docks_for(&nothing, &center).center, DockPlace::Vertical);
        // One the user chose stays as chosen.
        let horizontal = PerScreen {
            center: Some(DockPlace::Horizontal),
            ..PerScreen::default()
        };
        assert_eq!(
            docks_for(&horizontal, &center).center,
            DockPlace::Horizontal
        );
        // Turned off on purpose: off.
        let off = PerScreen {
            center: Some(DockPlace::Off),
            ..PerScreen::default()
        };
        assert_eq!(docks_for(&off, &center).center, DockPlace::Off);
    }

    #[test]
    fn three_screens_keep_the_usual_dock_on_the_left() {
        let all = PerScreen::<()>::ROLES;
        let docks = docks_for(&PerScreen::default(), &all);

        assert_eq!(docks.left, DockPlace::Vertical);
        assert_eq!(
            (docks.center, docks.right),
            (DockPlace::Off, DockPlace::Off)
        );
        assert_eq!(docks_for(&PerScreen::default(), &[]), docks);
        // Two screens without the left one: the center gets it.
        let two = docks_for(
            &PerScreen::default(),
            &[OutputRole::Center, OutputRole::Right],
        );
        assert_eq!(
            (two.center, two.right),
            (DockPlace::Vertical, DockPlace::Off)
        );
        // The left one turned off on purpose: no dock anywhere.
        let off = PerScreen {
            left: Some(DockPlace::Off),
            ..PerScreen::default()
        };
        let none = docks_for(&off, &all);
        assert_eq!(none, PerScreen::from_roles(|_| DockPlace::Off));
    }

    #[test]
    fn dock_icon_size_can_be_parsed_and_changed() {
        assert_eq!(Settings::parse("").dock_icon_size(), 24);
        assert_eq!(Settings::parse("").dock_icon_size, None);

        let text = "[dock]\nicon_size = 32\n";
        let settings = Settings::parse(text);
        assert_eq!(settings.dock_icon_size, Some(32));
        assert_eq!(settings.dock_icon_size(), 32);

        let changed = apply(text, &Change::DockIconSize(28)).unwrap();
        let settings = Settings::parse(&changed);
        assert_eq!(settings.dock_icon_size, Some(28));
        assert_eq!(settings.dock_icon_size(), 28);
    }

    #[test]
    fn audio_settings_can_be_parsed_and_changed() {
        let text = r#"
        [audio]
        hidden = ["alsa_output.hdmi"]

        [audio.nicks]
        "alsa_output.usb-headset" = "Casque"
        "#;
        let settings = Settings::parse(text);
        assert!(!settings.is_audio_sink_visible("alsa_output.hdmi"));
        assert!(settings.is_audio_sink_visible("alsa_output.usb-headset"));
        assert_eq!(
            settings.audio_sink_nick("alsa_output.usb-headset"),
            Some("Casque")
        );
        assert_eq!(
            settings.audio_sink_display_name("alsa_output.usb-headset", "USB Headset"),
            "Casque"
        );
        assert_eq!(
            settings.audio_sink_display_name("alsa_output.hdmi", "HDMI Screen"),
            "HDMI Screen"
        );

        let text = apply(
            text,
            &Change::AudioNick {
                device: "alsa_output.hdmi".into(),
                nick: Some("Écran TV".into()),
            },
        )
        .unwrap();
        let settings = Settings::parse(&text);
        assert_eq!(
            settings.audio_sink_nick("alsa_output.hdmi"),
            Some("Écran TV")
        );

        let text = apply(
            &text,
            &Change::AudioNick {
                device: "alsa_output.hdmi".into(),
                nick: None,
            },
        )
        .unwrap();
        let settings = Settings::parse(&text);
        assert_eq!(settings.audio_sink_nick("alsa_output.hdmi"), None);

        let text = apply(
            &text,
            &Change::AudioVisibility {
                device: "alsa_output.hdmi".into(),
                visible: true,
            },
        )
        .unwrap();
        let settings = Settings::parse(&text);
        assert!(settings.is_audio_sink_visible("alsa_output.hdmi"));

        let text = apply(
            &text,
            &Change::AudioVisibility {
                device: "alsa_output.speaker".into(),
                visible: false,
            },
        )
        .unwrap();
        let settings = Settings::parse(&text);
        assert!(!settings.is_audio_sink_visible("alsa_output.speaker"));
    }
}
