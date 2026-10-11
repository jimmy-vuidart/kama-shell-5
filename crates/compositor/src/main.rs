//! coquille: a Wayland compositor built around "Center + companions".
//!
//! Phase 1: nested in a window of the current session, one simulated output.

mod debug;
mod decoration;
mod handlers;
mod input;
mod keyboard_layout;
mod render;
mod shell;
mod shortcuts;
mod state;
mod theme;
mod winit;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use coquille_model::OutputRole;
use smithay::backend::winit::WinitEvent;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction};
use smithay::reexports::wayland_server::Display;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::{error, info, warn};

use crate::state::{ClientState, Options, State};
use crate::theme::{Theme, ThemeKind};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Role {
    Left,
    Center,
    Right,
}

impl From<Role> for OutputRole {
    fn from(role: Role) -> Self {
        match role {
            Role::Left => Self::Left,
            Role::Center => Self::Center,
            Role::Right => Self::Right,
        }
    }
}

/// Compositeur Wayland « Centre + compagnons », en mode imbriqué.
///
/// Raccourcis : Alt+1 disposition « plein », Alt+2 « côte à côte », Alt+Page suiv./préc.
/// onglet suivant/précédent, Alt+Maj+E quitter.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    /// Rôle de l'écran simulé : un écran latéral démarre en « côte à côte »,
    /// le centre en « plein ».
    #[arg(long, value_enum, default_value = "right")]
    role: Role,

    /// Thème des décorations.
    #[arg(long, value_enum, default_value = "light")]
    theme: ThemeKind,

    /// Occupe tout l'écran où la fenêtre s'ouvre, pour simuler un écran
    /// réel. Alt+Maj+E pour quitter.
    #[arg(long)]
    fullscreen: bool,

    /// Disposition du clavier (par exemple « fr »). Par défaut, celle du
    /// système.
    #[arg(long)]
    keyboard_layout: Option<String>,

    /// Variante de la disposition du clavier.
    #[arg(long, requires = "keyboard_layout")]
    keyboard_variant: Option<String>,

    /// Écrit le journal dans ce fichier plutôt que dans le terminal.
    #[arg(long, value_name = "FICHIER")]
    log_file: Option<PathBuf>,

    /// Commande à lancer dans le compositeur ; l'option peut être répétée.
    #[arg(long, value_name = "COMMANDE")]
    spawn: Vec<String>,

    /// Débogage : enregistre une image de la sortie puis quitte.
    #[arg(long, value_name = "FICHIER")]
    screenshot: Option<PathBuf>,

    /// Débogage : délai avant la capture, en millisecondes.
    #[arg(long, default_value_t = 2000, value_name = "MS")]
    screenshot_delay: u64,

    /// Débogage : rejoue des entrées, voir `debug.rs` pour la syntaxe.
    // Spelled with its full path so that clap reads one value holding a
    // whole script, not an option that may be repeated.
    #[arg(long, hide = true, value_name = "SCRIPT", value_parser = debug::parse)]
    debug_input: Option<std::vec::Vec<debug::TimedStep>>,
}

/// What event sources get: the display must stay outside the state it
/// dispatches to.
struct App {
    display: Display<State>,
    state: State,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(cli.log_file.as_deref())?;

    let mut event_loop: EventLoop<App> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    let mut display: Display<State> = Display::new()?;

    let (backend, winit_events) = winit::init(cli.fullscreen)?;

    let socket = ListeningSocketSource::new_auto().context("cannot create the Wayland socket")?;
    let socket_name = socket.socket_name().to_os_string();
    handle
        .insert_source(socket, |stream, _, app: &mut App| {
            let client = Arc::new(ClientState::default());
            if let Err(error) = app.state.display_handle.insert_client(stream, client) {
                warn!(%error, "cannot accept a client");
            }
        })
        .map_err(|error| anyhow::anyhow!("cannot listen on the socket: {error}"))?;

