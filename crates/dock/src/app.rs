//! The state of the dock, shared by its surfaces (the bar, its menu, the
//! drawer), and everything that changes it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use coquille_ipc::{Command, Event, Snapshot, ThemeName};
use coquille_model::OutputRole;
use iced::widget::operation;
use iced::{Color, Element, Subscription, Task, keyboard, mouse, window};
use iced_layershell::actions::{ActionCallback, IcedNewPopupSettings};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption, PopupAnchor,
    PopupGravity,
};
use iced_layershell::to_layer_message;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::backend::{
    self, Audio, Backend, Request, SessionAction, Switch, compositor, mock, services,
};
use crate::catalog::{App, Catalog};
use crate::config::{self, Config};
use crate::model::bar::{self, Click, Entry, MenuAction};
use crate::model::drawer::{self, Step};
use crate::model::mixer::{self, Volumes};
use crate::model::{Identity, identify};
use crate::steam::{self, Game};
use crate::ui;
use crate::ui::icons::Icons;
use crate::ui::style::{Palette, parse_color};

/// How long the bar takes to unfold under the pointer.
const UNFOLD: Duration = Duration::from_millis(140);
/// The identifier of the search field, to give it the keyboard.
pub const SEARCH: &str = "search";

/// How the dock was started.
#[derive(Debug, Clone)]
pub struct Options {
    /// The backends that are the real ones; the others are simulated.
    pub real: Vec<Backend>,
    /// The output to show on when the compositor does not say.
    pub output: Option<String>,
    pub config: PathBuf,
    /// Where the volumes of applications are kept.
    pub state_dir: PathBuf,
    /// Open the drawer at once (to capture it without a click).
    pub drawer: bool,
}

impl Options {
    pub fn is_real(&self, backend: Backend) -> bool {
        self.real.contains(&backend)
    }
}

/// A favorite being dragged along the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drag {
    /// Where the favorite is now.
    pub at: usize,
    /// It left its place: the release is not a click.
    pub moved: bool,
}

/// The drawer, while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawer {
    pub id: window::Id,
    pub query: String,
    /// The page of the grid of applications, from 0.
    pub page: usize,
    /// The item the arrow keys are on.
    pub selected: Option<usize>,
    pub mixer: bool,
    /// The session action waiting for its confirmation.
    pub confirm: Option<SessionAction>,
    /// What the last session action did, when it was only simulated.
    pub note: Option<String>,
    /// How wide the drawer is now.
    pub width: f32,
    /// The user is dragging its edge.
    pub resizing: bool,
    /// The width of the screen it is on, once the surface says.
    screen: Option<f32>,
}

/// Something of the drawer that can be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// An application, by its key.
    App(String),
    /// A game of the Steam library.
    Game(u64),
}

/// A switch of the control center.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Network,
    Bluetooth,
    DoNotDisturb,
    NightLight,
    DarkTheme,
}

#[to_layer_message(multi)]
#[derive(Debug, Clone)]
pub enum Message {
    /// Once a second: the clock.
    Tick,
    /// While something moves.
    Frame(Instant),
    Compositor(compositor::Update),
    Backend(backend::Update),
    IconsLoaded(Icons),
    Window(window::Id, window::Event),
    /// The edge of the drawer is grabbed, dragged to a place, let go.
    ResizeStart,
    ResizeTo(f32),
    ResizeEnd,
    Closed(window::Id),
    BarHover(bool),
    EntryHover(Option<usize>),
    EntryPress(usize),
    EntryRelease(usize),
    EntryMiddle(usize),
    EntryMenu(usize),
    Launcher,
    Menu(MenuAction),
    Key(keyboard::Event),
    Query(String),
    Submit,
    Page(usize),
    TurnPage(bool),
    Open(Target),
    CloseDrawer,
    ToggleMixer,
    Toggle(Toggle),
    Volume(f32),
    VolumeMute,
    Brightness(f32),
    StreamVolume(String, f32),
    StreamMute(String),
    Session(SessionAction),
    Confirm(bool),
    Settings,
    /// The whole Steam library.
    Library,
}

pub struct Dock {
    pub options: Options,
    pub config: Config,
    pub catalog: Catalog,
    /// The applications of the drawer, from A to Z.
    pub apps: Vec<App>,
    pub games: Vec<Game>,
    pub icons: Icons,
    pub snapshot: Snapshot,
    link: Option<UnboundedSender<Command>>,
    links: HashMap<Backend, UnboundedSender<Request>>,
    /// The screen the dock lives on.
    pub here: OutputRole,
    pub entries: Vec<Entry>,
    pub unread: Vec<(String, u32)>,
    pub now: DateTime<Local>,
    pub user: String,

