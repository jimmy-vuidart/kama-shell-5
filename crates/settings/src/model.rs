//! What the settings screen offers and shows, without the interface: the
//! pages, the choices of each list, the drawing of the three screens.

use std::ffi::OsStr;
use std::fmt;
use std::path::Path;

use coquille_config::{PerScreen, Settings};
use coquille_ipc::Snapshot;
use coquille_model::{OutputRole, Scale};

/// A page of the settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Page {
    #[default]
    Appearance,
    Wallpaper,
    Screens,
    Audio,
    Game,
    Mouse,
    Shortcuts,
    System,
}

impl Page {
    pub const ALL: [Self; 8] = [
        Self::Appearance,
        Self::Wallpaper,
        Self::Screens,
        Self::Audio,
        Self::Game,
        Self::Mouse,
        Self::Shortcuts,
        Self::System,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Appearance => "Apparence",
            Self::Wallpaper => "Fond d'écran",
            Self::Screens => "Écrans et dock",
            Self::Audio => "Son",
            Self::Game => "Mode jeu",
            Self::Mouse => "Souris et pavé",
            Self::Shortcuts => "Raccourcis clavier",
            Self::System => "Système",
        }
    }

    /// What the page is about, under its name.
    pub fn summary(self) -> &'static str {
        match self {
            Self::Appearance => "Le thème et la couleur d'accent.",
            Self::Wallpaper => "L'image derrière les fenêtres, et sa définition.",
            Self::Screens => "Quel écran est lequel, et où se tient le dock.",
            Self::Audio => {
                "Les périphériques de sortie, leurs surnoms et leur présence dans le lanceur."
            }
            Self::Game => "Ce qui se passe sur l'écran d'un jeu en plein écran.",
            Self::Mouse => {
                "La vitesse du pointeur, le profil d'accélération, le défilement et le pavé \
                 tactile."
            }
            Self::Shortcuts => "Les raccourcis clavier et gestes du shell, en lecture seule.",
            Self::System => "Le clavier, le terminal et les applications X11.",
        }
    }
}

/// The user-visible name of a screen.
pub fn screen_name(role: OutputRole) -> &'static str {
    match role {
        OutputRole::Left => "Écran gauche",
        OutputRole::Center => "Écran central",
        OutputRole::Right => "Écran droit",
    }
}

/// The name of a screen on the page: a lone screen (a laptop) is simply
/// « Écran ».
pub fn screen_title(role: OutputRole, shown: usize) -> &'static str {
    if shown == 1 {
        "Écran"
    } else {
        screen_name(role)
    }
}

/// The screens the page shows: the ones the compositor has, else (outside
/// of coquille, to set the file for the next start) all three.
pub fn shown_roles(snapshot: Option<&Snapshot>) -> Vec<OutputRole> {
    let live = snapshot.map(|snapshot| &snapshot.screens);
    match live.filter(|screens| !screens.is_empty()) {
        Some(screens) => PerScreen::<()>::ROLES
            .into_iter()
            .filter(|role| screens.iter().any(|screen| screen.role == *role))
            .collect(),
        None => PerScreen::<()>::ROLES.to_vec(),
    }
}

/// The page shows the touchpad's settings: there is one, or this does not
/// run inside coquille (the file is set for the next start).
pub fn shows_touchpad(snapshot: Option<&Snapshot>) -> bool {
    snapshot.is_none_or(|snapshot| snapshot.touchpad)
}

/// The scales offered, from 100 % to 300 %.
pub fn scale_choices() -> Vec<Choice> {
    Scale::ALL
        .iter()
        .map(|scale| Choice::new(&scale.get().to_string(), scale.label()))
        .collect()
}

/// The scale a line of [`scale_choices`] stands for.
pub fn scale_of(choice: &Choice) -> Scale {
    let value = choice.value.as_deref().and_then(|value| value.parse().ok());
    Scale::new(value.unwrap_or(1.0))
}

/// The line of [`scale_choices`] for `scale`.
pub fn scale_choice(scale: Scale) -> Option<Choice> {
    chosen(&scale_choices(), Some(&scale.get().to_string()))
}

/// A line of a list: what it says, and what choosing it writes. `None` is
/// the choice that leaves the setting to its default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: Option<String>,
    pub label: String,
}

