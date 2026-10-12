//! The bar: which applications it shows, in what order, with what marks,
//! and what a click on one of them does.

use coquille_ipc::{Habit, Screen, Window};
use coquille_model::OutputRole;

use super::Identity;

/// The mark left of an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indicator {
    /// The application is not running.
    None,
    /// It has a window on the screen of the dock: an accent bar.
    Here,
    /// Its windows are on other screens: a grey dot.
    Elsewhere,
}

/// One application of the bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub identity: Identity,
    /// It is one of the favorites; otherwise it is only there because it
    /// has a window open, on whatever screen.
    pub pinned: bool,
    /// Its windows, the one focused last first.
    pub windows: Vec<Window>,
    pub indicator: Indicator,
    /// Shown under the name when the bar is unfolded.
    pub subtitle: String,
    /// Where the user tidied it, with the name the habit is kept under.
    pub habit: Option<Habit>,
}

/// What the bar needs to know of the world.
pub struct World<'a> {
    pub favorites: &'a [String],
    pub windows: &'a [Window],
    pub screens: &'a [Screen],
    pub habits: &'a [Habit],
    /// The screen the dock is on.
    pub here: OutputRole,
}

/// The applications of the bar: the favorites in the order of the
/// configuration, then the other applications open on any screen, by name.
/// There is one dock for the three screens: it shows them all.
pub fn entries(world: &World<'_>, identify: &dyn Fn(&str) -> Identity) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for favorite in world.favorites {
        let identity = identify(favorite);
        if entries
            .iter()
            .all(|entry| entry.identity.key != identity.key)
        {
            entries.push(blank(identity, true));
        }
    }
    let pinned = entries.len();
    for window in world.windows {
        let identity = identify(&window.app);
        let known = entries
            .iter_mut()
            .find(|entry| entry.identity.key == identity.key);
        match known {
            Some(entry) => entry.windows.push(window.clone()),
            None => {
                let mut entry = blank(identity, false);
                entry.windows.push(window.clone());
                entries.push(entry);
            }
        }
    }
    entries[pinned..].sort_by_key(|entry| super::drawer::fold(&entry.identity.name));

    for entry in &mut entries {
        let has = |name: &str| {
            entry
                .identity
                .names
                .iter()
                .any(|own| own.eq_ignore_ascii_case(name))
        };
        entry.habit = world.habits.iter().find(|habit| has(&habit.app)).cloned();
        let here = entry
            .windows
            .iter()
            .any(|window| window.screen == world.here);
        entry.indicator = match (entry.windows.is_empty(), here) {
            (true, _) => Indicator::None,
            (false, true) => Indicator::Here,
            (false, false) => Indicator::Elsewhere,
        };
        entry.subtitle = subtitle(entry, world);
    }
    entries
}

fn blank(identity: Identity, pinned: bool) -> Entry {
    Entry {
        identity,
        pinned,
        windows: Vec::new(),
        indicator: Indicator::None,
        subtitle: String::new(),
        habit: None,
    }
}

/// Where an application is, then what is worth knowing about it: "Ici",
/// "Écran droit · 2 fenêtres", "Écran central · en jeu".
fn subtitle(entry: &Entry, world: &World<'_>) -> String {
    let Some(latest) = entry.windows.first() else {
        return "Fermé".to_owned();
    };
    let here = entry
        .windows
        .iter()
        .any(|window| window.screen == world.here);
    let place = if here { "Ici" } else { latest.screen.label() };
    let playing = world
        .screens
        .iter()
        .any(|screen| screen.role == latest.screen && screen.anchored);
    let detail = if !here && playing && entry.identity.game {
        Some("en jeu".to_owned())
    } else if !entry.pinned {
        Some("non épinglée".to_owned())
    } else if entry.windows.iter().all(|window| window.minimized) {
        Some("réduite".to_owned())
    } else if entry.windows.len() > 1 {
        Some(format!("{} fenêtres", entry.windows.len()))
    } else {
        None
    };
    match detail {
        Some(detail) => format!("{place} · {detail}"),
        None => place.to_owned(),
    }
}

/// What a left click on an application does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// It is not running: start it.
    Launch,
    /// Show this window, restoring it if it was hidden.
    Activate(u64),
}

/// A click starts an application that is closed, and shows the window used
/// last of one that is open. When that window already has the focus, the
/// click goes on to the next one, so that repeated clicks visit them all.
pub fn left_click(entry: &Entry) -> Click {
    match (entry.windows.first(), entry.windows.last()) {
        (Some(latest), Some(oldest)) if latest.active && !latest.minimized => {
            Click::Activate(oldest.id)
        }
        (Some(latest), _) => Click::Activate(latest.id),
        _ => Click::Launch,
    }
}

