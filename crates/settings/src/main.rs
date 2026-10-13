//! The settings screen of the coquille shell: an ordinary window, which
//! writes `~/.config/coquille/config.toml` and has the compositor and the
//! dock read it again.

mod app;
mod model;
mod view;

use std::path::PathBuf;

use clap::Parser;
use coquille_ui::style;
use iced::window;

use crate::app::{App, Options};
use crate::model::Page;

/// What windows of this program are called: the name of its desktop entry,
/// which gives its title bar a name and an icon.
const APP_ID: &str = "coquille-settings";

/// Les paramètres de coquille : apparence, écrans et dock, mode jeu, système.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Fichier de configuration (défaut : ~/.config/coquille/config.toml).
    #[arg(long, value_name = "FICHIER")]
    config: Option<PathBuf>,

    /// La page sur laquelle s'ouvrir.
    #[arg(long, value_enum, default_value = "appearance", value_name = "PAGE")]
    page: Page,
}

fn main() -> iced::Result {
    let cli = Cli::parse();
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new("warn,wgpu_hal=error,usvg=error,coquille_settings=info")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    let options = Options {
        config: cli
            .config
            .or_else(coquille_config::default_path)
            .unwrap_or_else(|| PathBuf::from("config.toml")),
        page: cli.page,
    };

    let mut application =
        iced::application(move || App::new(options.clone()), App::update, App::view)
            .title("Paramètres")
            .subscription(App::subscription)
            .style(App::style)
            .theme(App::iced_theme)
            .default_font(style::REGULAR)
            .antialiasing(true)
            .window(window::Settings {
                size: iced::Size::new(900.0, 640.0),
                min_size: Some(iced::Size::new(720.0, 480.0)),
                platform_specific: window::settings::PlatformSpecific {
                    application_id: APP_ID.to_owned(),
                    ..Default::default()
                },
                ..window::Settings::default()
            });
    for font in style::FIGTREE {
        application = application.font(*font);
    }
    application.run()
}
