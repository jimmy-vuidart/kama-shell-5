//! The polkit agent of the coquille shell: when an application needs an
//! administrator (an update, a disk to mount, the time to change), polkit
//! asks the agent of the session, which shows a dialog and has the helper
//! of polkit check the password.
//!
//! It registers for the login session on the system bus and stays out of
//! sight until polkit asks something. The compositor starts it, and it
//! ends with it.

mod agent;
mod app;
mod bus;
mod helper;
mod service;
mod view;

use std::borrow::Cow;

use clap::Parser;
use coquille_ui::style;
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};

use crate::app::{Options, Prompt};
use crate::bus::Subject;
use crate::service::Mode;

/// L'agent polkit de coquille : demande le mot de passe qu'une action
/// d'administration exige.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Ne sert que les demandes de ce processus, au lieu de celles de la
    /// session (pour essayer à côté d'un autre agent).
    #[arg(long, value_name = "PID")]
    process: Option<u32>,

    /// Montre une demande inventée, sans polkit, puis s'arrête. Le bon mot
    /// de passe est « coquille ».
    #[arg(long, conflicts_with = "process")]
    demo: bool,
}

fn main() -> iced_layershell::Result {
    let cli = Cli::parse();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(
            "warn,wgpu_hal=error,usvg=error,svgtree=error,layershellev=error,coquille_polkit=info",
        )
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let mode = match (cli.demo, cli.process) {
        (true, _) => Mode::Demo,
        (false, Some(pid)) => Mode::Real(Subject::Process(pid)),
        (false, None) => Mode::Real(Subject::Session),
    };
    let options = Options {
        mode,
        config: coquille_config::default_path(),
    };

    let fonts = style::FIGTREE
        .iter()
        .map(|font| Cow::Borrowed(*font))
        .collect();
    iced_layershell::daemon(
        move || Prompt::new(options.clone()),
        || "coquille-polkit".to_owned(),
        Prompt::update,
        Prompt::view,
    )
    .style(Prompt::style)
    .subscription(Prompt::subscription)
    .settings(Settings {
        layer_settings: LayerShellSettings {
            // Nothing shows until polkit asks.
            start_mode: StartMode::Background,
            ..LayerShellSettings::default()
        },
        fonts,
        default_font: style::REGULAR,
        antialiasing: true,
        ..Settings::default()
    })
    .run()
}
