//! The dialog of the agent: a panel over everything (layer-shell
//! `Overlay`), which takes the keyboard while it is there. It opens when
//! polkit asks for a password and closes when the request ends; the
//! compositor puts it on the screen of the pointer, or beside a game.

use std::path::PathBuf;

use coquille_ipc::{Command, Event, ThemeName};
use coquille_ui::compositor;
use coquille_ui::style::{Palette, parse_color};
use iced::widget::{operation, space};
use iced::{Color, Element, Subscription, Task, keyboard, window};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption,
};
use iced_layershell::to_layer_message;
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;

use crate::agent::Dialog;
use crate::helper::Password;
use crate::service::{self, Action, Mode, Update};
use crate::view;

/// What the compositor knows the dialog by: it blurs what is behind it,
/// and shows it over a game when games hold every screen.
const NAMESPACE: &str = "coquille-polkit";
/// The password field.
pub const FIELD: &str = "password";

#[derive(Debug, Clone)]
pub struct Options {
    pub mode: Mode,
    /// The configuration file, for the theme until the compositor says it.
    pub config: Option<PathBuf>,
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    Agent(Update),
    Compositor(compositor::Update),
    Window(window::Id, window::Event),
    Key(keyboard::Event),
    Typed(Password),
    Submit,
    Cancel,
    Choose(usize),
}

pub struct Prompt {
    options: Options,
    theme: ThemeName,
    accent: Option<String>,
    agent: Option<UnboundedSender<Action>>,
    /// The link to the compositor, which says the theme.
    link: Option<UnboundedSender<Command>>,
    dialog: Option<Dialog>,
    surface: Option<window::Id>,
    password: Password,
}

impl Prompt {
    pub fn new(options: Options) -> (Self, Task<Message>) {
        let file = options.config.as_ref().map(|path| {
            coquille_config::Settings::parse(&std::fs::read_to_string(path).unwrap_or_default())
        });
        let prompt = Self {
            theme: file.as_ref().map_or(ThemeName::Dark, |file| file.theme()),
            accent: file.and_then(|file| file.accent),
            options,
            agent: None,
            link: None,
            dialog: None,
            surface: None,
            password: Password::default(),
        };
        (prompt, Task::none())
    }

    pub fn palette(&self) -> Palette {
        let accent = self.accent.as_deref().and_then(parse_color);
        Palette::new(self.theme, accent)
    }

    pub fn password(&self) -> &Password {
        &self.password
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Agent(update) => self.agent(update),
            Message::Compositor(compositor::Update::Event(Event::State(snapshot))) => {
                self.theme = snapshot.theme;
                self.accent = Some(snapshot.accent);
                Task::none()
            }
            // Kept, or the link closes: the dialog has nothing to ask.
            Message::Compositor(compositor::Update::Connected(link)) => {
                self.link = Some(link);
                Task::none()
            }
            Message::Compositor(_) => Task::none(),
            Message::Window(id, window::Event::Opened { .. }) if Some(id) == self.surface => {
                operation::focus(FIELD)
            }
            Message::Window(..) => Task::none(),
            Message::Key(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) => self.update(Message::Cancel),
            Message::Key(_) => Task::none(),
            Message::Typed(password) => {
                self.password = password;
                Task::none()
            }
            Message::Submit => {
                if self.dialog.as_ref().is_some_and(|dialog| !dialog.busy()) {
                    let password = std::mem::take(&mut self.password);
                    self.tell(Action::Answer(password));
                }
                Task::none()
            }
            Message::Cancel => {
                self.tell(Action::Dismiss);
                Task::none()
            }
            Message::Choose(user) => {
                self.password = Password::default();
                self.tell(Action::Choose(user));
                operation::focus(FIELD)
            }
            _ => Task::none(),
        }
    }

    fn agent(&mut self, update: Update) -> Task<Message> {
        match update {
            Update::Ready(agent) => {
                self.agent = Some(agent);
                Task::none()
            }
            Update::Show(dialog) => {
                let was_busy = self.dialog.as_ref().is_some_and(Dialog::busy);
                let busy = dialog.busy();
                self.dialog = Some(dialog);
                if self.surface.is_none() {
                    return self.open();
                }
                if was_busy && !busy {
                    // The field was off while the helper checked: it takes
                    // the keyboard back for the next try.
                    return operation::focus(FIELD);
                }
                Task::none()
            }
            Update::Hide => {
                self.dialog = None;
                self.password = Password::default();
                match self.surface.take() {
                    Some(id) => window::close(id),
                    None => Task::none(),
                }
            }
            Update::Stopped => iced::exit(),
        }
    }

    fn open(&mut self) -> Task<Message> {
        debug!("dialog opened");
        self.password = Password::default();
        let (id, task) = Message::layershell_open(NewLayerShellSettings {
            size: None,
            layer: Layer::Overlay,
            anchor: Anchor::Left | Anchor::Right | Anchor::Top | Anchor::Bottom,
            exclusive_zone: Some(-1),
            margin: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            // The compositor chooses: the screen of the pointer, or a side
            // screen while a game holds it.
            output_option: OutputOption::Active,
            events_transparent: false,
            namespace: Some(NAMESPACE.to_owned()),
        });
        self.surface = Some(id);
        task
    }

    fn tell(&self, action: Action) {
        if let Some(agent) = &self.agent {
            let _ = agent.send(action);
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            service::subscription(self.options.mode.clone()).map(Message::Agent),
            window::events().map(|(id, event)| Message::Window(id, event)),
        ];
        if self.dialog.is_some() {
            // Escape gives up even while the field has the keyboard.
            subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                iced::Event::Keyboard(event) => Some(Message::Key(event)),
                _ => None,
            }));
        }
        if let Some(socket) = coquille_ipc::socket_from_env() {
            subscriptions.push(compositor::subscription(socket).map(Message::Compositor));
        }
        Subscription::batch(subscriptions)
    }

    pub fn view(&self, _id: window::Id) -> Element<'_, Message> {
        match &self.dialog {
            Some(dialog) => view::dialog(self, dialog),
            None => space().into(),
        }
    }

    pub fn style(&self, _theme: &iced::Theme) -> iced::theme::Style {
        iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: self.palette().text,
        }
    }
}