impl Choice {
    fn new(value: &str, label: impl Into<String>) -> Self {
        Self {
            value: Some(value.to_owned()),
            label: label.into(),
        }
    }

    fn default(label: impl Into<String>) -> Self {
        Self {
            value: None,
            label: label.into(),
        }
    }
}

impl fmt::Display for Choice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// The line of `choices` that stands for `value`.
pub fn chosen(choices: &[Choice], value: Option<&str>) -> Option<Choice> {
    let found = choices
        .iter()
        .find(|choice| choice.value.as_deref() == value);
    found.cloned()
}

/// The accents offered besides the one of the theme, as `#rrggbb`.
pub const ACCENTS: [&str; 9] = [
    "#5b63e6", "#2f6fed", "#7c5cf5", "#c6409f", "#e5484d", "#f08a24", "#c99a06", "#2f9e63",
    "#0f9aa8",
];

/// The keyboard layouts XKB knows, and the variants of each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layouts {
    /// Name (`fr`) and description (`French`).
    layouts: Vec<(String, String)>,
    /// Layout, name (`bepo`) and description (`French (BEPO)`).
    variants: Vec<(String, String, String)>,
}

impl Layouts {
    /// Where XKB lists them.
    pub const LIST: &str = "/usr/share/X11/xkb/rules/evdev.lst";

    pub fn load() -> Self {
        Self::parse(&std::fs::read_to_string(Self::LIST).unwrap_or_default())
    }

    /// Reads the list of XKB: sections opened by `! name`, then one line
    /// for each entry, a name and its description.
    pub fn parse(list: &str) -> Self {
        let mut found = Self::default();
        let mut section = "";
        for line in list.lines() {
            if let Some(name) = line.strip_prefix('!') {
                section = name.trim();
                continue;
            }
            let Some((name, description)) = line.trim().split_once(char::is_whitespace) else {
                continue;
            };
            let description = description.trim();
            match section {
                "layout" => found
                    .layouts
                    .push((name.to_owned(), description.to_owned())),
                "variant" => {
                    // `fr: French (BEPO)`.
                    if let Some((layout, description)) = description.split_once(": ") {
                        let variant = (layout.to_owned(), name.to_owned(), description.to_owned());
                        found.variants.push(variant);
                    }
                }
                _ => {}
            }
        }
        // By name: `fr` is easier to find than "French" in a long list.
        found.layouts.sort();
        found
    }

    /// The layouts to choose from; a layout the file names and XKB does not
    /// list is still shown, so that the list never hides what is set.
    pub fn layouts(&self, configured: Option<&str>) -> Vec<Choice> {
        let mut choices = vec![Choice::default("Celle du système")];
        let known = self.layouts.iter();
        choices.extend(known.map(|(name, text)| Choice::new(name, format!("{name} — {text}"))));
        if let Some(name) = configured.filter(|name| chosen(&choices, Some(name)).is_none()) {
            choices.push(Choice::new(name, name));
        }
        choices
    }

    /// The variants of a layout to choose from.
    pub fn variants(&self, layout: &str, configured: Option<&str>) -> Vec<Choice> {
        let mut choices = vec![Choice::default("Par défaut")];
        let own = self.variants.iter().filter(|(of, ..)| of == layout);
        choices.extend(own.map(|(_, name, text)| Choice::new(name, format!("{name} — {text}"))));
        if let Some(name) = configured.filter(|name| chosen(&choices, Some(name)).is_none()) {
            choices.push(Choice::new(name, name));
        }
        choices
    }
}

/// The terminals looked for, in the order the compositor prefers them: it
/// opens the first of the first three when the file names none.
const TERMINALS: [&str; 10] = [
    "kitty",
    "alacritty",
    "foot",
    "ghostty",
    "wezterm",
    "konsole",
    "ptyxis",
    "kgx",
    "gnome-terminal",
    "xterm",
];

/// Whether a program is in the `PATH`.
pub fn installed(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| directory.join(name).is_file())
    })
}

/// The terminals found on the system.
pub fn installed_terminals() -> Vec<&'static str> {
    let found = TERMINALS.into_iter().filter(|name| installed(name));
    found.collect()
}

