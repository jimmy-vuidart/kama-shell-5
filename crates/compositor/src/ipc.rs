//! The socket the programs of the shell (the dock) talk to: the state goes
//! out each time it changes, commands come in. See the `coquille-ipc` crate
//! for what is said.

use std::io::{ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use coquille_ipc::{
    Command, Event, Habit, LineBuffer, Screen, Snapshot, ThemeName, Window, decode, encode,
    socket_path,
};
use coquille_model::{AppKey, Placement, WindowId};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{Interest, Mode, PostAction};
use tracing::{debug, info, warn};

use crate::state::State;
use crate::theme::{Theme, ThemeKind};

pub struct Ipc {
    pub path: PathBuf,
    clients: Vec<Client>,
    next_client: u64,
    /// The state as it was sent last.
    sent: Option<Snapshot>,
    /// Something the shell shows may have changed since.
    stale: bool,
}

struct Client {
    id: u64,
    stream: UnixStream,
}

impl Ipc {
    /// Says something to every program listening. One that cannot take it
    /// is hung up on: it will come back and get the whole state again.
    fn broadcast(&mut self, event: &Event) {
        let line = encode(event);
        self.clients.retain_mut(|client| {
            let sent = client.stream.write_all(line.as_bytes()).is_ok();
            if !sent {
                debug!(
                    client = client.id,
                    "a program of the shell stopped listening"
                );
                let _ = client.stream.shutdown(Shutdown::Both);
            }
            sent
        });
    }
}

impl Drop for Ipc {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl From<ThemeKind> for ThemeName {
    fn from(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Light => Self::Light,
            ThemeKind::Dark => Self::Dark,
        }
    }
}

impl From<ThemeName> for ThemeKind {
    fn from(name: ThemeName) -> Self {
        match name {
            ThemeName::Light => Self::Light,
            ThemeName::Dark => Self::Dark,
        }
    }
}

impl State {
    /// Opens the socket of the shell, next to the Wayland one. Without it
    /// the compositor works all the same; the dock just knows nothing.
    pub fn start_ipc(&mut self) {
        let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") else {
            warn!("no XDG_RUNTIME_DIR: the shell cannot talk to the compositor");
            return;
        };
        let display = self.socket_name.to_string_lossy().into_owned();
        let path = socket_path(Path::new(&runtime), &display);
        // Left behind by a compositor that did not stop cleanly.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).and_then(|listener| {
            listener.set_nonblocking(true)?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            Ok(listener)
        });
        let listener = match listener {
            Ok(listener) => listener,
            Err(error) => {
                warn!(%error, path = %path.display(), "cannot open the socket of the shell");
                return;
            }
        };

        let source = Generic::new(listener, Interest::READ, Mode::Level);
        let watched = self
            .loop_handle
            .insert_source(source, |_, listener, state| {
                loop {
                    match listener.accept() {
                        Ok((stream, _)) => state.ipc_accept(stream),
                        Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                        Err(error) => {
                            warn!(%error, "cannot accept a program of the shell");
                            break;
                        }
                    }
                }
                Ok(PostAction::Continue)
            });
        if let Err(error) = watched {
            warn!(%error, "cannot watch the socket of the shell");
            return;
        }
        info!(path = %path.display(), "listening to the shell");
        self.ipc = Some(Ipc {
            path,
            clients: Vec::new(),
            next_client: 0,
            sent: None,
            stale: true,
        });
    }

    fn ipc_accept(&mut self, stream: UnixStream) {
        let Some(ipc) = self.ipc.as_mut() else {
            return;
        };
        let id = ipc.next_client;
        ipc.next_client += 1;
        let reader = stream
            .set_nonblocking(true)
            .and_then(|()| stream.try_clone());
        let Ok(reader) = reader else {
            return;
        };

        let mut lines = LineBuffer::default();
        let source = Generic::new(reader, Interest::READ, Mode::Level);
        let watched = self
            .loop_handle
            .insert_source(source, move |_, reader, state| {
                let mut reader: &UnixStream = reader;
                let mut chunk = [0_u8; 4096];
                loop {
                    match reader.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(count) => {
                            for line in lines.push(&chunk[..count]) {
                                state.ipc_command(&line);
                            }
                        }
                        Err(error) if error.kind() == ErrorKind::WouldBlock => {
                            return Ok(PostAction::Continue);
                        }
                        Err(error) if error.kind() == ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
                state.ipc_gone(id);
                Ok(PostAction::Remove)
            });
        if watched.is_err() {
            return;
        }
        debug!(client = id, "a program of the shell is listening");
        ipc.clients.push(Client { id, stream });
        // The newcomer needs the whole state; the others get it again.
        ipc.sent = None;
        ipc.stale = true;
    }

    fn ipc_gone(&mut self, id: u64) {
        if let Some(ipc) = self.ipc.as_mut() {
            ipc.clients.retain(|client| client.id != id);
            debug!(client = id, "a program of the shell left");
        }
    }

    /// Something the shell shows may have changed: it will be told at the
    /// end of this turn of the event loop.
    pub fn shell_changed(&mut self) {
        if let Some(ipc) = self.ipc.as_mut() {
            ipc.stale = true;
        }
    }

    /// Tells the shell the state, when it changed since it was last told.
    pub fn flush_ipc(&mut self) {
        let due = |ipc: &Ipc| ipc.stale && !ipc.clients.is_empty();
        if !self.ipc.as_ref().is_some_and(due) {
            return;
        }
        let snapshot = self.snapshot();
        let Some(ipc) = self.ipc.as_mut() else {
            return;
        };
        ipc.stale = false;
        if ipc.sent.as_ref() != Some(&snapshot) {
            ipc.broadcast(&Event::State(snapshot.clone()));
            ipc.sent = Some(snapshot);
        }
    }

    /// What the shell shows of the compositor, as of now.
    fn snapshot(&self) -> Snapshot {
        let desk = &self.shell.desk;
        let screens = self.outputs.iter().filter_map(|screen| {
            let info = self.shell.screen(screen.id)?;
            let model = desk.screen(screen.id)?;
            Some(Screen {
                name: screen.output.name(),
                role: info.role,
                width: info.rect.w,
                height: info.rect.h,
                layout: model.layout(),
                anchored: model.anchored().is_some(),
            })
        });
        let windows = desk.recent_windows().filter_map(|id| {
            let managed = self.shell.by_id(id)?;
            let geometry = managed.geometry;
            let slot = match desk.placement(id) {
                Some(Placement::Slot(slot)) => Some(slot.index),
                _ => None,
            };
            Some(Window {
                id: id.0,
                app: managed.app_id.clone(),
                title: managed.title.clone(),
                screen: self.shell.screen(geometry.output)?.role,
                slot,
                active: geometry.focused,
                minimized: geometry.minimized,
            })
        });
        let habits = desk.memory().iter().map(|(app, place)| Habit {
            app: app.as_str().to_owned(),
            screen: place.role,
            slot: place.slot,
        });
        Snapshot {
            theme: self.theme.kind.into(),
            accent: self.theme.accent.hex(),
            screens: screens.collect(),
            windows: windows.collect(),
            habits: habits.collect(),
        }
    }

    /// A program of the shell asks for something. Whatever it sent, the
    /// compositor goes on.
    fn ipc_command(&mut self, line: &str) {
        let command = match decode::<Command>(line) {
            Ok(command) => command,
            Err(error) => {
                warn!(%error, line, "the shell sent something that is not a command");
                return;
            }
        };
        debug!(?command, "asked by the shell");
        match command {
            Command::Activate { window } => {
                // Focusing a hidden window brings it back.
                match self.shell.desk.focus(WindowId(window)) {
                    Ok(()) => self.apply_arrangement(),
                    Err(error) => warn!(%error, "cannot show the window the shell asked for"),
                }
            }
            Command::Close { window } => self.close_window(WindowId(window)),
            Command::OpenOn { apps, screen } => {
                let wanted = self.shell.screens.iter().find(|known| known.role == screen);
                let names: Vec<AppKey> = apps.into_iter().map(AppKey::new).collect();
                let asked = wanted
                    .map(|known| known.id)
                    .map(|output| self.shell.desk.open_next_on(names, output));
                if !matches!(asked, Some(Ok(()))) {
                    warn!(?screen, "the shell asked for a screen that is not there");
                }
            }
            Command::Forget { app } => {
                self.shell.desk.forget_app(&AppKey::new(app));
                // Saves the habits, and tells the shell they changed.
                self.apply_arrangement();
            }
            Command::ToggleLauncher => self.toggle_launcher(),
            Command::SetTheme { theme } => self.set_theme(theme.into()),
            Command::Quit => {
                info!("quitting: asked by the shell");
                self.loop_signal.stop();
            }
        }
    }

    /// The shortcut of the drawer: the dock opens or closes it. A game that
    /// holds the pointer lets go of it, or the drawer could not be clicked.
    pub fn toggle_launcher(&mut self) {
        self.free_pointer();
        // Each screen may have its dock: the launcher opens where the
        // pointer is.
        let screen = self.pointer_screen().map(|screen| screen.role);
        if let Some(ipc) = self.ipc.as_mut() {
            ipc.broadcast(&Event::ToggleLauncher { screen });
        }
    }

    /// Switches between the light and the dark theme, keeping the accent
    /// the user chose.
    pub fn set_theme(&mut self, kind: ThemeKind) {
        if self.theme.kind == kind {
            return;
        }
        info!(?kind, "theme changed");
        let theme = Theme::new(kind);
        self.theme = match self.accent {
            Some(accent) => theme.with_accent(accent),
            None => theme,
        };
        self.full_redraw = true;
        self.shell_changed();
        self.queue_redraw();
    }
}
