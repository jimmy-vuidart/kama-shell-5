//! Light or dark for the applications, kept in step with the chrome: GTK,
//! libadwaita and the portals through the GNOME settings and GTK's own files
//! (`coquille_apps::tell_applications`), Qt through its gtk3 platform theme,
//! which reads the same settings and follows them while running.
//!
//! The settings are the user's and shared with any other session: they are
//! written when coquille is the session (real screens), and in nested runs
//! only when asked (`--app-theme`).

use std::sync::mpsc::{Sender, channel};

use tracing::warn;

use crate::theme::ThemeKind;

/// The variable that picks the platform theme of Qt applications.
pub const QT_PLATFORM_THEME_VAR: &str = "QT_QPA_PLATFORMTHEME";
/// Qt's gtk3 platform theme takes its palette and its light or dark from
/// the GTK theme, and follows its changes; the portal's one changes the color
/// scheme but keeps the palette it started with.
const QT_PLATFORM_THEME: &str = "gtk3";

/// The platform theme to give Qt applications: none when the user chose
/// one in the environment of the session.
pub fn qt_platform_theme() -> Option<&'static str> {
    std::env::var_os(QT_PLATFORM_THEME_VAR)
        .is_none()
        .then_some(QT_PLATFORM_THEME)
}

/// Tells the applications each theme it is given, one after the other, on a
/// thread of its own: `gsettings` may wait on D-Bus, and the compositor must
/// not.
pub struct AppTheme {
    sender: Sender<ThemeKind>,
}

impl AppTheme {
    pub fn start() -> Option<Self> {
        let (sender, receiver) = channel::<ThemeKind>();
        let spawned = std::thread::Builder::new()
            .name("app-theme".to_owned())
            .spawn(move || {
                while let Ok(mut kind) = receiver.recv() {
                    // Only the last of quick changes matters.
                    while let Ok(newer) = receiver.try_recv() {
                        kind = newer;
                    }
                    coquille_apps::tell_applications(kind.into());
                }
            });
        match spawned {
            Ok(_) => Some(Self { sender }),
            Err(error) => {
                warn!(%error, "cannot start telling the applications the theme");
                None
            }
        }
    }

    pub fn tell(&self, kind: ThemeKind) {
        let _ = self.sender.send(kind);
    }
}