/// The terminals to choose from: the automatic choice, saying which one it
/// is, then the installed ones, then the command the file names if it is
/// none of them.
pub fn terminals(installed: impl Fn(&str) -> bool, configured: Option<&str>) -> Vec<Choice> {
    let automatic = TERMINALS[..3].iter().find(|name| installed(name));
    let automatic = automatic.copied().unwrap_or(TERMINALS[0]);
    let mut choices = vec![Choice::default(format!("Automatique ({automatic})"))];
    let found = TERMINALS.iter().filter(|name| installed(name));
    choices.extend(found.map(|name| Choice::new(name, *name)));
    if let Some(command) = configured.filter(|name| chosen(&choices, Some(name)).is_none()) {
        choices.push(Choice::new(command, command));
    }
    choices
}

/// A screen as the page draws and names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCard {
    pub role: OutputRole,
    /// The connector the screen is on, or will be at the next start.
    pub connector: Option<String>,
    /// In pixels, when the compositor says.
    pub size: Option<(i32, i32)>,
    /// What the compositor draws it at.
    pub scale: Option<Scale>,
}

impl ScreenCard {
    /// `HDMI-A-1 · 1920 × 1080`, and its scale when it is not 100 %.
    pub fn detail(&self) -> String {
        let scale = self.scale.filter(|scale| *scale != Scale::ONE);
        let size = self.size.map(|(width, height)| match scale {
            Some(scale) => format!("{width} × {height} · {}", scale.label()),
            None => format!("{width} × {height}"),
        });
        match (&self.connector, size) {
            (Some(connector), Some(size)) => format!("{connector} · {size}"),
            (Some(connector), None) => connector.clone(),
            (None, Some(size)) => size,
            (None, None) => "Non branché".to_owned(),
        }
    }
}

/// The three screens: their connectors as the file names them, else as the
/// compositor found them, and their sizes.
pub fn screens(settings: &Settings, snapshot: Option<&Snapshot>) -> PerScreen<ScreenCard> {
    let card = |role: OutputRole| {
        let live = snapshot.and_then(|snapshot| snapshot.screens.iter().find(|s| s.role == role));
        // Made-up screens have made-up names: only the file names connectors.
        let real = live.filter(|_| snapshot.is_some_and(|snapshot| !snapshot.simulated));
        ScreenCard {
            role,
            connector: settings
                .outputs
                .get(role)
                .clone()
                .or_else(|| real.map(|screen| screen.name.clone())),
            size: live.map(|screen| {
                // Older compositors only said the logical size.
                if screen.pixel_width > 0 && screen.pixel_height > 0 {
                    (screen.pixel_width, screen.pixel_height)
                } else {
                    (screen.width, screen.height)
                }
            }),
            scale: live.map(|screen| screen.scale),
        }
    };
    PerScreen {
        left: card(OutputRole::Left),
        center: card(OutputRole::Center),
        right: card(OutputRole::Right),
    }
}

/// The connector of each screen, as [`screens`] shows them.
pub fn connectors(cards: &PerScreen<ScreenCard>) -> PerScreen<Option<String>> {
    PerScreen {
        left: cards.left.connector.clone(),
        center: cards.center.connector.clone(),
        right: cards.right.connector.clone(),
    }
}

/// The connectors a screen can be given: the ones of the screens plugged
/// in, and the ones the file names.
pub fn connector_choices(settings: &Settings, snapshot: Option<&Snapshot>) -> Vec<Choice> {
    let real = snapshot.filter(|snapshot| !snapshot.simulated);
    let plugged = real.into_iter().flat_map(|snapshot| &snapshot.screens);
    let mut names: Vec<String> = plugged.map(|screen| screen.name.clone()).collect();
    for role in PerScreen::<()>::ROLES {
        names.extend(settings.outputs.get(role).clone());
    }
    names.sort();
    names.dedup();
    names.iter().map(|name| Choice::new(name, name)).collect()
}

