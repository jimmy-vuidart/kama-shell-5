//! The state of the settings screen and everything that changes it. Each
//! change is written to the configuration file at once, then the compositor
//! is asked to read the file again.

use std::collections::HashMap;
use std::path::PathBuf;

use coquille_config::{Change, Settings, apply, assign};
use coquille_ipc::{Command, Event, Snapshot, ThemeName};
use coquille_model::OutputRole;
use coquille_ui::compositor;
use coquille_ui::style::{Palette, parse_color};
use iced::{Element, Subscription, Task};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{info, warn};

use crate::model::{self, Choice, Layouts, Page};
use crate::view;

/// How the settings screen was started.
#[derive(Debug, Clone)]
pub struct Options {
    pub config: PathBuf,
    /// The page to open on.
    pub page: Page,
}

#[derive(Debug, Clone)]
pub enum Message {
    Compositor(compositor::Update),
    Page(Page),
    /// A setting changed: it is written, and applied where it can be.
    Change(Change),
    /// The accent, as it is being typed.
    AccentText(String),
    /// The slider of the sticky edges moves, then is let go.
    Sticky(u32),
    StickyReleased,
    /// The mouse speed slider moves, then is let go.
    MouseSpeed(i32),
    MouseSpeedReleased,
    /// The touchpad speed slider moves, then is let go.
    TouchpadSpeed(i32),
    TouchpadSpeedReleased,
    /// The dock icon size slider moves, then is let go.
    DockIconSize(u32),
    DockIconSizeReleased,
    Connector(OutputRole, Choice),
    Layout(Choice),
    Variant(Choice),
    Terminal(Choice),
    /// Audio settings.
    AudioVisibility(String, bool),
    AudioNickChanged(String, String),
    AudioNickSubmit(String),
    /// Spatial tasks API token.
    TasksTokenChanged(String),
    TasksTokenSubmit,
    TasksTokenToggleVisible,
}

pub struct App {
    pub options: Options,
    pub page: Page,
    /// What the file says.
    pub file: Settings,
    /// Why the last change could not be written.
    pub error: Option<String>,
    /// The state of the compositor, when this runs inside coquille.
    pub snapshot: Option<Snapshot>,
    link: Option<UnboundedSender<Command>>,
    pub layouts: Layouts,
    /// The terminals found on the system.
    pub terminals: Vec<&'static str>,
    /// The accent as typed, which may not be a color yet.
    pub accent_text: String,
    /// Where the slider of the sticky edges stands: the delay they have, or
    /// will have when switched on again.
    pub sticky: u32,
    /// Where the mouse speed slider stands (-100..=100).
    pub mouse_speed: i32,
    /// Where the touchpad speed slider stands (-100..=100).
    pub touchpad_speed: i32,
    /// Where the dock icon size slider stands (16..=64).
    pub dock_icon_size: u32,
    /// The audio sinks known to the system.
    pub audio_sinks: Vec<model::AudioDeviceCard>,
    /// The draft nickname for each audio sink by name.
    pub audio_nick_inputs: HashMap<String, String>,
    /// Draft API token for the spatial launcher's tasks widget.
    pub tasks_token_input: String,
    /// Whether the API token field is shown in clear text.
    pub tasks_token_visible: bool,
}

impl App {
    pub fn new(options: Options) -> (Self, Task<Message>) {
        let file = read(&options);
        let audio_sinks = model::load_audio_sinks(&file);
        let mut audio_nick_inputs = HashMap::new();
        for sink in &audio_sinks {
            audio_nick_inputs.insert(sink.name.clone(), sink.nick.clone().unwrap_or_default());
        }
        let app = Self {
            page: options.page,
            accent_text: file.accent.clone().unwrap_or_default(),
            sticky: match file.sticky_edges_ms() {
                0 => model::sticky_on_slider(Settings::DEFAULT_STICKY_EDGES_MS),
                delay => model::sticky_on_slider(delay),
            },
            mouse_speed: model::speed_to_slider(file.mouse_accel_speed()),
            touchpad_speed: model::speed_to_slider(file.touchpad_accel_speed()),
            dock_icon_size: model::dock_icon_size_on_slider(file.dock_icon_size()),
            audio_sinks,
            audio_nick_inputs,
            tasks_token_input: file.tasks_token.clone().unwrap_or_default(),
            tasks_token_visible: false,
            file,
            error: None,
            snapshot: None,
            link: None,
            layouts: Layouts::load(),
            terminals: model::installed_terminals(),
            options,
        };
        (app, Task::none())
    }

