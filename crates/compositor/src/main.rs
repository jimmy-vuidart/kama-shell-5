//! coquille: a Wayland compositor built around "Center + companions".

mod backend;
mod config;
mod cursor;
mod debug;
mod decoration;
mod focus;
mod game;
mod handlers;
mod input;
mod ipc;
mod keyboard_layout;
mod outputs;
mod panels;
mod persist;
mod placement;
mod render;
mod resize;
mod shell;
mod shortcuts;
mod state;
mod theme;
mod xwayland;

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use coquille_model::OutputRole;
use smithay::backend::winit::WinitEvent;
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction};
use smithay::reexports::wayland_server::Display;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::{error, info, warn};

use crate::backend::Backend;
use crate::backend::winit::{REFRESH, Simulated};
use crate::config::Config;
use crate::shortcuts::ModKey;
use crate::state::{ClientState, Options, State};
use crate::theme::{Rgba, ThemeKind};

/// Where the compositor runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BackendChoice {
    /// Nested inside a graphical session if there is one, else on the
    /// hardware.
    Auto,
    /// In a window of the current session.
    Nested,
    /// On the real screens, from a virtual terminal or a login manager.
    Hardware,
}

impl BackendChoice {
    fn is_nested(self) -> bool {
        match self {
            Self::Nested => true,
            Self::Hardware => false,
            Self::Auto => ["WAYLAND_DISPLAY", "DISPLAY"]
                .iter()
                .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
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

/// Compositeur Wayland « Centre + compagnons ».
///
/// Lancé depuis une session graphique, il s'ouvre dans une fenêtre (mode
/// imbriqué). La touche des raccourcis, notée Mod, est Super sur le matériel
/// et Alt en mode imbriqué.
///
/// Souris : glisser une barre de titre ou une capsule montre les
/// emplacements, relâcher range la fenêtre ; un clic sur une capsule affiche
/// sa fenêtre ; Mod+glisser la déplace librement ; tirer un bord ou un coin
/// d'une fenêtre flottante la redimensionne ; Mod+glisser sur la séparation
/// de deux emplacements règle leur ratio.
///
/// Clavier : Mod+Entrée terminal ; Mod+1 à 5 disposition de l'écran du
/// pointeur (plein, côte à côte, haut/bas, écran libre, deux tiers + un
/// tiers) ; Mod+Page suiv./préc. onglet suivant/précédent ; Mod+R restaure la
/// dernière fenêtre réduite ; Mod+Maj+E quitter.
/// Ctrl+Alt+Retour arrière quitte aussi, quoi qu'il arrive.
///
/// Mode jeu : une fenêtre en plein écran ancre son écran (ni décoration ni
/// geste du shell). Mod seul libère le pointeur retenu par le jeu ;
/// Mod+Échap remet le jeu en fenêtre.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    /// Où tourner : dans une fenêtre de la session actuelle (nested) ou sur
    /// les vrais écrans (hardware). Par défaut, selon l'environnement.
    #[arg(long, value_enum, default_value = "auto")]
    backend: BackendChoice,

    /// Mode imbriqué : rôle de l'écran simulé. Un écran latéral démarre en
    /// « côte à côte », le centre en « plein ».
    #[arg(long, value_enum, default_value = "right")]
    role: Role,

    /// Mode imbriqué : simule les trois écrans côte à côte dans la fenêtre.
    #[arg(long, conflicts_with = "role")]
    three_screens: bool,

    /// Mode imbriqué : occupe tout l'écran où la fenêtre s'ouvre.
    #[arg(long)]
    fullscreen: bool,

    /// Thème des décorations (sinon celui du fichier de configuration).
    #[arg(long, value_enum)]
    theme: Option<ThemeKind>,

    /// Disposition du clavier (par exemple « fr »). Par défaut, celle de la
    /// configuration, sinon celle du système.
    #[arg(long)]
    keyboard_layout: Option<String>,

    /// Variante de la disposition du clavier.
    #[arg(long, requires = "keyboard_layout")]
    keyboard_variant: Option<String>,

    /// Fichier de configuration (défaut : ~/.config/coquille/config.toml).
    #[arg(long, value_name = "FICHIER")]
    config: Option<PathBuf>,

    /// Dossier où garder la mémoire par application. Par défaut
    /// ~/.local/state/coquille sur le matériel, et rien en mode imbriqué.
    #[arg(long, value_name = "DOSSIER")]
    state_dir: Option<PathBuf>,

    /// Écrit le journal dans ce fichier plutôt que dans le terminal.
    #[arg(long, value_name = "FICHIER")]
    log_file: Option<PathBuf>,

    /// Commande à lancer dans le compositeur ; l'option peut être répétée.
    #[arg(long, value_name = "COMMANDE")]
    spawn: Vec<String>,

    /// Ne démarre pas XWayland : les applications X11 (dont beaucoup de
    /// jeux) ne pourront pas s'ouvrir.
    #[arg(long)]
    no_xwayland: bool,

    /// Session de connexion : indique l'affichage aux services de la session
    /// (activation D-Bus, portails). À ne pas utiliser à côté d'une autre
    /// session graphique du même utilisateur, qui partage ces services.
    #[arg(long)]
    export_environment: bool,

    /// Quitte tout seul après ce délai : filet de sécurité pour un premier
    /// essai sur le matériel.
    #[arg(long, value_name = "SECONDES")]
    exit_after: Option<u64>,

    /// Débogage : examine la carte graphique et les écrans sans rien
    /// afficher, puis quitte. Fonctionne depuis une session graphique.
    #[arg(long)]
    probe: bool,

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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging(cli.log_file.as_deref())?;
    let config = config::load(cli.config.as_deref());
    if cli.probe {
        return backend::udev::probe(&config.outputs);
    }

    let mut event_loop: EventLoop<'static, State> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    let mut display: Display<State> = Display::new()?;

    let socket = ListeningSocketSource::new_auto().context("cannot create the Wayland socket")?;
    let socket_name = socket.socket_name().to_os_string();
    handle
        .insert_source(socket, |stream, _, state| {
            let client = Arc::new(ClientState::default());
            if let Err(error) = state.display_handle.insert_client(stream, client) {
                warn!(%error, "cannot accept a client");
            }
        })
        .map_err(|error| anyhow::anyhow!("cannot listen on the socket: {error}"))?;

    // The display dispatches to the state, so it cannot live inside it: the
    // source that wakes up on client requests owns it instead.
    let display_handle = display.handle();
    let poll_fd = display.backend().poll_fd().try_clone_to_owned()?;
    handle
        .insert_source(
            Generic::new(poll_fd, Interest::READ, Mode::Level),
            move |_, _, state| {
                if let Err(error) = display.dispatch_clients(state) {
                    error!(%error, "cannot dispatch client requests");
                }
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| anyhow::anyhow!("cannot watch the display: {error}"))?;

    let nested = cli.backend.is_nested();
    let (backend, screens) = if nested {
        let simulated = if cli.three_screens {
            Simulated::Three
        } else {
            Simulated::One(cli.role.into())
        };
        // A script must not be disturbed by the real mouse passing over the
        // window.
        let scripted = cli.debug_input.is_some();
        init_nested(&handle, simulated, cli.fullscreen, scripted)?
    } else {
        let (udev, screens) = backend::udev::init(&handle, &config.outputs, &config.game)?;
        (Backend::Udev(Box::new(udev)), screens)
    };

    // Nested, the host session keeps Super for itself.
    let mod_key = if nested { ModKey::Alt } else { ModKey::Super };
    let mut options = options(&cli, &config, mod_key);
    // Nested runs are trials: they do not touch the habits of real sessions
    // unless asked to.
    options.state_dir = match (&cli.state_dir, nested) {
        (Some(directory), _) => Some(directory.clone()),
        (None, false) => persist::default_dir(),
        (None, true) => None,
    };
    let mut state = State::new(
        display_handle,
        event_loop.get_signal(),
        handle.clone(),
        socket_name,
        backend,
        screens,
        options,
    )?;
    state.start_ipc();
    if !cli.no_xwayland && config.xwayland.unwrap_or(true) {
        // What the X server says goes next to the compositor's own log.
        let log = cli.log_file.as_deref().and_then(Path::parent);
        let log = log.map(|directory| directory.join("xwayland.log"));
        state.start_xwayland(log.as_deref());
    } else {
        info!("XWayland is turned off: X11 applications will not run");
    }
    if let Backend::Udev(udev) = &mut state.backend {
        udev.init_explicit_sync(&state.display_handle);
        if cli.export_environment {
            export_environment(&state);
        }
        state.center_pointer();
        // The first frame must not wait for the first input event.
        handle.insert_idle(|state| state.redraw_screens());
    }
    info!(socket = ?state.socket_name, "listening");

    if let Some(path) = cli.screenshot {
        let timer = Timer::from_duration(Duration::from_millis(cli.screenshot_delay));
        handle
            .insert_source(timer, move |_, _, state| {
                if let Backend::Winit(winit) = &mut state.backend {
                    winit.screenshot = Some(path.clone());
                }
                state.render_window();
                TimeoutAction::Drop
            })
            .map_err(|error| anyhow::anyhow!("cannot arm the screenshot timer: {error}"))?;
    }

    if let Some(seconds) = cli.exit_after {
        // Should the event loop itself be stuck, the timer below never fires:
        // this thread ends the process anyway, and the session with it.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(seconds + 5));
            error!("still running after the --exit-after delay: exiting hard");
            std::process::exit(3);
        });
        handle
            .insert_source(
                Timer::from_duration(Duration::from_secs(seconds)),
                |_, _, state| {
                    info!("quitting: the --exit-after delay is over");
                    state.loop_signal.stop();
                    TimeoutAction::Drop
                },
            )
            .map_err(|error| anyhow::anyhow!("cannot arm the exit timer: {error}"))?;
    }

    let mut elapsed = Duration::ZERO;
    for timed in cli.debug_input.unwrap_or_default() {
        elapsed += Duration::from_millis(timed.after_ms);
        handle
            .insert_source(Timer::from_duration(elapsed), move |_, _, state| {
                state.debug_input(timed.step);
                TimeoutAction::Drop
            })
            .map_err(|error| anyhow::anyhow!("cannot arm a debug input timer: {error}"))?;
    }

    for command in &cli.spawn {
        state.spawn(command);
    }

    event_loop.run(None, &mut state, |state| {
        // On real screens, frames start as soon as something changed; the
        // nested window draws on its own timer.
        state.redraw_screens();
        state.flush_ipc();
        if let Err(error) = state.display_handle.flush_clients() {
            warn!(%error, "cannot flush clients");
        }
    })?;
    info!("stopped");
    Ok(())
}

/// Tells the session's services (D-Bus activation, portals) which display
/// to use, as they are started outside of the compositor.
fn export_environment(state: &State) {
    let mut variables = vec![
        format!("WAYLAND_DISPLAY={}", state.socket_name.to_string_lossy()),
        "XDG_CURRENT_DESKTOP=coquille".to_owned(),
    ];
    // Where the dock, started by those services, finds the compositor.
    if let Some(ipc) = &state.ipc {
        variables.push(format!(
            "{}={}",
            coquille_ipc::SOCKET_ENV,
            ipc.path.display()
        ));
    }
    // Where X11 applications started by those services find the X server.
    if let Some(number) = state.xdisplay {
        variables.push(format!("DISPLAY={}", xwayland::display_name(number)));
    }
    let result = std::process::Command::new("dbus-update-activation-environment")
        .arg("--systemd")
        .args(&variables)
        .status();
    match result {
        Ok(status) if status.success() => {}
        Ok(status) => warn!(%status, "cannot export the environment to the session"),
        Err(error) => warn!(%error, "cannot export the environment to the session"),
    }
}

/// Opens the host window and the timer that paces its frames.
fn init_nested(
    handle: &LoopHandle<'static, State>,
    simulated: Simulated,
    fullscreen: bool,
    ignore_input: bool,
) -> anyhow::Result<(Backend, Vec<(shell::ScreenInfo, smithay::output::Output)>)> {
    let (winit, events) = backend::winit::init(simulated, fullscreen)?;
    let screens = winit.screens();

    handle
        .insert_source(events, move |event, _, state| match event {
            WinitEvent::Resized { size, .. } => state.resize_window(size),
            WinitEvent::Input(_) if ignore_input => {}
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::Redraw => state.queue_redraw(),
            WinitEvent::CloseRequested => state.loop_signal.stop(),
            WinitEvent::Focus(_) => {}
        })
        .map_err(|error| anyhow::anyhow!("cannot watch the host window: {error}"))?;

    let frame = Duration::from_secs(1000) / REFRESH.unsigned_abs();
    handle
        .insert_source(Timer::from_duration(frame), move |_, _, state| {
            state.frame_tick();
            TimeoutAction::ToDuration(frame)
        })
        .map_err(|error| anyhow::anyhow!("cannot arm the frame timer: {error}"))?;

    Ok((Backend::Winit(Box::new(winit)), screens))
}

/// Settings from the command line first, then the configuration file, then
/// the system.
fn options(cli: &Cli, config: &Config, mod_key: ModKey) -> Options {
    let layout = cli
        .keyboard_layout
        .as_deref()
        .or(config.keyboard.layout.as_deref());
    let variant = cli
        .keyboard_variant
        .as_deref()
        .or(config.keyboard.variant.as_deref());
    let accent = config.accent.as_deref().and_then(|text| {
        let accent = Rgba::parse(text);
        if accent.is_none() {
            warn!(
                accent = text,
                "not a color (#rrggbb): using the accent of the theme"
            );
        }
        accent
    });
    Options {
        theme: cli.theme.or(config.theme).unwrap_or(ThemeKind::Light),
        accent,
        keyboard: keyboard_layout::detect(layout, variant),
        mod_key,
        terminal: config.terminal.clone().unwrap_or_else(default_terminal),
        state_dir: None,
        sticky_edges_ms: config.game.sticky_edges_ms(),
    }
}

/// The first terminal found among the usual ones.
fn default_terminal() -> String {
    let installed = |name: &str| {
        std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|directory| directory.join(name).is_file())
        })
    };
    let candidates = ["kitty", "alacritty", "foot"];
    let found = candidates.into_iter().find(|name| installed(name));
    found.unwrap_or(candidates[0]).to_owned()
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
