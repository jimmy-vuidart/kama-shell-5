//! What the dock talks to: the compositor, the audio server, the network,
//! Bluetooth, the screens, the session. Each has a real implementation and
//! a simulated one, so that the dock can be tried without touching anything.

pub mod audio;
pub mod backlight;
pub mod battery;
pub mod compositor;
pub mod installed;
pub mod mock;
pub mod notify;
pub mod services;
pub mod tasks;
pub mod tray;
pub mod xembed;

use tokio::sync::mpsc::UnboundedSender;

use crate::model::mixer::Stream;
use crate::model::notify::Notice;
use crate::model::tray::TrayItem;

/// A backend that can be the real one or a simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, clap::ValueEnum)]
pub enum Backend {
    /// Screens and windows, from the coquille compositor.
    Compositor,
    /// Volumes, through PipeWire.
    Audio,
    /// NetworkManager.
    Network,
    /// BlueZ.
    Bluetooth,
    /// The brightness of the screens: a laptop's backlight, else DDC/CI.
    Brightness,
    /// Logging out, restarting, powering off: always after a confirmation.
    Session,
    /// The system tray (StatusNotifierItem and DBusMenu).
    Tray,
    /// Freedesktop notifications on the session bus.
    Notifications,
    /// A laptop's battery, through UPower.
    Battery,
}

impl Backend {
    /// The backends that are real unless the dock is told otherwise: all
    /// of them.
    pub const DEFAULT: [Self; 9] = [
        Self::Compositor,
        Self::Audio,
        Self::Network,
        Self::Bluetooth,
        Self::Brightness,
        Self::Session,
        Self::Tray,
        Self::Notifications,
        Self::Battery,
    ];
}

/// A switch of the control center.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Switch {
    pub on: bool,
    /// Shown under its name: "Ethernet", "Casque", "Désactivé".
    pub detail: String,
    /// The service behind it answers; otherwise the switch is greyed.
    pub available: bool,
}

/// An audio output device reported by the audio server.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AudioSink {
    pub name: String,
    pub description: String,
}

/// The output the sound goes to, and what plays on it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Audio {
    /// The name of the output device, as the user knows it.
    pub output: String,
    /// The technical name of the output device.
    pub output_name: String,
    pub volume: f32,
    pub muted: bool,
    pub streams: Vec<Stream>,
    pub sinks: Vec<AudioSink>,
}

/// What a backend tells the dock.
#[derive(Debug, Clone)]
pub enum Update {
    /// The backend is ready to take requests.
    Link(Backend, UnboundedSender<Request>),
    Audio(Audio),
    /// How loud each stream is right now, from 0 to 1.
    Levels(Vec<(u32, f32)>),
    Network(Switch),
    Bluetooth(Switch),
    /// The brightness of the screens, from 0 to 1; `None` when no screen
    /// can be reached.
    Brightness(Option<f32>),
    Tray(Vec<TrayItem>),
    /// A laptop's battery; `None` when it has none, or it is gone.
    Battery(Option<crate::model::battery::Battery>),
    Notify(Notice),
    /// What is installed, summed up: when it changes, the applications and
    /// the games are read again.
    Installed(u64),
    NotifyClosed {
        id: u32,
        reason: u32,
    },
}

/// What the dock asks of a backend.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Volume(f32),
    Mute(bool),
    SelectOutput(String),
    StreamVolume(Vec<u32>, f32),
    StreamMute(Vec<u32>, bool),
    /// Whether to tell how loud each stream is: only while the mixer shows.
    Levels(bool),
    Network(bool),
    Bluetooth(bool),
    Brightness(f32),
    TrayActivate {
        address: String,
        x: i32,
        y: i32,
    },
    TraySecondary {
        address: String,
        x: i32,
        y: i32,
    },
    /// The item draws its own menu (an X11 tray icon, or an item without
    /// DBusMenu).
    TrayContextMenu {
        address: String,
        x: i32,
        y: i32,
    },
    TrayMenuItem {
        address: String,
        menu_path: String,
        id: i32,
    },
    NotifyClose {
        id: u32,
        reason: u32,
    },
    NotifyAction {
        id: u32,
        key: String,
    },
}

