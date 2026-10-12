//! What the compositor and the programs of the shell (the dock) say to each
//! other, over a Unix socket: one JSON message per line, in both directions.
//!
//! The compositor pushes [`Event`]s: the whole state each time it changes.
//! Programs send [`Command`]s. Nothing here knows about Wayland.

use std::path::{Path, PathBuf};

use coquille_model::{LayoutKind, OutputRole};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The variable that tells the programs started by the compositor where its
/// socket is.
pub const SOCKET_ENV: &str = "COQUILLE_SOCKET";

/// A line longer than this is not a message: it is dropped.
const MAX_LINE: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    #[default]
    Light,
    Dark,
}

/// Everything the shell shows of the compositor's state.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub theme: ThemeName,
    /// The accent color, as `#rrggbb`.
    pub accent: String,
    pub screens: Vec<Screen>,
    /// The window focused last comes first.
    pub windows: Vec<Window>,
    /// Where the user tidied each application.
    pub habits: Vec<Habit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screen {
    /// The name clients know the output by (its connector).
    pub name: String,
    pub role: OutputRole,
    pub width: i32,
    pub height: i32,
    pub layout: LayoutKind,
    /// Held by a fullscreen window (game mode): it shows no panel.
    pub anchored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub id: u64,
    /// `app_id`, or `WM_CLASS` for an X11 window.
    pub app: String,
    pub title: String,
    pub screen: OutputRole,
    /// Its slot on that screen; `None` when it floats.
    pub slot: Option<usize>,
    /// It has the focus.
    pub active: bool,
    pub minimized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Habit {
    pub app: String,
    pub screen: OutputRole,
    pub slot: usize,
}

/// From the compositor to the programs of the shell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// The state, whole, sent on connection and after each change.
    State(Snapshot),
    /// The shortcut of the drawer was pressed.
    ToggleLauncher,
}

/// From a program of the shell to the compositor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Shows a window and gives it the focus; restores it if it was hidden.
    Activate {
        window: u64,
    },
    Close {
        window: u64,
    },
    /// The next window of an application opens on this screen, once.
    OpenOn {
        app: String,
        screen: OutputRole,
    },
    /// Forgets where an application was tidied.
    Forget {
        app: String,
    },
    /// Opens or closes the drawer, as its shortcut does.
    ToggleLauncher,
    SetTheme {
        theme: ThemeName,
    },
    /// Ends the session.
    Quit,
}

