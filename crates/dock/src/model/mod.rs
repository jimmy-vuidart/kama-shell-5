//! What the dock shows, decided without drawing anything: every rule of the
//! bar, of the drawer and of the mixer lives here, tested.

pub mod bar;
pub mod drawer;
pub mod mixer;
pub mod tray;

use crate::steam::Game;

/// The edge of its screen an upright dock stands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
}

/// Which way a dock lies on its screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// A column on the left or the right edge; its launcher is a drawer
    /// beside it.
    Vertical(Side),
    /// A row on the bottom edge; its launcher is a panel above it.
    Horizontal,
}
use coquille_apps::Catalog;

/// An application as the dock knows it, whatever led to it: a name of the
/// configuration, or what a window calls itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The same for every name of the application: what entries are told
    /// apart by.
    pub key: String,
    pub name: String,
    pub icon: Option<String>,
    /// The picture of a game, shown in place of an icon.
    pub cover: Option<std::path::PathBuf>,
    /// How to start it, when the dock knows.
    pub command: Option<String>,
    /// Every name its windows may bear (`app_id`, `WM_CLASS`).
    pub names: Vec<String>,
    pub game: bool,
}

/// Finds the application a name stands for.
pub fn identify(catalog: &Catalog, games: &[Game], name: &str) -> Identity {
    if let Some(game) = games.iter().find(|game| game.window_class() == name) {
        return Identity {
            key: game.window_class(),
            name: game.name.clone(),
            icon: None,
            cover: game.cover.clone(),
            command: Some(game.command()),
            names: vec![game.window_class()],
            game: true,
        };
    }
    let Some(app) = catalog.find(name) else {
        return Identity {
            key: name.to_lowercase(),
            name: display_name(name),
            icon: Some(name.to_lowercase()),
            cover: None,
            command: None,
            names: vec![name.to_owned()],
            game: name.starts_with("steam_app_"),
        };
    };
    // The shortcut Steam wrote for a game: its windows will bear the number
    // of the game, and it opens at the center like any game.
    if let Some(number) = crate::steam::game_of_command(&app.command) {
        let class = format!("steam_app_{number}");
        return Identity {
            key: class.clone(),
            name: app.name.clone(),
            icon: app.icon.clone(),
            cover: None,
            command: Some(app.command.clone()),
            names: vec![class],
            game: true,
        };
    }
    let program = app.command.split_whitespace().next().unwrap_or_default();
    let program = program.rsplit('/').next().unwrap_or_default();
    let last_part = app.id.rsplit('.').next().unwrap_or_default();
    let mut names = vec![app.id.clone()];
    let others = [
        app.wm_class.as_deref(),
        Some(last_part),
        Some(program),
        Some(name),
    ];
    for other in others.into_iter().flatten() {
        let known = names.iter().any(|known| known.eq_ignore_ascii_case(other));
        if !other.is_empty() && !known {
            names.push(other.to_owned());
        }
    }
    Identity {
        key: app.id.clone(),
        name: app.name.clone(),
        icon: app.icon.clone(),
        cover: None,
        command: Some(app.command.clone()),
        names,
        game: false,
    }
}

/// A name to show for an application nothing is known about: the last part
/// of its identifier, capitalised (`org.example.Editor` gives `Editor`).
fn display_name(name: &str) -> String {
    let last = name.rsplit('.').next().unwrap_or(name);
    let mut letters = last.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => "Application".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coquille_apps::App;

    fn catalog() -> Catalog {
        Catalog::new(vec![App {
            id: "code-oss".into(),
            name: "Éditeur de code".into(),
            command: "/usr/bin/code-oss --new-window".into(),
            icon: Some("com.visualstudio.code.oss".into()),
            wm_class: Some("Code".into()),
        }])
    }

    #[test]
    fn a_window_and_a_configured_name_lead_to_the_same_application() {
        let from_config = identify(&catalog(), &[], "code");
        let from_window = identify(&catalog(), &[], "Code");

        assert_eq!(from_config.key, "code-oss");
        assert_eq!(from_window.key, "code-oss");
        assert_eq!(from_window.name, "Éditeur de code");
        assert_eq!(
            from_window.command.as_deref(),
            Some("/usr/bin/code-oss --new-window")
        );
    }

    #[test]
    fn an_application_is_asked_for_under_every_name_it_may_bear() {
        let identity = identify(&catalog(), &[], "code");

        assert_eq!(identity.names, ["code-oss", "Code"]);
    }

    #[test]
    fn an_unknown_application_keeps_a_presentable_name() {
        let identity = identify(&catalog(), &[], "org.example.editor");

        assert_eq!(identity.key, "org.example.editor");
        assert_eq!(identity.name, "Editor");
        assert_eq!(identity.command, None);
        assert_eq!(identity.names, ["org.example.editor"]);
    }

    #[test]
    fn the_shortcut_steam_wrote_for_a_game_is_that_game() {
        let catalog = Catalog::new(vec![App {
            id: "Factorio".into(),
            name: "Factorio".into(),
            command: "steam steam://rungameid/427520".into(),
            icon: Some("steam_icon_427520".into()),
            wm_class: None,
        }]);

        let identity = identify(&catalog, &[], "Factorio");

        assert!(identity.game);
        assert_eq!(identity.key, "steam_app_427520");
        assert_eq!(identity.names, ["steam_app_427520"]);
    }

    #[test]
    fn the_window_of_a_game_bears_the_name_of_the_game() {
        let games = [Game {
            id: 42,
            name: "Cendres d'Aube".into(),
            last_played: 0,
            cover: None,
        }];

        let identity = identify(&catalog(), &games, "steam_app_42");

        assert_eq!(identity.name, "Cendres d'Aube");
        assert!(identity.game);
        assert_eq!(
            identity.command.as_deref(),
            Some("steam steam://rungameid/42")
        );
    }
}
