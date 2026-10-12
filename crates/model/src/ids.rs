//! Identifiers handed out by the compositor.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A toplevel window, Wayland or X11.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WindowId(pub u64);

/// A physical (or simulated) output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OutputId(pub u32);

/// What identifies an application across launches: the Wayland `app_id`, or
/// the X11 `WM_CLASS`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppKey(String);

impl AppKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Both names stand for the same application: only the case may differ
    /// (`code`, `Code`).
    pub fn same_app(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
    }

    /// The application is a game: a title started by Steam, whose windows
    /// are all named after its number. Games open at the center.
    pub fn is_game(&self) -> bool {
        self.0
            .strip_prefix("steam_app_")
            .is_some_and(|title| !title.is_empty())
    }
}

impl fmt::Display for AppKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod game_tests {
    use super::*;

    #[test]
    fn a_window_of_a_steam_title_is_a_game() {
        assert!(AppKey::new("steam_app_1245620").is_game());
        assert!(AppKey::new("steam_app_default").is_game());
    }

    #[test]
    fn steam_itself_and_other_applications_are_not_games() {
        assert!(!AppKey::new("steam").is_game());
        assert!(!AppKey::new("firefox").is_game());
        assert!(!AppKey::new("steam_app_").is_game());
    }
}