    /// The theme in use: the one of the compositor, which the dock can
    /// change too, else the one of the file.
    pub fn theme(&self) -> ThemeName {
        let live = self.snapshot.as_ref().map(|snapshot| snapshot.theme);
        live.unwrap_or(self.file.theme())
    }

    pub fn palette(&self) -> Palette {
        let live = self.snapshot.as_ref().map(|snapshot| &snapshot.accent);
        let accent = live.or(self.file.accent.as_ref());
        Palette::new(self.theme(), accent.and_then(|accent| parse_color(accent)))
    }

    /// Whether changes take effect at once: only inside coquille.
    pub fn connected(&self) -> bool {
        self.link.is_some()
    }

    /// Reads the file again: the dock writes to it too.
    fn read_file(&mut self) {
        let file = read(&self.options);
        // What is being typed or dragged is only replaced by news.
        if file.accent != self.file.accent {
            self.accent_text = file.accent.clone().unwrap_or_default();
        }
        if file.sticky_edges_ms != self.file.sticky_edges_ms && file.sticky_edges_ms() > 0 {
            self.sticky = model::sticky_on_slider(file.sticky_edges_ms());
        }
        if file.mouse_accel_speed != self.file.mouse_accel_speed {
            self.mouse_speed = model::speed_to_slider(file.mouse_accel_speed());
        }
        if file.touchpad_accel_speed != self.file.touchpad_accel_speed {
            self.touchpad_speed = model::speed_to_slider(file.touchpad_accel_speed());
        }
        if file.dock_icon_size != self.file.dock_icon_size {
            self.dock_icon_size = model::dock_icon_size_on_slider(file.dock_icon_size());
        }
        let audio_sinks = model::load_audio_sinks(&file);
        for sink in &audio_sinks {
            let file_nick = sink.nick.as_deref().unwrap_or("");
            let old_file_nick = self.file.audio_sink_nick(&sink.name).unwrap_or("");
            if file_nick != old_file_nick || !self.audio_nick_inputs.contains_key(&sink.name) {
                self.audio_nick_inputs
                    .insert(sink.name.clone(), file_nick.to_owned());
            }
        }
        self.audio_sinks = audio_sinks;
        if file.tasks_token != self.file.tasks_token {
            self.tasks_token_input = file.tasks_token.clone().unwrap_or_default();
        }
        self.file = file;
    }

