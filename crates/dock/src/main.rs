//! The dock of the coquille shell: a bar on the left screen, the drawer of
//! applications it opens, the control center and its mixer.

mod app;
mod backend;
mod config;
mod model;
mod steam;
mod ui;

use std::borrow::Cow;
use std::path::PathBuf;

use clap::Parser;
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};

use crate::app::{Dock, Options};
use crate::backend::Backend;

/// Le dock de coquille : barre, tiroir d'applications, centre de contrôle.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Simule tout : fenêtres, son, réseau, Bluetooth, luminosité, session.
    /// Rien n'est lu ni modifié sur le système.
    #[arg(long)]
    mock: bool,

    /// Les services réels à utiliser, séparés par des virgules ; les autres
    /// sont simulés. Sans cette option : tous.
    #[arg(long, value_enum, value_delimiter = ',', value_name = "SERVICES")]
    backends: Option<Vec<Backend>>,

    /// L'écran où s'afficher, par son nom ; par défaut l'écran gauche que
    /// désigne le compositeur.
    #[arg(long, value_name = "NOM")]
    output: Option<String>,

    /// Fichier de configuration (défaut : ~/.config/coquille/config.toml).
    #[arg(long, value_name = "FICHIER")]
    config: Option<PathBuf>,

    /// Dossier où garder les volumes par application
    /// (défaut : ~/.local/state/coquille).
    #[arg(long, value_name = "DOSSIER")]
    state_dir: Option<PathBuf>,

    /// Ouvre le tiroir dès le démarrage.
    #[arg(long)]
    drawer: bool,

    /// Ouvre le tiroir et le mixeur dès le démarrage.
    #[arg(long)]
    mixer: bool,

    /// Ralentit les animations autant de fois (pour les regarder, ou les
    /// capturer en route).
    #[arg(long, value_name = "FOIS", default_value_t = 1.0)]
    animation_slowdown: f32,

    /// Force le mode spatial (barre verticale 56 px et lanceur spatial).
    #[arg(long)]
    spatial: bool,
}

fn state_dir() -> PathBuf {
    let home = || PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"));
    base.join("coquille")
}

fn main() -> iced_layershell::Result {
    let cli = Cli::parse();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(
            "warn,wgpu_hal=error,usvg=error,svgtree=error,layershellev=error,coquille_dock=info",
        )
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    // Simulated, the dock writes nowhere the real one would: its state goes
    // to a directory of its own.
    let real = match (cli.backends, cli.mock) {
        (Some(backends), _) => backends,
        (None, true) => Vec::new(),
        (None, false) => Backend::DEFAULT.to_vec(),
    };
    let scratch = || std::env::temp_dir().join("coquille-dock-mock");
    let options = Options {
        real,
        output: cli.output,
        config: cli
            .config
            .or_else(config::default_path)
            .unwrap_or_else(|| PathBuf::from("config.toml")),
        state_dir: cli
            .state_dir
            .unwrap_or_else(|| if cli.mock { scratch() } else { state_dir() }),
        drawer: cli.drawer,
        mixer: cli.mixer,
        slowdown: cli.animation_slowdown,
        spatial: cli.spatial,
    };

    let fonts = ui::style::FIGTREE
        .iter()
        .map(|font| Cow::Borrowed(*font))
        .collect();
    iced_layershell::daemon(
        move || Dock::new(options.clone()),
        || "coquille-dock".to_owned(),
        Dock::update,
        Dock::view,
    )
    .style(Dock::style)
    .subscription(Dock::subscription)
    .settings(Settings {
        layer_settings: LayerShellSettings {
            // No surface at first: the bar opens once its screen is known.
            start_mode: StartMode::Background,
            ..LayerShellSettings::default()
        },
        fonts,
        default_font: ui::style::REGULAR,
        antialiasing: true,
        ..Settings::default()
    })
    .run()
}