/// The size of each screen on the drawing of the page, `width` wide in all
/// without the gaps (half of it for a lone screen, which would be drawn too
/// large). A screen of unknown size is drawn as the real setup has it: 21:9
/// in the center, 16:9 on the sides. A screen not `shown` is not drawn.
pub fn drawing(
    cards: &PerScreen<ScreenCard>,
    shown: &[OutputRole],
    width: f32,
) -> PerScreen<(f32, f32)> {
    let size = |card: &ScreenCard| {
        if !shown.contains(&card.role) {
            return (0.0, 0.0);
        }
        let usual = match card.role {
            OutputRole::Center => (3440, 1440),
            OutputRole::Left | OutputRole::Right => (1920, 1080),
        };
        let (w, h) = card.size.filter(|(w, h)| *w > 0 && *h > 0).unwrap_or(usual);
        (w as f32, h as f32)
    };
    let sizes = [size(&cards.left), size(&cards.center), size(&cards.right)];
    let width = if shown.len() == 1 { width / 2.0 } else { width };
    let scale = width / sizes.iter().map(|(w, _)| w).sum::<f32>().max(1.0);
    let scaled = |(w, h): (f32, f32)| ((w * scale).round(), (h * scale).round());
    PerScreen {
        left: scaled(sizes[0]),
        center: scaled(sizes[1]),
        right: scaled(sizes[2]),
    }
}

/// A path as the user writes it: `~` for the home directory.
pub fn short_path(path: &Path, home: Option<&OsStr>) -> String {
    let inside = home.and_then(|home| path.strip_prefix(home).ok());
    match inside {
        Some(inside) => format!("~/{}", inside.display()),
        None => path.display().to_string(),
    }
}

/// The delays the slider of the sticky edges offers, in milliseconds.
pub const STICKY_RANGE: std::ops::RangeInclusive<u32> = 100..=1000;
pub const STICKY_STEP: u32 = 50;

/// What the slider of the sticky edges shows for a delay of the file.
pub fn sticky_on_slider(delay: u64) -> u32 {
    let delay = u32::try_from(delay).unwrap_or(u32::MAX);
    delay.clamp(*STICKY_RANGE.start(), *STICKY_RANGE.end())
}

/// The pointer speed range on the slider, as a percentage (-100 to 100).
pub const MOUSE_SPEED_RANGE: std::ops::RangeInclusive<i32> = -100..=100;
pub const MOUSE_SPEED_STEP: i32 = 5;

/// Converts a speed from config [-1.0, 1.0] to a slider value [-100, 100].
pub fn speed_to_slider(speed: f64) -> i32 {
    let percent = (speed.clamp(-1.0, 1.0) * 100.0).round() as i32;
    percent.clamp(*MOUSE_SPEED_RANGE.start(), *MOUSE_SPEED_RANGE.end())
}

/// Converts a slider value [-100, 100] to a config speed [-1.0, 1.0].
pub fn slider_to_speed(slider: i32) -> f64 {
    let clamped = slider.clamp(*MOUSE_SPEED_RANGE.start(), *MOUSE_SPEED_RANGE.end());
    (clamped as f64) / 100.0
}

/// The dock icon sizes the slider offers, in pixels.
pub const DOCK_ICON_SIZE_RANGE: std::ops::RangeInclusive<u32> = 16..=64;
pub const DOCK_ICON_SIZE_STEP: u32 = 4;

/// What the slider of dock icon size shows for a size in the config.
pub fn dock_icon_size_on_slider(size: u32) -> u32 {
    let clamped = size.clamp(*DOCK_ICON_SIZE_RANGE.start(), *DOCK_ICON_SIZE_RANGE.end());
    let start = *DOCK_ICON_SIZE_RANGE.start();
    let rounded = ((clamped - start + DOCK_ICON_SIZE_STEP / 2) / DOCK_ICON_SIZE_STEP)
        * DOCK_ICON_SIZE_STEP
        + start;
    rounded.clamp(*DOCK_ICON_SIZE_RANGE.start(), *DOCK_ICON_SIZE_RANGE.end())
}

/// An audio output device known to PipeWire/PulseAudio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceCard {
    pub name: String,
    pub description: String,
    pub nick: Option<String>,
    pub visible: bool,
    pub active: bool,
}

/// Parses the output of `pactl -f json list sinks` and matches it with the current settings.
pub fn parse_audio_sinks(
    json: &str,
    default_sink: Option<&str>,
    settings: &Settings,
) -> Vec<AudioDeviceCard> {
    let sinks: Vec<serde_json::Value> = serde_json::from_str(json).unwrap_or_default();
    sinks
        .into_iter()
        .filter_map(|sink| {
            let name = sink["name"].as_str()?.to_owned();
            let described = sink["description"]
                .as_str()
                .filter(|text| !text.is_empty() && *text != "(null)");
            let nick_prop = sink["properties"]["node.nick"].as_str();
            let description = described.or(nick_prop).unwrap_or(&name).to_owned();
            let nick = settings.audio_sink_nick(&name).map(str::to_owned);
            let visible = settings.is_audio_sink_visible(&name);
            let active = default_sink.is_some_and(|def| def == name);
            Some(AudioDeviceCard {
                name,
                description,
                nick,
                visible,
                active,
            })
        })
        .collect()
}