    pub bar: Option<window::Id>,
    bar_height: u32,
    hovered: bool,
    /// From 0 (folded) to 1 (unfolded).
    pub unfold: f32,
    last_frame: Option<Instant>,
    pub hover_entry: Option<usize>,
    pub drag: Option<Drag>,
    /// The menu, with the key of its application.
    pub menu: Option<(window::Id, String)>,
    pub drawer: Option<Drawer>,

    pub audio: Audio,
    pub levels: HashMap<u32, f32>,
    volumes: Volumes,
    pub network: Switch,
    pub bluetooth: Switch,
    pub brightness: Option<f32>,
    pub do_not_disturb: bool,
    pub night_light: bool,
    ticks: u64,
}

impl Dock {
    pub fn new(options: Options) -> (Self, Task<Message>) {
        let mut config = config::load(&options.config);
        let catalog = Catalog::load();
        let games = steam::load();
        // A favorite written twice under two names would show once and
        // break the count: keep the first name of each application.
        let mut seen = Vec::new();
        config.favorites.retain(|name| {
            let key = identify(&catalog, &games, name).key;
            let new = !seen.contains(&key);
            seen.push(key);
            new
        });

        // Steam writes a desktop file for each game: those are listed with
        // the games, not among the applications.
        let mut apps = catalog.apps().to_vec();
        apps.retain(|app| steam::game_of_command(&app.command).is_none());
        drawer::sort_by_name(&mut apps, |app| &app.name);
        let theme = config.theme.unwrap_or_default();
        let simulated = |backend| !options.is_real(backend);
        let snapshot = if simulated(Backend::Compositor) {
            let mut snapshot = mock::snapshot(theme, &config.favorites);
            snapshot.accent = config.accent.clone().unwrap_or_default();
            snapshot
        } else {
            Snapshot::default()
        };
        let volumes = Volumes::load(&options.state_dir.join("volumes.json"));

        let mut dock = Self {
            audio: if simulated(Backend::Audio) {
                mock::audio(&config.favorites)
            } else {
                Audio::default()
            },
            network: if simulated(Backend::Network) {
                mock::network()
            } else {
                Switch::default()
            },
            bluetooth: if simulated(Backend::Bluetooth) {
                mock::bluetooth()
            } else {
                Switch::default()
            },
            brightness: simulated(Backend::Brightness).then_some(0.7),
            unread: if simulated(Backend::Compositor) {
                mock::unread(&config.favorites)
            } else {
                Vec::new()
            },
            options,
            config,
            catalog,
            apps,
            games,
            icons: Icons::default(),
            snapshot,
            link: None,
            links: HashMap::new(),
            here: OutputRole::Left,
            entries: Vec::new(),
            now: Local::now(),
            user: user_name(),
            bar: None,
            bar_height: 0,
            hovered: false,
            unfold: 0.0,
            last_frame: None,
            hover_entry: None,
            drag: None,
            menu: None,
            drawer: None,
            levels: HashMap::new(),
            volumes,
            do_not_disturb: false,
            night_light: false,
            ticks: 0,
        };
        dock.rebuild_entries();

        // Looking icons up reads many directories: off the interface.
        let mut names: Vec<String> = dock
            .apps
            .iter()
            .filter_map(|app| app.icon.clone())
            .collect();
        names.extend(
            dock.entries
                .iter()
                .filter_map(|entry| entry.identity.icon.clone()),
        );
        let icons = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || Icons::load(names))
                    .await
                    .unwrap_or_default()
            },
            Message::IconsLoaded,
        );
        let mut tasks = vec![icons, dock.show_bar()];
        if dock.options.drawer {
            tasks.push(Task::done(Message::Launcher));
        }
        (dock, Task::batch(tasks))
    }

    pub fn palette(&self) -> Palette {
        let accent = parse_color(&self.snapshot.accent);
        Palette::new(self.snapshot.theme, accent)
    }

    pub fn identify(&self, name: &str) -> Identity {
        identify(&self.catalog, &self.games, name)
    }

    /// How many entries of the bar are favorites; they come first.
    pub fn pinned(&self) -> usize {
        self.entries.iter().filter(|entry| entry.pinned).count()
    }

    pub fn unread_of(&self, entry: &Entry) -> u32 {
        let named = |name: &String| entry.identity.names.iter().any(|own| own == name);
        let found = self
            .unread
            .iter()
            .find(|(name, _)| named(name) || *name == entry.identity.key);
        found.map_or(0, |(_, count)| *count)
    }

    /// The screen of the dock, as the compositor describes it.
    fn screen(&self) -> Option<&coquille_ipc::Screen> {
        self.snapshot
            .screens
            .iter()
            .find(|screen| screen.role == self.here)
    }

    fn rebuild_entries(&mut self) {
        let world = bar::World {
            favorites: &self.config.favorites,
            windows: &self.snapshot.windows,
            screens: &self.snapshot.screens,
            habits: &self.snapshot.habits,
            here: self.here,
        };
        let (catalog, games) = (&self.catalog, &self.games);
        self.entries = bar::entries(&world, &|name| identify(catalog, games, name));
    }

    /// Opens the bar once the dock knows which output is its screen.
    fn show_bar(&mut self) -> Task<Message> {
        if self.bar.is_some() {
            return Task::none();
        }
        let simulated = !self.options.is_real(Backend::Compositor);
        let named = self.options.output.clone();
        let from_compositor = || self.screen().map(|screen| screen.name.clone());
        let output = match (named, simulated) {
            (Some(name), _) => OutputOption::OutputName(name),
            (None, true) => OutputOption::Active,
            (None, false) => match from_compositor() {
                Some(name) => OutputOption::OutputName(name),
                // Not known yet: the state of the compositor will tell.
                None => return Task::none(),
            },
        };
        let height = self.wanted_bar_height();
        self.bar_height = height;
        let (id, task) = Message::layershell_open(NewLayerShellSettings {
            size: Some((ui::bar::SURFACE_WIDTH, height)),
            layer: Layer::Top,
            anchor: Anchor::Left,
            exclusive_zone: Some(ui::bar::EXCLUSIVE_ZONE),
            margin: None,
            keyboard_interactivity: KeyboardInteractivity::None,
            output_option: output,
            events_transparent: false,
            namespace: Some("coquille-dock".to_owned()),
        });
        self.bar = Some(id);
        task
    }

    fn wanted_bar_height(&self) -> u32 {
        let pinned = self.pinned();
        ui::bar::surface_height(pinned, self.entries.len() - pinned)
    }

    /// Gives the bar the height its entries need, and limits what takes
    /// the pointer to what shows.
    fn fit_bar(&mut self) -> Task<Message> {
        let Some(id) = self.bar else {
            return Task::none();
        };
        let height = self.wanted_bar_height();
        let resize = if height == self.bar_height {
            Task::none()
        } else {
            self.bar_height = height;
            Task::done(Message::SizeChange {
                id,
                size: (ui::bar::SURFACE_WIDTH, height),
            })
        };
        resize.chain(self.input_region())
    }

    /// The bar takes the pointer where it shows only: folded, the windows
    /// beside it stay clickable.
    fn input_region(&self) -> Task<Message> {
        let Some(id) = self.bar else {
            return Task::none();
        };
        let width = if self.hovered || self.unfold > 0.0 {
            ui::bar::UNFOLDED_REACH
        } else {
            ui::bar::EXCLUSIVE_ZONE.unsigned_abs()
        };
        let (width, height) = (width as i32, self.bar_height as i32);
        Task::done(Message::SetInputRegion {
            id,
            callback: ActionCallback::new(move |region| region.add(0, 0, width, height)),
        })
    }

    /// The surface of the drawer says how wide its screen is: the drawer
    /// keeps inside it.
    fn fit_drawer(&mut self, screen: f32) {
        if let Some(drawer) = &mut self.drawer {
            drawer.screen = Some(screen);
        }
        let wanted = self.drawer.as_ref().map(|drawer| drawer.width);
        let width = self.drawer_width(wanted) as f32;
        if let Some(drawer) = &mut self.drawer {
            drawer.width = width;
        }
    }

    /// Asks the compositor for something; simulated, the made-up world
    /// follows as well as it can.
    fn send(&mut self, command: Command) {
        if let Some(link) = &self.link {
            if link.send(command).is_err() {
                self.link = None;
            }
            return;
        }
        if self.options.is_real(Backend::Compositor) {
            warn!(?command, "the compositor is not there: command dropped");
            return;
        }
        debug!(?command, "simulated");
        let windows = &mut self.snapshot.windows;
        match command {
            Command::Activate { window } => {
                if let Some(index) = windows.iter().position(|known| known.id == window) {
                    let mut shown = windows.remove(index);
                    shown.minimized = false;
                    windows.insert(0, shown);
                    for (rank, known) in windows.iter_mut().enumerate() {
                        known.active = rank == 0;
                    }
                }
            }
            Command::Close { window } => windows.retain(|known| known.id != window),
            Command::Forget { app } => self.snapshot.habits.retain(|habit| habit.app != app),
            Command::SetTheme { theme } => self.snapshot.theme = theme,
            Command::OpenOn { .. } | Command::ToggleLauncher | Command::Quit => {}
        }
        self.rebuild_entries();
    }

    /// Asks a service for something; simulated, it is done at once.
    fn request(&mut self, request: Request) {
        if let Some(link) = self.links.get(&request.backend()) {
            let _ = link.send(request.clone());
        }
        // The interface does not wait for the service to answer.
        let streams = &mut self.audio.streams;
        match request {
            Request::Volume(volume) => self.audio.volume = volume,
            Request::Mute(muted) => self.audio.muted = muted,
            Request::StreamVolume(ids, volume) => {
                for stream in streams.iter_mut().filter(|stream| ids.contains(&stream.id)) {
                    stream.volume = volume;
                }
            }
            Request::StreamMute(ids, muted) => {
                for stream in streams.iter_mut().filter(|stream| ids.contains(&stream.id)) {
                    stream.muted = muted;
                }
            }
            Request::Levels(_) => {}
            Request::Network(on) => self.network.on = on,
            Request::Bluetooth(on) => self.bluetooth.on = on,
            Request::Brightness(level) => self.brightness = Some(level),
        }
    }

    /// The lines of the mixer.
    pub fn mixer_lines(&self) -> Vec<mixer::Line> {
        mixer::lines(&self.audio.streams, &|name| self.identify(name))
    }

    fn save_volumes(&self) {
        let path = self.options.state_dir.join("volumes.json");
        if let Err(error) = self.volumes.save(&path) {
            warn!(%error, "cannot save the volumes of applications");
        }
    }

    fn save_config(&self, change: impl Fn(&str) -> Option<String>) {
        if let Err(error) = config::update(&self.options.config, change) {
            warn!(%error, path = %self.options.config.display(), "cannot write the configuration");
        }
    }

    fn save_favorites(&self) {
        let favorites = self.config.favorites.clone();
        self.save_config(move |text| config::with_favorites(text, &favorites));
    }

    fn toggle_drawer(&mut self) -> Task<Message> {
        if let Some(drawer) = self.drawer.take() {
            return window::close(drawer.id);
        }
        // No panel on a screen held by a game.
        if self.screen().is_some_and(|screen| screen.anchored) {
            return Task::none();
        }
        let output = match (&self.options.output, self.screen()) {
            (Some(name), _) => OutputOption::OutputName(name.clone()),
            (None, Some(screen)) if self.options.is_real(Backend::Compositor) => {
                OutputOption::OutputName(screen.name.clone())
            }
            _ => OutputOption::Active,
        };
        let (id, task) = Message::layershell_open(NewLayerShellSettings {
            size: None,
            layer: Layer::Overlay,
            anchor: Anchor::Left | Anchor::Right | Anchor::Top | Anchor::Bottom,
            // Over everything, the bar included: the drawer draws its own.
            exclusive_zone: Some(-1),
            margin: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            output_option: output,
            events_transparent: false,
            namespace: Some("coquille-drawer".to_owned()),
        });
        self.drawer = Some(Drawer {
            id,
            query: String::new(),
            page: 0,
            selected: None,
            mixer: false,
            confirm: None,
            note: None,
            width: self.drawer_width(None) as f32,
            resizing: false,
            screen: None,
        });
        self.close_menu().chain(task)
    }

    /// The width of the drawer when `wanted` is asked for; without it, the
    /// width kept in the configuration.
    fn drawer_width(&self, wanted: Option<f32>) -> u32 {
        let kept = self.config.drawer_width.map(|width| width as f32);
        // The surface of the drawer covers its screen: its width is the
        // most reliable one. Before it is known, the compositor's word.
        let surface = self.drawer.as_ref().and_then(|drawer| drawer.screen);
        let told = || {
            let real = self.options.is_real(Backend::Compositor);
            self.screen()
                .filter(|_| real)
                .map(|screen| screen.width as f32)
        };
        drawer::width(wanted.or(kept), surface.or_else(told))
    }

    fn close_menu(&mut self) -> Task<Message> {
        match self.menu.take() {
            Some((id, _)) => window::close(id),
            None => Task::none(),
        }
    }

    /// What the drawer lists for the search in progress: applications,
    /// then games.
    pub fn hits(&self, query: &str) -> Vec<(Target, Identity)> {
        let apps = drawer::search(&self.apps, |app| &app.name, query);
        let games = drawer::search(&self.games, |game| &game.name, query);
        let apps = apps
            .into_iter()
            .map(|app| (Target::App(app.id.clone()), self.identify(&app.id)));
        let games = games
            .into_iter()
            .map(|game| (Target::Game(game.id), self.identify(&game.window_class())));
        apps.chain(games).collect()
    }

    /// Starts what was picked in the drawer, where its label says.
    fn open(&mut self, target: &Target) {
        let identity = match target {
            Target::App(key) => self.identify(key),
            Target::Game(id) => self.identify(&format!("steam_app_{id}")),
        };
        if identity.game {
            // Belt and braces: the compositor sends games to the center
            // by itself.
            self.send(Command::OpenOn {
                apps: identity.names.clone(),
                screen: OutputRole::Center,
            });
        }
        match &identity.command {
            Some(command) => spawn(command),
            None => warn!(name = identity.name, "no command known to start this"),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.now = Local::now();
                self.ticks += 1;
                if !self.options.is_real(Backend::Audio) {
                    self.levels = mock::levels(&self.audio, self.ticks).into_iter().collect();
                }
                Task::none()
            }
            Message::Frame(now) => {
                let elapsed = self
                    .last_frame
                    .replace(now)
                    .map_or(Duration::ZERO, |last| now - last);
                let step = elapsed.as_secs_f32() / UNFOLD.as_secs_f32();
                let target = if self.hovered { 1.0 } else { 0.0 };
                let before = self.unfold;
                self.unfold = if target > before {
                    (before + step).min(1.0)
                } else {
                    (before - step).max(0.0)
                };
                if self.unfold == target {
                    self.last_frame = None;
                    // Folded again: the windows beside the bar get the
                    // pointer back.
                    return self.input_region();
                }
                Task::none()
            }
            Message::Compositor(update) => self.compositor(update),
            Message::Backend(update) => self.backend(update),
            Message::IconsLoaded(icons) => {
                self.icons = icons;
                Task::none()
            }
            Message::Window(id, window::Event::Opened { size, .. }) => {
                if self.bar == Some(id) {
                    return self.input_region();
                }
                if self.drawer.as_ref().is_some_and(|drawer| drawer.id == id) {
                    self.fit_drawer(size.width);
                    return operation::focus(SEARCH);
                }
                Task::none()
            }
            Message::Window(id, window::Event::Resized(size)) => {
                if self.drawer.as_ref().is_some_and(|drawer| drawer.id == id) {
                    self.fit_drawer(size.width);
                }
                Task::none()
            }
            Message::Window(_, _) => Task::none(),
            Message::ResizeStart => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.resizing = true;
                }
                Task::none()
            }
            Message::ResizeTo(edge) => {
                if self.drawer.as_ref().is_some_and(|drawer| drawer.resizing) {
                    let width = self.drawer_width(Some(edge)) as f32;
                    if let Some(drawer) = &mut self.drawer {
                        drawer.width = width;
                    }
                }
                Task::none()
            }
            Message::ResizeEnd => {
                let Some(drawer) = self.drawer.as_mut().filter(|drawer| drawer.resizing) else {
                    return Task::none();
                };
                drawer.resizing = false;
                // Kept for the next time, here and in the configuration.
                let width = drawer.width.round() as u32;
                if self.config.drawer_width != Some(width) {
                    self.config.drawer_width = Some(width);
                    self.save_config(move |text| config::with_drawer_width(text, width));
                }
                Task::none()
            }
            Message::Closed(id) => {
                // A click outside the menu closes it without telling more;
                // the bar, kept unfolded for it, folds again.
                if self.menu.as_ref().is_some_and(|(menu, _)| *menu == id) {
                    self.menu = None;
                    self.hovered = false;
                    self.hover_entry = None;
                    return self.input_region();
                }
                if self.drawer.as_ref().is_some_and(|drawer| drawer.id == id) {
                    self.drawer = None;
                }
                if self.bar == Some(id) {
                    self.bar = None;
                }
                Task::none()
            }
            Message::BarHover(hovered) => {
                // The menu keeps the bar as it is while it is open.
                if self.menu.is_some() && !hovered {
                    return Task::none();
                }
                self.hovered = hovered;
                if !hovered {
                    self.hover_entry = None;
                    if let Some(drag) = self.drag.take() {
                        if drag.moved {
                            self.save_favorites();
                        }
                    }
                }
                self.input_region()
            }
            Message::EntryHover(index) => {
                self.hover_entry = index;
                let pinned = self.pinned();
                if let (Some(drag), Some(to)) = (self.drag.as_mut(), index) {
                    if to < pinned && to != drag.at {
                        bar::reorder(&mut self.config.favorites, drag.at, to);
                        drag.at = to;
                        drag.moved = true;
                        self.rebuild_entries();
                    }
                }
                Task::none()
            }
            Message::EntryPress(index) => {
                let pinned = self.entries.get(index).is_some_and(|entry| entry.pinned);
                self.drag = pinned.then_some(Drag {
                    at: index,
                    moved: false,
                });
                self.close_menu()
            }
            Message::EntryRelease(index) => {
                if let Some(Drag { moved: true, .. }) = self.drag.take() {
                    self.save_favorites();
                    return Task::none();
                }
                let Some(entry) = self.entries.get(index).cloned() else {
                    return Task::none();
                };
                match bar::left_click(&entry) {
                    Click::Activate(window) => self.send(Command::Activate { window }),
                    Click::Launch => match &entry.identity.command {
                        Some(command) => spawn(command),
                        None => warn!(name = entry.identity.name, "no command to start this"),
                    },
                }
                Task::none()
            }
            Message::EntryMiddle(index) => {
                if let Some(command) = self
                    .entries
                    .get(index)
                    .and_then(|entry| entry.identity.command.as_ref())
                {
                    spawn(command);
                }
                Task::none()
            }
            Message::EntryMenu(index) => {
                let (Some(bar), Some(entry)) = (self.bar, self.entries.get(index)) else {
                    return Task::none();
                };
                let menu = bar::menu(entry);
                let size = (ui::menu::WIDTH, ui::menu::height(&menu));
                let anchor = ui::bar::entry_rect(index, self.pinned(), self.unfold > 0.5);
                let settings = IcedNewPopupSettings::new(bar, size, anchor)
                    .anchor(PopupAnchor::TopRight)
                    .gravity(PopupGravity::BottomRight);
                let key = entry.identity.key.clone();
                let close = self.close_menu();
                let (id, open) = Message::popup_open(settings);
                self.menu = Some((id, key));
                self.drag = None;
                close.chain(open)
            }
            Message::Launcher => self.toggle_drawer(),
            Message::Menu(action) => {
                let Some((_, key)) = &self.menu else {
                    return Task::none();
                };
                let Some(entry) = self
                    .entries
                    .iter()
                    .find(|entry| entry.identity.key == *key)
                    .cloned()
                else {
                    return self.close_menu();
                };
                match action {
                    MenuAction::OpenOn(screen) => {
                        self.send(Command::OpenOn {
                            apps: entry.identity.names.clone(),
                            screen,
                        });
                        if let Some(command) = &entry.identity.command {
                            spawn(command);
                        }
                    }
                    MenuAction::NewWindow => {
                        if let Some(command) = &entry.identity.command {
                            spawn(command);
                        }
                    }
                    MenuAction::Pin => {
                        bar::pin(&mut self.config.favorites, &entry);
                        self.save_favorites();
                    }
                    MenuAction::Unpin => {
                        let (catalog, games) = (&self.catalog, &self.games);
                        let identify = |name: &str| identify(catalog, games, name);
                        bar::unpin(&mut self.config.favorites, &entry, &identify);
                        self.save_favorites();
                    }
                    MenuAction::Forget => {
                        if let Some(habit) = &entry.habit {
                            self.send(Command::Forget {
                                app: habit.app.clone(),
                            });
                        }
                    }
                    MenuAction::Close(window) => self.send(Command::Close { window }),
                }
                self.rebuild_entries();
                self.hovered = false;
                Task::batch([self.close_menu(), self.fit_bar()])
            }
            Message::Key(event) => self.key(event),
            Message::Query(query) => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.query = query;
                    drawer.selected = None;
                    drawer.page = 0;
                }
                Task::none()
            }
            Message::Submit => {
                let Some(drawer) = &self.drawer else {
                    return Task::none();
                };
                let target = if drawer.query.trim().is_empty() {
                    let apps = drawer::page(&self.apps, drawer.page);
                    drawer
                        .selected
                        .and_then(|index| apps.get(index))
                        .map(|app| Target::App(app.id.clone()))
                } else {
                    let hits = self.hits(&drawer.query);
                    hits.get(drawer.selected.unwrap_or(0))
                        .map(|(target, _)| target.clone())
                };
                match target {
                    Some(target) => self.update(Message::Open(target)),
                    None => Task::none(),
                }
            }
            Message::Page(page) => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.page = page.min(drawer::page_count(self.apps.len()) - 1);
                    drawer.selected = None;
                }
                Task::none()
            }
            Message::TurnPage(forward) => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.page = drawer::turn_page(drawer.page, self.apps.len(), forward);
                    drawer.selected = None;
                }
                Task::none()
            }
            Message::Open(target) => {
                self.open(&target);
                self.update(Message::CloseDrawer)
            }
            Message::CloseDrawer => match self.drawer.take() {
                Some(drawer) => {
                    self.request(Request::Levels(false));
                    window::close(drawer.id)
                }
                None => Task::none(),
            },
            Message::ToggleMixer => {
                let Some(drawer) = &mut self.drawer else {
                    return Task::none();
                };
                drawer.mixer = !drawer.mixer;
                let shown = drawer.mixer;
                self.request(Request::Levels(shown));
                Task::none()
            }
            Message::Toggle(toggle) => {
                match toggle {
                    Toggle::Network => self.request(Request::Network(!self.network.on)),
                    Toggle::Bluetooth => self.request(Request::Bluetooth(!self.bluetooth.on)),
                    // Kept for when there are notifications and a night
                    // light: for now they only remember their position.
                    Toggle::DoNotDisturb => self.do_not_disturb = !self.do_not_disturb,
                    Toggle::NightLight => self.night_light = !self.night_light,
                    Toggle::DarkTheme => {
                        let theme = match self.snapshot.theme {
                            ThemeName::Dark => ThemeName::Light,
                            ThemeName::Light => ThemeName::Dark,
                        };
                        self.send(Command::SetTheme { theme });
                        self.save_config(move |text| config::with_theme(text, theme));
                    }
                }
                Task::none()
            }
            Message::Volume(volume) => {
                self.request(Request::Volume(mixer::clamp(volume)));
                if self.audio.muted && volume > 0.0 {
                    self.request(Request::Mute(false));
                }
                Task::none()
            }
            Message::VolumeMute => {
                self.request(Request::Mute(!self.audio.muted));
                Task::none()
            }
            Message::Brightness(level) => {
                self.request(Request::Brightness(level.clamp(0.05, 1.0)));
                Task::none()
            }
            Message::StreamVolume(key, volume) => {
                let volume = mixer::clamp(volume);
                let line = self
                    .mixer_lines()
                    .into_iter()
                    .find(|line| line.identity.key == key);
                if let Some(line) = line {
                    self.request(Request::StreamVolume(line.streams, volume));
                    if self.volumes.remember(&key, volume, line.muted) {
                        self.save_volumes();
                    }
                }
                Task::none()
            }
            Message::StreamMute(key) => {
                let line = self
                    .mixer_lines()
                    .into_iter()
                    .find(|line| line.identity.key == key);
                if let Some(line) = line {
                    self.request(Request::StreamMute(line.streams, !line.muted));
                    if self.volumes.remember(&key, line.volume, !line.muted) {
                        self.save_volumes();
                    }
                }
                Task::none()
            }
            Message::Session(action) => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.confirm = Some(action);
                    drawer.note = None;
                }
                Task::none()
            }
            Message::Confirm(confirmed) => {
                let action = self
                    .drawer
                    .as_mut()
                    .and_then(|drawer| drawer.confirm.take());
                let Some(action) = action.filter(|_| confirmed) else {
                    return Task::none();
                };
                if !self.options.is_real(Backend::Session) {
                    info!(?action, "session action simulated: nothing was done");
                    if let Some(drawer) = &mut self.drawer {
                        drawer.note = Some(format!(
                            "« {} » : simulé, rien n'a été fait.",
                            action.verb()
                        ));
                    }
                    return Task::none();
                }
                info!(?action, "session action");
                match action {
                    SessionAction::Logout => self.send(Command::Quit),
                    SessionAction::Restart => spawn("systemctl reboot"),
                    SessionAction::PowerOff => spawn("systemctl poweroff"),
                }
                Task::none()
            }
            Message::Library => {
                spawn("steam steam://open/games");
                self.update(Message::CloseDrawer)
            }
            Message::Settings => {
                if let Some(command) = self.identify("org.gnome.Settings").command {
                    spawn(&command);
                }
                self.update(Message::CloseDrawer)
            }
            // The messages added for the layer shell are handled by it.
            _ => Task::none(),
        }
    }

    fn compositor(&mut self, update: compositor::Update) -> Task<Message> {
        match update {
            compositor::Update::Connected(link) => {
                self.link = Some(link);
                Task::none()
            }
            compositor::Update::Disconnected => {
                self.link = None;
                Task::none()
            }
            compositor::Update::Event(Event::ToggleLauncher) => self.toggle_drawer(),
            compositor::Update::Event(Event::State(snapshot)) => {
                self.snapshot = snapshot;
                self.rebuild_entries();
                // A game took the screen of the dock: the drawer goes.
                let anchored = self.screen().is_some_and(|screen| screen.anchored);
                let close = match self.drawer.take_if(|_| anchored) {
                    Some(drawer) => window::close(drawer.id),
                    None => Task::none(),
                };
                Task::batch([close, self.show_bar(), self.fit_bar()])
            }
        }
    }

    fn backend(&mut self, update: backend::Update) -> Task<Message> {
        match update {
            backend::Update::Link(backend, link) => {
                self.links.insert(backend, link);
            }
            backend::Update::Audio(audio) => {
                // An application that starts playing gets back the volume
                // it had last time.
                let known: Vec<u32> = self.audio.streams.iter().map(|stream| stream.id).collect();
                let mut restored = Vec::new();
                for stream in audio
                    .streams
                    .iter()
                    .filter(|stream| !known.contains(&stream.id))
                {
                    let key = self.identify(&stream.app).key;
                    if let Some(saved) = self.volumes.restore(&key, stream) {
                        restored.push((stream.id, saved));
                    }
                }
                self.audio = audio;
                for (id, saved) in restored {
                    self.request(Request::StreamVolume(vec![id], saved.volume));
                    self.request(Request::StreamMute(vec![id], saved.muted));
                }
            }
            backend::Update::Levels(levels) => self.levels = levels.into_iter().collect(),
            backend::Update::Network(switch) => self.network = switch,
            backend::Update::Bluetooth(switch) => self.bluetooth = switch,
            backend::Update::Brightness(level) => self.brightness = level,
        }
        Task::none()
    }

    fn key(&mut self, event: keyboard::Event) -> Task<Message> {
        use keyboard::key::Named;
        let keyboard::Event::KeyPressed { key, .. } = event else {
            return Task::none();
        };
        let keyboard::Key::Named(named) = key else {
            return Task::none();
        };
        let Some(open) = &self.drawer else {
            return Task::none();
        };
        let step = match named {
            Named::Escape if open.confirm.is_some() => return self.update(Message::Confirm(false)),
            Named::Escape => return self.update(Message::CloseDrawer),
            Named::PageDown => return self.update(Message::TurnPage(true)),
            Named::PageUp => return self.update(Message::TurnPage(false)),
            Named::ArrowLeft => Step::Left,
            Named::ArrowRight => Step::Right,
            Named::ArrowUp => Step::Up,
            Named::ArrowDown => Step::Down,
            _ => return Task::none(),
        };
        let count = if open.query.trim().is_empty() {
            drawer::page(&self.apps, open.page).len()
        } else {
            self.hits(&open.query).len()
        };
        if let Some(open) = &mut self.drawer {
            open.selected = drawer::step(open.selected, count, drawer::COLUMNS, step);
        }
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::events().map(|(id, event)| Message::Window(id, event)),
            window::close_events().map(Message::Closed),
        ];
        // While the edge of the drawer is held, the pointer drags it
        // wherever it goes on the screen.
        if self.drawer.as_ref().is_some_and(|drawer| drawer.resizing) {
            subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                    Some(Message::ResizeTo(position.x))
                }
                iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    Some(Message::ResizeEnd)
                }
                _ => None,
            }));
        }
        // The clock; with the mixer open, the made-up levels move faster.
        let simulated_levels = !self.options.is_real(Backend::Audio)
            && self.drawer.as_ref().is_some_and(|drawer| drawer.mixer);
        let pace = if simulated_levels {
            Duration::from_millis(120)
        } else {
            Duration::from_secs(1)
        };
        subscriptions.push(iced::time::every(pace).map(|_| Message::Tick));

        let target = if self.hovered { 1.0 } else { 0.0 };
        if self.unfold != target {
            subscriptions.push(window::frames().map(Message::Frame));
        }
        if self.drawer.is_some() {
            // Every key, the ones the search field used too: Escape closes
            // the drawer even while typing.
            subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                iced::Event::Keyboard(event) => Some(Message::Key(event)),
                _ => None,
            }));
        }
        if self.options.is_real(Backend::Compositor) {
            if let Some(socket) = coquille_ipc::socket_from_env() {
                let link = compositor::subscription(socket).map(Message::Compositor);
                subscriptions.push(link);
            }
        }
        if self.options.is_real(Backend::Audio) {
            subscriptions.push(backend::audio::subscription().map(Message::Backend));
        }
        if self.options.is_real(Backend::Network) {
            subscriptions.push(services::network_subscription().map(Message::Backend));
        }
        if self.options.is_real(Backend::Bluetooth) {
            subscriptions.push(services::bluetooth_subscription().map(Message::Backend));
        }
        if self.options.is_real(Backend::Brightness) {
            subscriptions.push(services::brightness_subscription().map(Message::Backend));
        }
        Subscription::batch(subscriptions)
    }

    pub fn view(&self, id: window::Id) -> Element<'_, Message> {
        if self.bar == Some(id) {
            return ui::bar::view(self);
        }
        if let Some((_, key)) = self.menu.as_ref().filter(|(menu, _)| *menu == id) {
            if let Some(entry) = self.entries.iter().find(|entry| entry.identity.key == *key) {
                return ui::menu::view(self, entry);
            }
        }
        if let Some(drawer) = self.drawer.as_ref().filter(|drawer| drawer.id == id) {
            return ui::drawer::view(self, drawer);
        }
        iced::widget::space().into()
    }

    pub fn style(&self, _theme: &iced::Theme) -> iced::theme::Style {
        iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: self.palette().text,
        }
    }
}

/// Starts a program of the user, detached from the dock.
pub fn spawn(command: &str) {
    info!(command, "starting");
    let detached = format!("{command} >/dev/null 2>&1 &");
    let started = std::process::Command::new("sh")
        .arg("-c")
        .arg(detached)
        .status();
    if let Err(error) = started {
        warn!(command, %error, "cannot start");
    }
}

/// The name of the user, as the system has it: the full one, else the login.
fn user_name() -> String {
    let login = std::env::var("USER").unwrap_or_default();
    let accounts = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let full = accounts.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let full = fields.nth(3)?.split(',').next()?;
        (name == login && !full.is_empty()).then(|| full.to_owned())
    });
    full.unwrap_or(login)
}