impl Request {
    /// The backend this request is for.
    pub fn backend(&self) -> Backend {
        match self {
            Self::Volume(_)
            | Self::Mute(_)
            | Self::SelectOutput(_)
            | Self::StreamVolume(..)
            | Self::StreamMute(..)
            | Self::Levels(_) => Backend::Audio,
            Self::Network(_) => Backend::Network,
            Self::Bluetooth(_) => Backend::Bluetooth,
            Self::Brightness(_) => Backend::Brightness,
            Self::TrayActivate { .. }
            | Self::TraySecondary { .. }
            | Self::TrayContextMenu { .. }
            | Self::TrayMenuItem { .. } => Backend::Tray,
            Self::NotifyClose { .. } | Self::NotifyAction { .. } => Backend::Notifications,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logging_out_is_asked_of_the_compositor_not_of_a_program() {
        assert_eq!(SessionAction::Logout.command(), None);
    }

    #[test]
    fn restarting_and_powering_off_go_through_the_system_without_a_prompt() {
        let restart = SessionAction::Restart.command().unwrap();
        let power_off = SessionAction::PowerOff.command().unwrap();

        assert_eq!(restart, ["systemctl", "reboot", "--no-ask-password"]);
        assert_eq!(power_off, ["systemctl", "poweroff", "--no-ask-password"]);
    }

    #[test]
    fn notification_requests_go_to_the_notifications_backend() {
        assert_eq!(
            Request::NotifyClose { id: 1, reason: 3 }.backend(),
            Backend::Notifications
        );
        assert_eq!(
            Request::NotifyAction {
                id: 1,
                key: "default".into(),
            }
            .backend(),
            Backend::Notifications
        );
    }

    #[test]
    fn every_service_is_real_unless_the_dock_is_told_to_pretend() {
        assert!(Backend::DEFAULT.contains(&Backend::Session));
        assert!(Backend::DEFAULT.contains(&Backend::Tray));
        assert!(Backend::DEFAULT.contains(&Backend::Notifications));
        assert!(Backend::DEFAULT.contains(&Backend::Battery));
        assert_eq!(Backend::DEFAULT.len(), 9);
    }

    #[test]
    fn a_refusal_of_the_system_is_told_in_one_line() {
        let refusal = "Failed to reboot system via logind: Interactive authentication required.\n";

        assert_eq!(
            SessionAction::Restart.failure(refusal),
            "Redémarrer : Failed to reboot system via logind: Interactive authentication required."
        );
        assert_eq!(
            SessionAction::PowerOff.failure("  \n"),
            "Éteindre : refusé par le système."
        );
    }
}

/// What ends or restarts the session, always after a confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Logout,
    /// A laptop sleeps (closing its lid does the same, through logind).
    Suspend,
    Restart,
    PowerOff,
}

impl SessionAction {
    /// The question of the confirmation.
    pub fn question(self) -> &'static str {
        match self {
            Self::Logout => "Se déconnecter ?",
            Self::Suspend => "Mettre en veille ?",
            Self::Restart => "Redémarrer l'ordinateur ?",
            Self::PowerOff => "Éteindre l'ordinateur ?",
        }
    }

    /// The button that confirms.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Logout => "Se déconnecter",
            Self::Suspend => "Mettre en veille",
            Self::Restart => "Redémarrer",
            Self::PowerOff => "Éteindre",
        }
    }

    /// The program that does it. Logging out has none: the compositor is
    /// asked to quit, which ends the session. The others go through
    /// systemd, which must refuse rather than ask for a password nobody
    /// could type.
    pub fn command(self) -> Option<[&'static str; 3]> {
        match self {
            Self::Logout => None,
            Self::Suspend => Some(["systemctl", "suspend", "--no-ask-password"]),
            Self::Restart => Some(["systemctl", "reboot", "--no-ask-password"]),
            Self::PowerOff => Some(["systemctl", "poweroff", "--no-ask-password"]),
        }
    }

    /// What to tell the user when the system refused: its own words when it
    /// gave some.
    pub fn failure(self, reason: &str) -> String {
        let reason = reason.lines().find(|line| !line.trim().is_empty());
        let reason = reason.map_or("refusé par le système.", str::trim);
        format!("{} : {reason}", self.verb())
    }
}
