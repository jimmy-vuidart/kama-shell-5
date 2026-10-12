//! What the dock talks to: the compositor, the audio server, the network,
//! Bluetooth, the screens, the session. Each has a real implementation and
//! a simulated one, so that the dock can be tried without touching anything.

pub mod audio;
pub mod compositor;
pub mod mock;
pub mod services;

use tokio::sync::mpsc::UnboundedSender;

use crate::model::mixer::Stream;

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
    /// The brightness of the screens, through DDC/CI.
    Brightness,
    /// Logging out, restarting, powering off. Simulated unless asked for:
    /// nothing may end the session by accident.
    Session,
}

impl Backend {
    /// The backends that are real unless the dock is told otherwise.
    pub const DEFAULT: [Self; 5] = [
        Self::Compositor,
        Self::Audio,
        Self::Network,
        Self::Bluetooth,
        Self::Brightness,
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

/// The output the sound goes to, and what plays on it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Audio {
    /// The name of the output device, as the user knows it.
    pub output: String,
    pub volume: f32,
    pub muted: bool,
    pub streams: Vec<Stream>,
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
}

/// What the dock asks of a backend.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Volume(f32),
    Mute(bool),
    StreamVolume(Vec<u32>, f32),
    StreamMute(Vec<u32>, bool),
    /// Whether to tell how loud each stream is: only while the mixer shows.
    Levels(bool),
    Network(bool),
    Bluetooth(bool),
    Brightness(f32),
}

impl Request {
    /// The backend this request is for.
    pub fn backend(&self) -> Backend {
        match self {
            Self::Volume(_)
            | Self::Mute(_)
            | Self::StreamVolume(..)
            | Self::StreamMute(..)
            | Self::Levels(_) => Backend::Audio,
            Self::Network(_) => Backend::Network,
            Self::Bluetooth(_) => Backend::Bluetooth,
            Self::Brightness(_) => Backend::Brightness,
        }
    }
}

/// What ends or restarts the session, always after a confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Logout,
    Restart,
    PowerOff,
}

impl SessionAction {
    /// The question of the confirmation.
    pub fn question(self) -> &'static str {
        match self {
            Self::Logout => "Se déconnecter ?",
            Self::Restart => "Redémarrer l'ordinateur ?",
            Self::PowerOff => "Éteindre l'ordinateur ?",
        }
    }

    /// The button that confirms.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Logout => "Se déconnecter",
            Self::Restart => "Redémarrer",
            Self::PowerOff => "Éteindre",
        }
    }
}