    let poll_fd = display.backend().poll_fd().try_clone_to_owned()?;
    handle
        .insert_source(
            Generic::new(poll_fd, Interest::READ, Mode::Level),
            |_, _, app: &mut App| {
                if let Err(error) = app.display.dispatch_clients(&mut app.state) {
                    error!(%error, "cannot dispatch client requests");
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| anyhow::anyhow!("cannot watch the display: {error}"))?;

    handle
        .insert_source(winit_events, |event, _, app: &mut App| {
            let state = &mut app.state;
            match event {
                WinitEvent::Resized { size, .. } => state.resize_output(size),
                WinitEvent::Input(event) => state.process_input_event(event),
                WinitEvent::Redraw => state.queue_redraw(),
                WinitEvent::CloseRequested => state.loop_signal.stop(),
                WinitEvent::Focus(_) => {}
            }
        })
        .map_err(|error| anyhow::anyhow!("cannot watch the host window: {error}"))?;

    let options = Options {
        role: cli.role.into(),
        theme: Theme::new(cli.theme),
        keyboard: keyboard_layout::detect(
            cli.keyboard_layout.as_deref(),
            cli.keyboard_variant.as_deref(),
        ),
    };
    let state = State::new(
        display.handle(),
        event_loop.get_signal(),
        socket_name,
        backend,
        options,
    )?;
    let mut app = App { display, state };
    info!(socket = ?app.state.socket_name, "listening");

    let frame = Duration::from_secs(1000) / state::REFRESH.unsigned_abs();
    handle
        .insert_source(Timer::from_duration(frame), move |_, _, app: &mut App| {
            app.state.frame_tick();
            TimeoutAction::ToDuration(frame)
        })
        .map_err(|error| anyhow::anyhow!("cannot arm the frame timer: {error}"))?;

    if let Some(path) = cli.screenshot {
        let timer = Timer::from_duration(Duration::from_millis(cli.screenshot_delay));
        handle
            .insert_source(timer, move |_, _, app: &mut App| {
                app.state.backend.screenshot = Some(path.clone());
                app.state.render();
                TimeoutAction::Drop
            })
            .map_err(|error| anyhow::anyhow!("cannot arm the screenshot timer: {error}"))?;
    }

    let mut elapsed = Duration::ZERO;
    for timed in cli.debug_input.unwrap_or_default() {
        elapsed += Duration::from_millis(timed.after_ms);
        handle
            .insert_source(Timer::from_duration(elapsed), move |_, _, app: &mut App| {
                app.state.debug_input(timed.step);
                TimeoutAction::Drop
            })
            .map_err(|error| anyhow::anyhow!("cannot arm a debug input timer: {error}"))?;
    }

    for command in &cli.spawn {
        spawn(command, &app.state);
    }

    event_loop.run(None, &mut app, |app| {
        if let Err(error) = app.state.display_handle.flush_clients() {
            warn!(%error, "cannot flush clients");
        }
    })?;
    Ok(())
}

/// Logs to the terminal, or to a file when the compositor is started from a
/// menu. Panics are logged too, so that a crash leaves a trace either way.
fn init_logging(file: Option<&Path>) -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match file {
        Some(path) => {
            if let Some(directory) = path.parent() {
                fs::create_dir_all(directory)
                    .with_context(|| format!("cannot create {}", directory.display()))?;
            }
            let file = File::create(path)
                .with_context(|| format!("cannot write the log to {}", path.display()))?;
            builder
                .with_ansi(false)
                .with_writer(Mutex::new(file))
                .init();
        }
        None => builder.init(),
    }

    std::panic::set_hook(Box::new(|panic| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        error!("{panic}\n{backtrace}");
    }));
    Ok(())
}

/// Runs a shell command as a client of this compositor.
fn spawn(command: &str, state: &State) {
    let child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .env("WAYLAND_DISPLAY", &state.socket_name)
        // Without this, toolkits may prefer the host's X server.
        .env_remove("DISPLAY")
        .spawn();
    match child {
        Ok(child) => info!(command, pid = child.id(), "spawned"),
        Err(error) => warn!(command, %error, "cannot spawn"),
    }
}