/// A line of the menu opened by a right click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    /// One of the three "open on" tiles.
    OpenOn(OutputRole),
    NewWindow,
    Pin,
    Unpin,
    /// Forget where the application was tidied: it reopens floating.
    Forget,
    /// Close the window used last.
    Close(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Menu {
    pub title: String,
    pub subtitle: String,
    /// The screen that stands out among the "open on" tiles: where the
    /// application is, else where it would reopen.
    pub current: Option<OutputRole>,
    pub actions: Vec<MenuAction>,
}

/// The menu of an application, according to its state.
pub fn menu(entry: &Entry) -> Menu {
    let count = entry.windows.len();
    let latest = entry.windows.first();
    let subtitle = match (count, latest) {
        (_, None) => "Fermée".to_owned(),
        (1, Some(window)) => format!("1 fenêtre · {}", window.screen.label().to_lowercase()),
        (count, Some(_)) => format!("{count} fenêtres"),
    };
    let mut actions: Vec<MenuAction> = [OutputRole::Left, OutputRole::Center, OutputRole::Right]
        .into_iter()
        .map(MenuAction::OpenOn)
        .collect();
    if entry.identity.command.is_some() {
        actions.push(MenuAction::NewWindow);
    }
    actions.push(if entry.pinned {
        MenuAction::Unpin
    } else {
        MenuAction::Pin
    });
    if entry.habit.is_some() {
        actions.push(MenuAction::Forget);
    }
    if let Some(window) = latest {
        actions.push(MenuAction::Close(window.id));
    }
    Menu {
        title: entry.identity.name.clone(),
        subtitle,
        current: latest
            .map(|window| window.screen)
            .or(entry.habit.as_ref().map(|habit| habit.screen)),
        actions,
    }
}

/// Moves a favorite to another position, the others closing ranks.
pub fn reorder(favorites: &mut Vec<String>, from: usize, to: usize) {
    if from < favorites.len() && to < favorites.len() && from != to {
        let moved = favorites.remove(from);
        favorites.insert(to, moved);
    }
}

/// Adds an application to the favorites, at the end, unless it is there.
pub fn pin(favorites: &mut Vec<String>, entry: &Entry) {
    if !is_favorite(favorites, entry) {
        favorites.push(entry.identity.key.clone());
    }
}

/// Removes an application from the favorites, under whatever name the
/// configuration had it.
pub fn unpin(favorites: &mut Vec<String>, entry: &Entry, identify: &dyn Fn(&str) -> Identity) {
    favorites.retain(|favorite| identify(favorite).key != entry.identity.key);
}

fn is_favorite(favorites: &[String], entry: &Entry) -> bool {
    let names = &entry.identity.names;
    let named = |favorite: &String| names.iter().any(|name| name.eq_ignore_ascii_case(favorite));
    favorites
        .iter()
        .any(|favorite| *favorite == entry.identity.key || named(favorite))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HERE: OutputRole = OutputRole::Left;

    fn identity(name: &str) -> Identity {
        // "Code" and "code" are the same application, as with a catalog.
        let key = name.to_lowercase();
        Identity {
            name: key.clone(),
            icon: None,
            cover: None,
            command: Some(key.clone()),
            names: vec![key.clone()],
            game: key.starts_with("steam_app_"),
            key,
        }
    }

    fn window(id: u64, app: &str, screen: OutputRole) -> Window {
        Window {
            id,
            app: app.into(),
            title: String::new(),
            screen,
            slot: None,
            active: false,
            minimized: false,
        }
    }

    fn screen(role: OutputRole, anchored: bool) -> Screen {
        Screen {
            name: format!("{role:?}"),
            role,
            width: 1920,
            height: 1080,
            layout: coquille_model::LayoutKind::Columns,
            anchored,
        }
    }

    fn bar(favorites: &[&str], windows: &[Window]) -> Vec<Entry> {
        bar_with(favorites, windows, &[], &[])
    }

    fn bar_with(
        favorites: &[&str],
        windows: &[Window],
        screens: &[Screen],
        habits: &[Habit],
    ) -> Vec<Entry> {
        let favorites: Vec<String> = favorites.iter().map(|name| (*name).to_owned()).collect();
        let world = World {
            favorites: &favorites,
            windows,
            screens,
            habits,
            here: HERE,
        };
        entries(&world, &identity)
    }

    fn keys(entries: &[Entry]) -> Vec<&str> {
        entries
            .iter()
            .map(|entry| entry.identity.key.as_str())
            .collect()
    }

    #[test]
    fn favorites_come_first_in_the_order_of_the_configuration() {
        let entries = bar(&["firefox", "discord", "kitty"], &[]);

        assert_eq!(keys(&entries), ["firefox", "discord", "kitty"]);
        assert!(entries.iter().all(|entry| entry.pinned));
    }

    #[test]
    fn a_favorite_written_twice_shows_once() {
        let entries = bar(&["firefox", "Firefox"], &[]);

        assert_eq!(keys(&entries), ["firefox"]);
    }

    #[test]
    fn the_other_applications_open_on_any_screen_follow_by_name() {
        let windows = [
            window(1, "obsidian", HERE),
            window(2, "gimp", OutputRole::Center),
            window(3, "mpv", OutputRole::Right),
        ];

        let entries = bar(&["firefox"], &windows);

        assert_eq!(keys(&entries), ["firefox", "gimp", "mpv", "obsidian"]);
        assert!(entries[1..].iter().all(|entry| !entry.pinned));
    }

    #[test]
    fn an_application_open_on_another_screen_says_which_one() {
        let entries = bar(&[], &[window(3, "mpv", OutputRole::Right)]);

        assert_eq!(entries[0].indicator, Indicator::Elsewhere);
        assert_eq!(entries[0].subtitle, "Écran droit · non épinglée");
    }

    #[test]
    fn several_windows_on_another_screen_are_counted() {
        let windows = [
            window(1, "firefox", OutputRole::Right),
            window(2, "firefox", OutputRole::Right),
        ];

        let entries = bar(&["firefox"], &windows);

        assert_eq!(entries[0].subtitle, "Écran droit · 2 fenêtres");
    }

    #[test]
    fn a_window_hidden_on_another_screen_is_said_to_be() {
        let mut hidden = window(1, "kitty", OutputRole::Center);
        hidden.minimized = true;

        let entries = bar(&["kitty"], &[hidden]);

        assert_eq!(entries[0].subtitle, "Écran central · réduite");
    }

    #[test]
    fn a_closed_favorite_has_no_mark() {
        let entries = bar(&["kitty"], &[]);

        assert_eq!(entries[0].indicator, Indicator::None);
        assert_eq!(entries[0].subtitle, "Fermé");
    }

    #[test]
    fn an_application_open_here_has_the_accent_bar() {
        let entries = bar(&["discord"], &[window(1, "discord", HERE)]);

        assert_eq!(entries[0].indicator, Indicator::Here);
        assert_eq!(entries[0].subtitle, "Ici");
    }

    #[test]
    fn an_application_open_on_another_screen_has_the_grey_dot() {
        let entries = bar(&["firefox"], &[window(1, "firefox", OutputRole::Right)]);

        assert_eq!(entries[0].indicator, Indicator::Elsewhere);
        assert_eq!(entries[0].subtitle, "Écran droit");
    }

    #[test]
    fn a_window_here_wins_over_windows_elsewhere() {
        let windows = [
            window(1, "firefox", OutputRole::Right),
            window(2, "firefox", HERE),
        ];

        let entries = bar(&["firefox"], &windows);

        assert_eq!(entries[0].indicator, Indicator::Here);
        assert_eq!(entries[0].subtitle, "Ici · 2 fenêtres");
    }

    #[test]
    fn a_game_being_played_says_so() {
        let windows = [window(1, "steam_app_7", OutputRole::Center)];
        let screens = [screen(OutputRole::Center, true)];

        let entries = bar_with(&["steam_app_7"], &windows, &screens, &[]);

        assert_eq!(entries[0].subtitle, "Écran central · en jeu");
    }

    #[test]
    fn an_application_that_is_not_a_favorite_says_so() {
        let entries = bar(&[], &[window(1, "obsidian", HERE)]);

        assert_eq!(entries[0].subtitle, "Ici · non épinglée");
    }

    #[test]
    fn a_hidden_window_is_said_to_be() {
        let mut hidden = window(1, "kitty", HERE);
        hidden.minimized = true;

        let entries = bar(&["kitty"], &[hidden]);

        assert_eq!(entries[0].subtitle, "Ici · réduite");
        assert_eq!(entries[0].indicator, Indicator::Here);
    }

    #[test]
    fn windows_and_favorites_meet_whatever_the_case_of_their_names() {
        let entries = bar(&["code"], &[window(1, "Code", HERE)]);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].windows.len(), 1);
    }

    #[test]
    fn a_click_on_a_closed_application_starts_it() {
        let entries = bar(&["kitty"], &[]);

        assert_eq!(left_click(&entries[0]), Click::Launch);
    }

    #[test]
    fn a_click_shows_the_window_used_last() {
        let windows = [window(4, "kitty", HERE), window(2, "kitty", HERE)];

        let entries = bar(&["kitty"], &windows);

        assert_eq!(left_click(&entries[0]), Click::Activate(4));
    }

    #[test]
    fn a_click_on_the_application_in_use_goes_to_its_next_window() {
        let mut focused = window(4, "kitty", HERE);
        focused.active = true;
        let windows = [focused, window(2, "kitty", HERE), window(9, "kitty", HERE)];

        let entries = bar(&["kitty"], &windows);

        assert_eq!(left_click(&entries[0]), Click::Activate(9));
    }

    #[test]
    fn a_click_restores_a_hidden_window() {
        let mut hidden = window(4, "kitty", HERE);
        hidden.minimized = true;

        let entries = bar(&["kitty"], &[hidden]);

        assert_eq!(left_click(&entries[0]), Click::Activate(4));
    }

    #[test]
    fn the_menu_of_a_closed_favorite_offers_to_open_and_unpin() {
        let entries = bar(&["kitty"], &[]);

        let menu = menu(&entries[0]);

        assert_eq!(menu.subtitle, "Fermée");
        assert_eq!(
            menu.actions,
            [
                MenuAction::OpenOn(OutputRole::Left),
                MenuAction::OpenOn(OutputRole::Center),
                MenuAction::OpenOn(OutputRole::Right),
                MenuAction::NewWindow,
                MenuAction::Unpin,
            ]
        );
        assert_eq!(menu.current, None);
    }

    #[test]
    fn the_menu_of_an_open_application_can_close_its_last_window() {
        let windows = [
            window(4, "kitty", HERE),
            window(2, "kitty", OutputRole::Right),
        ];

        let entries = bar(&["kitty"], &windows);
        let menu = menu(&entries[0]);

        assert_eq!(menu.subtitle, "2 fenêtres");
        assert_eq!(menu.actions.last(), Some(&MenuAction::Close(4)));
        assert_eq!(menu.current, Some(HERE));
    }

    #[test]
    fn the_menu_says_where_the_only_window_is() {
        let entries = bar(&["kitty"], &[window(4, "kitty", HERE)]);

        assert_eq!(menu(&entries[0]).subtitle, "1 fenêtre · écran gauche");
    }

    #[test]
    fn the_menu_of_an_application_that_is_not_a_favorite_offers_to_pin_it() {
        let entries = bar(&[], &[window(1, "obsidian", HERE)]);

        let menu = menu(&entries[0]);

        assert!(menu.actions.contains(&MenuAction::Pin));
        assert!(!menu.actions.contains(&MenuAction::Unpin));
    }

    #[test]
    fn forgetting_a_place_is_offered_only_when_there_is_one() {
        let habit = Habit {
            app: "Discord".into(),
            screen: OutputRole::Right,
            slot: 1,
        };

        let without = bar(&["discord"], &[]);
        let with = bar_with(&["discord"], &[], &[], std::slice::from_ref(&habit));

        assert!(!menu(&without[0]).actions.contains(&MenuAction::Forget));
        assert!(menu(&with[0]).actions.contains(&MenuAction::Forget));
        assert_eq!(with[0].habit, Some(habit));
        // Closed, the tile of the screen it would reopen on stands out.
        assert_eq!(menu(&with[0]).current, Some(OutputRole::Right));
    }

    #[test]
    fn a_favorite_can_be_moved_up_or_down() {
        let mut favorites: Vec<String> = ["a", "b", "c", "d"].map(String::from).to_vec();

        reorder(&mut favorites, 3, 1);
        assert_eq!(favorites, ["a", "d", "b", "c"]);

        reorder(&mut favorites, 0, 2);
        assert_eq!(favorites, ["d", "b", "a", "c"]);
    }

    #[test]
    fn moving_a_favorite_nowhere_changes_nothing() {
        let mut favorites: Vec<String> = ["a", "b"].map(String::from).to_vec();

        reorder(&mut favorites, 1, 1);
        reorder(&mut favorites, 5, 0);
        reorder(&mut favorites, 0, 5);

        assert_eq!(favorites, ["a", "b"]);
    }

    #[test]
    fn pinning_adds_at_the_end_once() {
        let entries = bar(&[], &[window(1, "obsidian", HERE)]);
        let mut favorites = vec!["firefox".to_owned()];

        pin(&mut favorites, &entries[0]);
        pin(&mut favorites, &entries[0]);

        assert_eq!(favorites, ["firefox", "obsidian"]);
    }

    #[test]
    fn unpinning_removes_the_name_the_configuration_had() {
        let entries = bar(&["Firefox", "kitty"], &[]);
        let mut favorites = vec!["Firefox".to_owned(), "kitty".to_owned()];

        unpin(&mut favorites, &entries[0], &identity);

        assert_eq!(favorites, ["kitty"]);
    }
}