#[derive(Debug, Error)]
#[error("not a message: {0}")]
pub struct DecodeError(#[from] serde_json::Error);

/// A message as it goes on the socket: one line of JSON.
pub fn encode<T: Serialize>(message: &T) -> String {
    // The messages of this crate hold nothing JSON cannot express.
    let mut line = serde_json::to_string(message).unwrap_or_default();
    line.push('\n');
    line
}

pub fn decode<T: DeserializeOwned>(line: &str) -> Result<T, DecodeError> {
    Ok(serde_json::from_str(line.trim())?)
}

/// Where a compositor listens, given its Wayland display.
pub fn socket_path(runtime_dir: &Path, wayland_display: &str) -> PathBuf {
    runtime_dir.join(format!("coquille-{wayland_display}.sock"))
}

/// The socket of the compositor this program runs in, if it is coquille.
pub fn socket_from_env() -> Option<PathBuf> {
    std::env::var_os(SOCKET_ENV).map(PathBuf::from)
}

/// Turns what arrives on a socket, in pieces of any size, into lines.
#[derive(Debug, Default)]
pub struct LineBuffer {
    pending: Vec<u8>,
}

impl LineBuffer {
    /// Takes what was just read; returns the lines it completes, without
    /// their end. A line that is not text, or far too long, is skipped.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(end) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            if let Ok(text) = String::from_utf8(line) {
                let text = text.trim();
                if !text.is_empty() {
                    lines.push(text.to_owned());
                }
            }
        }
        if self.pending.len() > MAX_LINE {
            self.pending.clear();
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        Snapshot {
            theme: ThemeName::Dark,
            accent: "#7aa7ff".into(),
            screens: vec![Screen {
                name: "HDMI-A-1".into(),
                role: OutputRole::Left,
                width: 1920,
                height: 1080,
                layout: LayoutKind::Columns,
                anchored: false,
            }],
            windows: vec![Window {
                id: 4,
                app: "discord".into(),
                title: "# raid-mardi".into(),
                screen: OutputRole::Left,
                slot: Some(0),
                active: true,
                minimized: false,
            }],
            habits: vec![Habit {
                app: "discord".into(),
                screen: OutputRole::Left,
                slot: 0,
            }],
        }
    }

    #[test]
    fn every_command_survives_the_trip() {
        let commands = [
            Command::Activate { window: 12 },
            Command::Close { window: 12 },
            Command::OpenOn {
                app: "firefox".into(),
                screen: OutputRole::Center,
            },
            Command::Forget {
                app: "firefox".into(),
            },
            Command::ToggleLauncher,
            Command::SetTheme {
                theme: ThemeName::Dark,
            },
            Command::Quit,
        ];

        for command in commands {
            let line = encode(&command);
            assert_eq!(decode::<Command>(&line).unwrap(), command, "{line}");
        }
    }

    #[test]
    fn the_state_survives_the_trip() {
        let event = Event::State(snapshot());

        let line = encode(&event);

        assert_eq!(decode::<Event>(&line).unwrap(), event);
        assert_eq!(
            decode::<Event>(&encode(&Event::ToggleLauncher)).unwrap(),
            Event::ToggleLauncher
        );
    }

    #[test]
    fn a_message_is_one_line_of_readable_json() {
        let line = encode(&Command::OpenOn {
            app: "firefox".into(),
            screen: OutputRole::Center,
        });

        assert_eq!(
            line,
            "{\"command\":\"open_on\",\"app\":\"firefox\",\"screen\":\"center\"}\n"
        );
    }

    #[test]
    fn an_unknown_command_is_an_error_not_a_crash() {
        assert!(decode::<Command>(r#"{"command":"dance"}"#).is_err());
    }

    #[test]
    fn a_malformed_message_is_an_error() {
        assert!(decode::<Command>("{\"command\":").is_err());
        assert!(decode::<Command>("").is_err());
        assert!(decode::<Command>(r#"{"command":"close"}"#).is_err());
        assert!(decode::<Command>(r#"{"command":"close","window":"four"}"#).is_err());
    }

    #[test]
    fn fields_added_by_a_newer_version_are_ignored() {
        let command = decode::<Command>(r#"{"command":"close","window":3,"force":true}"#);

        assert_eq!(command.unwrap(), Command::Close { window: 3 });
    }

    #[test]
    fn the_socket_is_named_after_the_wayland_display() {
        let path = socket_path(Path::new("/run/user/1000"), "wayland-1");

        assert_eq!(path, Path::new("/run/user/1000/coquille-wayland-1.sock"));
    }

    #[test]
    fn a_line_arriving_in_pieces_is_put_back_together() {
        let mut buffer = LineBuffer::default();

        assert_eq!(buffer.push(b"{\"command\":"), Vec::<String>::new());
        assert_eq!(buffer.push(b"\"quit\"}\n"), ["{\"command\":\"quit\"}"]);
    }

    #[test]
    fn several_lines_arriving_together_are_told_apart() {
        let mut buffer = LineBuffer::default();

        let lines = buffer.push(b"one\ntwo\n\nthr");

        assert_eq!(lines, ["one", "two"]);
        assert_eq!(buffer.push(b"ee\n"), ["three"]);
    }

    #[test]
    fn a_line_that_never_ends_is_dropped() {
        let mut buffer = LineBuffer::default();

        buffer.push(&vec![b'x'; MAX_LINE + 1]);

        assert_eq!(buffer.push(b"\nquit\n"), ["quit"]);
    }

    #[test]
    fn bytes_that_are_not_text_are_skipped() {
        let mut buffer = LineBuffer::default();

        assert_eq!(buffer.push(b"\xff\xfe\nquit\n"), ["quit"]);
    }
}
