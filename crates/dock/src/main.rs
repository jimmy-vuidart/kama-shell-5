//! Probe: one panel on the left edge that keeps 60 pixels for itself.

use iced::widget::{container, text};
use iced::{Border, Color, Element, Length, Task};
use iced_layershell::daemon;
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::settings::{LayerShellSettings, Settings, StartMode};
use iced_layershell::to_layer_message;

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
enum Message {
    Started,
}

struct Dock;

fn boot() -> (Dock, Task<Message>) {
    (Dock, Task::done(Message::Started))
}

fn update(_dock: &mut Dock, message: Message) -> Task<Message> {
    match message {
        Message::Started => {
            let output = std::env::args().nth(1);
            let (_id, task) = Message::layershell_open(NewLayerShellSettings {
                size: Some((60, 0)),
                layer: Layer::Top,
                anchor: Anchor::Left | Anchor::Top | Anchor::Bottom,
                exclusive_zone: Some(60),
                margin: None,
                keyboard_interactivity: KeyboardInteractivity::None,
                output_option: output.map_or(OutputOption::Active, OutputOption::OutputName),
                events_transparent: false,
                namespace: Some("coquille-dock".into()),
            });
            task
        }
        _ => Task::none(),
    }
}

fn view(_dock: &Dock, _id: iced::window::Id) -> Element<'_, Message> {
    let bar = container(text("21\n14").size(13))
        .width(44)
        .height(300)
        .style(|_| container::Style {
            background: Some(Color::from_rgba8(30, 35, 45, 0.96).into()),
            text_color: Some(Color::WHITE),
            border: Border {
                radius: 14.0.into(),
                ..Border::default()
            },
            ..container::Style::default()
        });
    container(bar).padding([0, 8]).center_y(Length::Fill).into()
}

fn main() -> iced_layershell::Result {
    daemon(boot, || "coquille-dock".to_owned(), update, view)
        .style(|_, theme: &iced::Theme| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme.palette().text,
        })
        .settings(Settings {
            layer_settings: LayerShellSettings {
                start_mode: StartMode::Background,
                ..LayerShellSettings::default()
            },
            ..Settings::default()
        })
        .run()
}
