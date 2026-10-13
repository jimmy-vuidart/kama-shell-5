//! What the settings screen offers and shows, without the interface: the
//! pages, the choices of each list, the drawing of the three screens.

use std::ffi::OsStr;
use std::fmt;
use std::path::Path;

use coquille_config::{PerScreen, Settings};
use coquille_ipc::Snapshot;
use coquille_model::OutputRole;

/// A page of the settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Page {
    #[default]
    Appearance,
    Screens,
    Game,
    System,
}

impl Page {
    pub const ALL: [Self; 4] = [Self::Appearance, Self::Screens, Self::Game, Self::System];

    pub fn name(self) -> &'static str {
        match self {
            Self::Appearance => "Apparence",
            Self::Screens => "Écrans et dock",
            Self::Game => "Mode jeu",
            Self::System => "Système",
        }
    }

    /// What the page is about, under its name.
    pub fn summary(self) -> &'static str {
        match self {
            Self::Appearance => "Le thème et la couleur d'accent des fenêtres et du dock.",
            Self::Screens => "Quel écran est lequel, et où se tient le dock.",
            Self::Game => "Ce qui se passe sur l'écran d'un jeu en plein écran.",
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
pub const ACCENTS: [&str; 8] = [
    "#2f6fed", "#7c5cf5", "#c6409f", "#e5484d", "#f08a24", "#c99a06", "#2f9e63", "#0f9aa8",
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
}

impl ScreenCard {
    /// `HDMI-A-1 · 1920 × 1080`.
    pub fn detail(&self) -> String {
        let size = self
            .size
            .map(|(width, height)| format!("{width} × {height}"));
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
            size: live.map(|screen| (screen.width, screen.height)),
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
/// without the gaps. A screen of unknown size is drawn as the real setup
/// has it: 21:9 in the center, 16:9 on the sides.
pub fn drawing(cards: &PerScreen<ScreenCard>, width: f32) -> PerScreen<(f32, f32)> {
    let size = |card: &ScreenCard| {
        let usual = match card.role {
            OutputRole::Center => (3440, 1440),
            OutputRole::Left | OutputRole::Right => (1920, 1080),
        };
        let (w, h) = card.size.filter(|(w, h)| *w > 0 && *h > 0).unwrap_or(usual);
        (w as f32, h as f32)
    };
    let sizes = [size(&cards.left), size(&cards.center), size(&cards.right)];
    let scale = width / sizes.iter().map(|(w, _)| w).sum::<f32>();
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
            width,
            height,
            layout: LayoutKind::Full,
            anchored: false,
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

        let drawn = drawing(&cards, 364.0);

        assert_eq!(drawn.left, (96.0, 54.0));
        assert_eq!(drawn.center, (172.0, 72.0));
        assert_eq!(drawn.right, (96.0, 54.0));
        // Unknown sizes are drawn as the real setup.
        assert_eq!(drawing(&screens(&Settings::default(), None), 364.0), drawn);
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
}