    /// Writes a change to the file, then has the compositor read it.
    fn change(&mut self, change: Change) {
        info!(?change, "setting changed");
        let path = &self.options.config;
        if let Err(error) = coquille_config::update(path, |text| apply(text, &change)) {
            warn!(path = %path.display(), %error, "cannot write the configuration");
            let text = std::fs::read_to_string(path).unwrap_or_default();
            self.error = Some(if apply(&text, &change).is_none() {
                "Le fichier de configuration n'est pas du TOML valide : rien n'est enregistré \
                 tant qu'il n'est pas corrigé."
                    .to_owned()
            } else {
                format!("Réglage non enregistré : {error}")
            });
            return;
        }
        self.error = None;
        self.read_file();
        if let Some(link) = &self.link {
            let _ = link.send(Command::ReloadConfig);
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Compositor(update) => self.compositor(update),
            Message::Page(page) => self.page = page,
            Message::Change(change) => self.change(change),
            Message::AccentText(text) => {
                let color = parse_color(&text).is_some();
                self.accent_text.clone_from(&text);
                if color {
                    self.change(Change::Accent(Some(text)));
                }
            }
            Message::Sticky(delay) => self.sticky = delay,
            Message::StickyReleased => self.change(Change::StickyEdges(self.sticky.into())),
            Message::MouseSpeed(speed) => self.mouse_speed = speed,
            Message::MouseSpeedReleased => {
                self.change(Change::MouseAccelSpeed(model::slider_to_speed(
                    self.mouse_speed,
                )));
            }
            Message::TouchpadSpeed(speed) => self.touchpad_speed = speed,
            Message::TouchpadSpeedReleased => {
                self.change(Change::TouchpadAccelSpeed(model::slider_to_speed(
                    self.touchpad_speed,
                )));
            }
            Message::DockIconSize(size) => self.dock_icon_size = size,
            Message::DockIconSizeReleased => {
                self.change(Change::DockIconSize(self.dock_icon_size));
            }
            Message::Connector(role, choice) => {
                let Some(connector) = choice.value else {
                    return Task::none();
                };
                let cards = model::screens(&self.file, self.snapshot.as_ref());
                let outputs = assign(&model::connectors(&cards), role, &connector);
                self.change(Change::Outputs(outputs));
            }
            Message::Layout(choice) => {
                // Another layout has other variants.
                let same = choice.value == self.file.keyboard_layout;
                let variant = self.file.keyboard_variant.clone().filter(|_| same);
                self.change(Change::Keyboard {
                    layout: choice.value,
                    variant,
                });
            }
            Message::Variant(choice) => self.change(Change::Keyboard {
                layout: self.file.keyboard_layout.clone(),
                variant: choice.value,
            }),
            Message::Terminal(choice) => self.change(Change::Terminal(choice.value)),
            Message::AudioVisibility(device, visible) => {
                self.change(Change::AudioVisibility { device, visible });
            }
            Message::AudioNickChanged(device, text) => {
                self.audio_nick_inputs.insert(device, text);
            }
            Message::AudioNickSubmit(device) => {
                let text = self
                    .audio_nick_inputs
                    .get(&device)
                    .cloned()
                    .unwrap_or_default();
                let nick = if text.trim().is_empty() {
                    None
                } else {
                    Some(text.trim().to_owned())
                };
                self.change(Change::AudioNick { device, nick });
            }
            Message::TasksTokenChanged(text) => {
                self.tasks_token_input = text;
            }
            Message::TasksTokenSubmit => {
                let trimmed = self.tasks_token_input.trim();
                let token = (!trimmed.is_empty()).then(|| trimmed.to_owned());
                self.change(Change::TasksToken(token));
            }
            Message::TasksTokenToggleVisible => {
                self.tasks_token_visible = !self.tasks_token_visible;
            }
        }
        Task::none()
    }

    fn compositor(&mut self, update: compositor::Update) {
        match update {
            compositor::Update::Connected(link) => self.link = Some(link),
            compositor::Update::Disconnected => {
                self.link = None;
                self.snapshot = None;
            }
            compositor::Update::Event(Event::State(snapshot)) => {
                // The theme may have been changed from the dock, which
                // writes it to the file as well.
                self.snapshot = Some(snapshot);
                self.read_file();
            }
            compositor::Update::Event(Event::ConfigChanged) => self.read_file(),
            // For the dock.
            compositor::Update::Event(Event::Brightness { .. }) => {}
            compositor::Update::Event(Event::ToggleLauncher { .. }) => {}
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        match coquille_ipc::socket_from_env() {
            Some(socket) => compositor::subscription(socket).map(Message::Compositor),
            None => Subscription::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        view::window(self)
    }

    pub fn style(&self, _theme: &iced::Theme) -> iced::theme::Style {
        let palette = self.palette();
        let connected = self.connected() || coquille_ipc::socket_from_env().is_some();
        iced::theme::Style {
            background_color: view::backdrop(palette, connected),
            text_color: palette.text,
        }
    }

    /// The built-in look of what is not styled here (scroll bars).
    pub fn iced_theme(&self) -> iced::Theme {
        match self.theme() {
            ThemeName::Light => iced::Theme::Light,
            ThemeName::Dark => iced::Theme::Dark,
        }
    }
}

fn read(options: &Options) -> Settings {
    Settings::parse(&std::fs::read_to_string(&options.config).unwrap_or_default())
}