/// Loads the audio sinks currently available on the system using `pactl`.
pub fn load_audio_sinks(settings: &Settings) -> Vec<AudioDeviceCard> {
    let default_sink = std::process::Command::new("pactl")
        .args(["-f", "json", "info"])
        .output()
        .ok()
        .and_then(|out| {
            let info: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
            info["default_sink_name"].as_str().map(str::to_owned)
        });
    let sinks_json = std::process::Command::new("pactl")
        .args(["-f", "json", "list", "sinks"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .unwrap_or_default();
    parse_audio_sinks(&sinks_json, default_sink.as_deref(), settings)
}

/// A group of keyboard shortcuts shown on the shortcuts page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutGroup {
    pub title: &'static str,
    pub entries: &'static [ShortcutEntry],
}

/// A single keyboard shortcut and what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutEntry {
    pub title: &'static str,
    pub detail: &'static str,
    /// The keys or chord to press, shown as keycaps separated by `+`.
    pub keys: &'static [&'static str],
}

/// The keyboard shortcuts and modifier gestures of the shell, grouped by section.
pub const SHORTCUT_GROUPS: [ShortcutGroup; 4] = [
    ShortcutGroup {
        title: "Lanceur et applications",
        entries: &[
            ShortcutEntry {
                title: "Tiroir d'applications",
                detail: "Ouvre ou ferme le tiroir du dock (flèches et Page suiv./préc. pour naviguer, Échap pour fermer).",
                keys: &["Super"],
            },
            ShortcutEntry {
                title: "Terminal",
                detail: "Ouvre le terminal choisi dans la page Système.",
                keys: &["Super", "Entrée"],
            },
            ShortcutEntry {
                title: "Paramètres",
                detail: "Ouvre cette fenêtre de paramètres.",
                keys: &["Super", "S"],
            },
            ShortcutEntry {
                title: "Capture d'écran",
                detail: "Sélectionne une région de l'écran à capturer (Échap pour annuler).",
                keys: &["Impr. écran"],
            },
        ],
    },
    ShortcutGroup {
        title: "Fenêtres et navigation",
        entries: &[
            ShortcutEntry {
                title: "Fenêtre suivante",
                detail: "Ouvre le sélecteur de fenêtres dans l'ordre d'utilisation (flèches pour choisir, Échap pour annuler).",
                keys: &["Alt", "Tab"],
            },
            ShortcutEntry {
                title: "Fenêtre précédente",
                detail: "Recule dans le sélecteur de fenêtres.",
                keys: &["Alt", "Maj", "Tab"],
            },
            ShortcutEntry {
                title: "Onglet suivant",
                detail: "Affiche la fenêtre suivante dans la pile de l'emplacement actif.",
                keys: &["Super", "Page suiv."],
            },
            ShortcutEntry {
                title: "Onglet précédent",
                detail: "Affiche la fenêtre précédente dans la pile de l'emplacement actif.",
                keys: &["Super", "Page préc."],
            },
            ShortcutEntry {
                title: "Restaurer une fenêtre réduite",
                detail: "Ramène la dernière fenêtre qui a été réduite.",
                keys: &["Super", "R"],
            },
            ShortcutEntry {
                title: "Ranger une fenêtre",
                detail: "Affiche les emplacements des écrans pour y déposer la fenêtre (Échap pendant un glisser annule).",
                keys: &["Super", "Glisser"],
            },
            ShortcutEntry {
                title: "Redimensionner les colonnes",
                detail: "Ajuste la largeur de deux colonnes sur leur séparation.",
                keys: &["Super", "Glisser"],
            },
        ],
    },
    ShortcutGroup {
        title: "Mode jeu",
        entries: &[
            ShortcutEntry {
                title: "Quitter le plein écran",
                detail: "Remet le jeu ancré en fenêtre.",
                keys: &["Super", "Échap"],
            },
            ShortcutEntry {
                title: "Libérer le pointeur",
                detail: "Libère le curseur retenu par le jeu et ouvre le tiroir jusqu'au prochain clic dans le jeu.",
                keys: &["Super"],
            },
        ],
    },
    ShortcutGroup {
        title: "Matériel et session",
        entries: &[
            ShortcutEntry {
                title: "Luminosité de l'écran",
                detail: "Augmente ou diminue la luminosité sur un portable, même en jeu.",
                keys: &["Luminosité −/+"],
            },
            ShortcutEntry {
                title: "Quitter la session",
                detail: "Ferme coquille et met fin à la session graphique.",
                keys: &["Super", "Maj", "E"],
            },
            ShortcutEntry {
                title: "Quitter la session (secours)",
                detail: "Arrête coquille même si la touche Super ne répond pas.",
                keys: &["Ctrl", "Alt", "Retour arrière"],
            },
            ShortcutEntry {
                title: "Changer de console virtuelle",
                detail: "Bascule vers un autre terminal système (VT 1 à 12).",
                keys: &["Ctrl", "Alt", "F1…F12"],
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use coquille_ipc::Screen;
    use coquille_model::LayoutKind;

    use super::*;

    const LIST: &str = "! model\n  pc104           Generic 104-key PC\n\n! layout\n  us              English (US)\n  fr              French\n  de              German\n\n! variant\n  bepo            fr: French (BEPO)\n  azerty          fr: French (AZERTY)\n  neo             de: German (Neo 2)\n\n! option\n  grp             Switching to another layout\n";

    fn labels(choices: &[Choice]) -> Vec<&str> {
        choices.iter().map(|choice| choice.label.as_str()).collect()
    }

    fn snapshot(simulated: bool) -> Snapshot {
        let screen = |name: &str, role, width, height| Screen {
            name: name.to_owned(),
            role,
            x: 0,
            y: 0,
            width,
            height,
            layout: LayoutKind::Full,
            anchored: false,
            scale: Default::default(),
            pixel_width: width,
            pixel_height: height,
        };
        Snapshot {
            simulated,
            screens: vec![
                screen("HDMI-A-1", OutputRole::Left, 1920, 1080),
                screen("DP-2", OutputRole::Center, 3440, 1440),
                screen("DP-1", OutputRole::Right, 1920, 1080),
            ],
            ..Snapshot::default()
        }
    }

    #[test]
    fn layouts_are_listed_by_name_after_the_one_of_the_system() {
        let layouts = Layouts::parse(LIST);

        assert_eq!(
            labels(&layouts.layouts(None)),
            [
                "Celle du système",
                "de — German",
                "fr — French",
                "us — English (US)"
            ]
        );
    }

    #[test]
    fn a_layout_has_its_own_variants_only() {
        let layouts = Layouts::parse(LIST);

        assert_eq!(
            labels(&layouts.variants("fr", None)),
            [
                "Par défaut",
                "bepo — French (BEPO)",
                "azerty — French (AZERTY)"
            ]
        );
        assert_eq!(labels(&layouts.variants("us", None)), ["Par défaut"]);
    }

    #[test]
    fn what_the_file_names_is_listed_even_when_xkb_does_not_know_it() {
        let layouts = Layouts::parse(LIST);

        let choices = layouts.layouts(Some("xx"));

        assert_eq!(chosen(&choices, Some("xx")).unwrap().label, "xx");
        assert_eq!(layouts.layouts(Some("fr")).len(), 4);
        assert_eq!(
            labels(&layouts.variants("fr", Some("oss"))).last(),
            Some(&"oss")
        );
    }

    #[test]
    fn the_real_list_of_the_system_is_understood() {
        let layouts = Layouts::load();

        // Not every machine that runs the tests has XKB data.
        if layouts.layouts.is_empty() {
            return;
        }
        assert!(chosen(&layouts.layouts(None), Some("fr")).is_some());
        assert!(layouts.variants("fr", None).len() > 1);
    }

    #[test]
    fn the_automatic_terminal_says_which_one_it_is() {
        let choices = terminals(|name| matches!(name, "foot" | "konsole"), None);

        assert_eq!(labels(&choices), ["Automatique (foot)", "foot", "konsole"]);
        assert_eq!(chosen(&choices, None).unwrap().label, "Automatique (foot)");
    }

    #[test]
    fn a_command_of_the_file_that_is_no_known_terminal_is_listed() {
        let choices = terminals(|name| name == "kitty", Some("kitty --single-instance"));

        assert_eq!(
            labels(&choices),
            ["Automatique (kitty)", "kitty", "kitty --single-instance"]
        );
        assert_eq!(labels(&terminals(|_| false, None)), ["Automatique (kitty)"]);
    }

    #[test]
    fn screens_are_named_by_the_file_then_by_the_compositor() {
        let settings = Settings::parse("[outputs]\nleft = \"DP-1\"\n");

        let cards = screens(&settings, Some(&snapshot(false)));

        assert_eq!(cards.left.detail(), "DP-1 · 1920 × 1080");
        assert_eq!(cards.center.detail(), "DP-2 · 3440 × 1440");
        let names = labels(&connector_choices(&settings, Some(&snapshot(false))))
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(names, ["DP-1", "DP-2", "HDMI-A-1"]);
    }

    #[test]
    fn made_up_screens_lend_their_size_not_their_name() {
        let settings = Settings::parse("[outputs]\ncenter = \"DP-2\"\n");

        let cards = screens(&settings, Some(&snapshot(true)));

        assert_eq!(cards.left.detail(), "1920 × 1080");
        assert_eq!(cards.center.detail(), "DP-2 · 3440 × 1440");
        assert_eq!(connector_choices(&settings, Some(&snapshot(true))).len(), 1);
    }

    #[test]
    fn without_a_compositor_only_the_file_speaks() {
        let cards = screens(&Settings::default(), None);

        assert_eq!(cards.right.detail(), "Non branché");
        assert_eq!(connectors(&cards), PerScreen::default());
    }

    #[test]
    fn the_drawing_keeps_the_proportions_of_the_screens() {
        let cards = screens(&Settings::default(), Some(&snapshot(false)));

        let all = PerScreen::<()>::ROLES;
        let drawn = drawing(&cards, &all, 364.0);

        assert_eq!(drawn.left, (96.0, 54.0));
        assert_eq!(drawn.center, (172.0, 72.0));
        assert_eq!(drawn.right, (96.0, 54.0));
        // Unknown sizes are drawn as the real setup.
        assert_eq!(
            drawing(&screens(&Settings::default(), None), &all, 364.0),
            drawn
        );
    }

    #[test]
    fn a_laptop_shows_one_screen_with_its_pixels_and_its_scale() {
        let mut laptop = snapshot(false);
        laptop.screens = vec![coquille_ipc::Screen {
            name: "eDP-1".into(),
            role: OutputRole::Center,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            layout: LayoutKind::Full,
            anchored: false,
            scale: Scale::new(2.0),
            pixel_width: 3840,
            pixel_height: 2160,
        }];

        let shown = shown_roles(Some(&laptop));
        let cards = screens(&Settings::default(), Some(&laptop));

        assert_eq!(shown, [OutputRole::Center]);
        assert_eq!(screen_title(OutputRole::Center, shown.len()), "Écran");
        assert_eq!(cards.center.detail(), "eDP-1 · 3840 × 2160 · 200\u{202f}%");
        let drawn = drawing(&cards, &shown, 364.0);
        assert_eq!(drawn.center, (182.0, 102.0));
        assert_eq!(drawn.left, (0.0, 0.0));
        assert_eq!(shown_roles(None).len(), 3);
    }

    #[test]
    fn every_scale_is_offered_and_read_back() {
        let choices = scale_choices();

        assert_eq!(choices.len(), 9);
        assert_eq!(choices[4].label, "200\u{202f}%");
        assert_eq!(scale_of(&choices[4]), Scale::new(2.0));
        assert_eq!(scale_choice(Scale::new(1.25)), Some(choices[1].clone()));
    }

    #[test]
    fn the_home_directory_is_written_as_a_tilde() {
        let home = Some(OsStr::new("/home/jimmy"));
        let inside = Path::new("/home/jimmy/.config/coquille/config.toml");

        assert_eq!(short_path(inside, home), "~/.config/coquille/config.toml");
        assert_eq!(
            short_path(Path::new("/etc/config.toml"), home),
            "/etc/config.toml"
        );
        assert_eq!(
            short_path(inside, None),
            "/home/jimmy/.config/coquille/config.toml"
        );
    }

    #[test]
    fn the_slider_shows_a_delay_it_can_reach() {
        assert_eq!(sticky_on_slider(300), 300);
        assert_eq!(sticky_on_slider(20), 100);
        assert_eq!(sticky_on_slider(5000), 1000);
    }

    #[test]
    fn mouse_speed_slider_conversions() {
        assert_eq!(speed_to_slider(0.0), 0);
        assert_eq!(speed_to_slider(0.5), 50);
        assert_eq!(speed_to_slider(-0.75), -75);
        assert_eq!(speed_to_slider(1.5), 100);
        assert_eq!(speed_to_slider(-2.0), -100);

        assert_eq!(slider_to_speed(0), 0.0);
        assert_eq!(slider_to_speed(50), 0.5);
        assert_eq!(slider_to_speed(-75), -0.75);
        assert_eq!(slider_to_speed(120), 1.0);
        assert_eq!(slider_to_speed(-150), -1.0);
    }

    #[test]
    fn dock_icon_size_slider_snaps_and_clamps() {
        assert_eq!(dock_icon_size_on_slider(24), 24);
        assert_eq!(dock_icon_size_on_slider(16), 16);
        assert_eq!(dock_icon_size_on_slider(64), 64);
        assert_eq!(dock_icon_size_on_slider(10), 16);
        assert_eq!(dock_icon_size_on_slider(70), 64);
        assert_eq!(dock_icon_size_on_slider(25), 24);
        assert_eq!(dock_icon_size_on_slider(27), 28);
    }

    #[test]
    fn every_page_has_a_name_and_summary() {
        for page in Page::ALL {
            assert!(!page.name().is_empty());
            assert!(!page.summary().is_empty());
        }
    }

    #[test]
    fn every_shortcut_group_and_entry_is_complete() {
        assert!(!SHORTCUT_GROUPS.is_empty());
        for group in SHORTCUT_GROUPS {
            assert!(!group.title.is_empty());
            assert!(!group.entries.is_empty());
            for entry in group.entries {
                assert!(!entry.title.is_empty());
                assert!(!entry.detail.is_empty());
                assert!(!entry.keys.is_empty());
                assert!(entry.keys.iter().all(|key| !key.is_empty()));
            }
        }
    }

    #[test]
    fn audio_sinks_are_parsed_and_configured() {
        let json = r#"[
            {
                "name": "alsa_output.pci-0000_01_00.1.hdmi-stereo",
                "description": "HDA NVidia Digital Stereo (HDMI)",
                "properties": { "node.nick": "HDMI Audio" }
            },
            {
                "name": "alsa_output.usb-SteelSeries_Arctis_Nova_Pro-00.analog-stereo",
                "description": "SteelSeries Arctis Nova Pro Wireless",
                "properties": { "node.nick": "Arctis Nova Pro" }
            }
        ]"#;

        let config = r#"
[audio]
hidden = ["alsa_output.pci-0000_01_00.1.hdmi-stereo"]

[audio.nicks]
"alsa_output.usb-SteelSeries_Arctis_Nova_Pro-00.analog-stereo" = "Casque gaming"
"#;
        let settings = Settings::parse(config);
        let sinks = parse_audio_sinks(
            json,
            Some("alsa_output.usb-SteelSeries_Arctis_Nova_Pro-00.analog-stereo"),
            &settings,
        );

        assert_eq!(sinks.len(), 2);

        assert_eq!(sinks[0].name, "alsa_output.pci-0000_01_00.1.hdmi-stereo");
        assert_eq!(sinks[0].description, "HDA NVidia Digital Stereo (HDMI)");
        assert_eq!(sinks[0].nick, None);
        assert!(!sinks[0].visible);
        assert!(!sinks[0].active);

        assert_eq!(
            sinks[1].name,
            "alsa_output.usb-SteelSeries_Arctis_Nova_Pro-00.analog-stereo"
        );
        assert_eq!(sinks[1].description, "SteelSeries Arctis Nova Pro Wireless");
        assert_eq!(sinks[1].nick.as_deref(), Some("Casque gaming"));
        assert!(sinks[1].visible);
        assert!(sinks[1].active);
    }
}
