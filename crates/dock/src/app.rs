//! The state of the dock, shared by its surfaces (the bar, its menu, the
//! drawer), and everything that changes it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use coquille_ipc::{Command, Event, Snapshot, ThemeName};
use coquille_model::OutputRole;
use iced::widget::operation;
use iced::{Color, Element, Point, Size, Subscription, Task, keyboard, mouse, window};
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
use crate::config::{self, Config};
use crate::model::bar::{self, Click, Entry, MenuAction};
use crate::model::drawer::{self, Step};
use crate::model::mixer::{self, Volumes};
use crate::model::{Identity, Orientation, identify};
use crate::steam::{self, Game};
use crate::ui;
use crate::ui::icons::Icons;
use crate::ui::style::{Palette, parse_color};
use coquille_apps::{App, Catalog};

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

/// A bar of the dock, on its screen. Each screen may have one, upright on
/// its left edge or lying on its bottom edge.
pub struct BarState {
    pub id: window::Id,
    pub screen: OutputRole,
    pub orientation: Orientation,
    /// What it shows: the same applications on every bar, but "here" is
    /// its own screen.
    pub entries: Vec<Entry>,
    /// The size of its surface.
    size: (u32, u32),
    hovered: bool,
    /// From 0 (folded) to 1 (unfolded): an upright bar only.
    pub unfold: f32,
    pub hover_entry: Option<usize>,
}

impl BarState {
    /// How many of its entries are favorites; they come first.
    pub fn pinned(&self) -> usize {
        self.entries.iter().filter(|entry| entry.pinned).count()
    }
}

/// The launcher, while it is open: a drawer beside an upright bar, a panel
/// above a lying one.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawer {
    pub id: window::Id,
    /// The screen it is on: the one of the bar that opened it.
    pub screen: OutputRole,
    pub orientation: Orientation,
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
    /// How wide the drawer is now (upright).
    pub width: f32,
    /// How high the panel is now (lying).
    pub height: f32,
    /// The user is dragging its edge.
    pub resizing: bool,
    /// The size of the screen it is on, once the surface says.
    pub surface: Option<Size>,
}

impl Drawer {
    /// How many columns the grid the arrow keys walk has.
    pub fn columns(&self) -> usize {
        drawer::columns(self.orientation, !self.query.trim().is_empty())
    }
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
    ResizeTo(Point),
    ResizeEnd,
    Closed(window::Id),
    /// What happens on a bar says which bar.
    BarHover(window::Id, bool),
    EntryHover(window::Id, Option<usize>),
    EntryPress(window::Id, usize),
    EntryRelease(window::Id, usize),
    EntryMiddle(window::Id, usize),
    EntryMenu(window::Id, usize),
    /// Opens or closes the launcher, on this screen if it can be.
    Launcher(Option<OutputRole>),
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
    /// A session action did not go through: why.
    SessionRefused(String),
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
    /// The bars shown, one per screen that has a dock.
    pub bars: Vec<BarState>,
    pub unread: Vec<(String, u32)>,
    pub now: DateTime<Local>,
    pub user: String,

    last_frame: Option<Instant>,
    pub drag: Option<Drag>,
    /// The menu, with the bar it opened from and the key of its
    /// application.
    pub menu: Option<(window::Id, window::Id, String)>,
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
            bars: Vec::new(),
            now: Local::now(),
            user: user_name(),
            last_frame: None,
            drag: None,
            menu: None,
            drawer: None,
            levels: HashMap::new(),
            volumes,
            do_not_disturb: false,
            night_light: false,
            ticks: 0,
        };
        // Looking icons up reads many directories: off the interface.
        let mut names: Vec<String> = dock
            .catalog
            .apps()
            .iter()
            .filter_map(|app| app.icon.clone())
            .collect();
        let favorites = dock.config.favorites.iter();
        names.extend(favorites.filter_map(|name| dock.identify(name).icon));
        let icons = Task::perform(
            async move {
                tokio::task::spawn_blocking(move || Icons::load(names))
                    .await
                    .unwrap_or_default()
            },
            Message::IconsLoaded,
        );
        let mut tasks = vec![icons, dock.show_bars()];
        if dock.options.drawer {
            tasks.push(Task::done(Message::Launcher(None)));
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

    pub fn unread_of(&self, entry: &Entry) -> u32 {
        let named = |name: &String| entry.identity.names.iter().any(|own| own == name);
        let found = self
            .unread
            .iter()
            .find(|(name, _)| named(name) || *name == entry.identity.key);
        found.map_or(0, |(_, count)| *count)
    }

    pub fn bar(&self, id: window::Id) -> Option<&BarState> {
        self.bars.iter().find(|bar| bar.id == id)
    }

    fn bar_mut(&mut self, id: window::Id) -> Option<&mut BarState> {
        self.bars.iter_mut().find(|bar| bar.id == id)
    }

    /// The bar of a screen, when it has one.
    pub fn bar_on(&self, screen: OutputRole) -> Option<&BarState> {
        self.bars.iter().find(|bar| bar.screen == screen)
    }

    /// A screen, as the compositor describes it.
    fn screen(&self, role: OutputRole) -> Option<&coquille_ipc::Screen> {
        self.snapshot
            .screens
            .iter()
            .find(|screen| screen.role == role)
    }

    /// What a bar on `here` shows.
    fn entries_for(&self, here: OutputRole) -> Vec<Entry> {
        let world = bar::World {
            favorites: &self.config.favorites,
            windows: &self.snapshot.windows,
            screens: &self.snapshot.screens,
            habits: &self.snapshot.habits,
            here,
        };
        bar::entries(&world, &|name| self.identify(name))
    }

    fn rebuild_entries(&mut self) {
        let lists: Vec<Vec<Entry>> = self
            .bars
            .iter()
            .map(|bar| self.entries_for(bar.screen))
            .collect();
        for (bar, entries) in self.bars.iter_mut().zip(lists) {
            bar.entries = entries;
        }
    }

    /// Opens the bar of each screen that must have one, once the dock knows
    /// which output that screen is.
    fn show_bars(&mut self) -> Task<Message> {
        let simulated = !self.options.is_real(Backend::Compositor);
        let mut tasks = Vec::new();
        for (index, (screen, orientation)) in self.config.docks.clone().into_iter().enumerate() {
            if self.bar_on(screen).is_some() {
                continue;
            }
            // `--output` names the screen of the first dock; a made-up
            // world has no real output to put the others on.
            let named = self.options.output.clone().filter(|_| index == 0);
            let output = match (named, simulated) {
                (Some(name), _) => OutputOption::OutputName(name),
                (None, true) if index == 0 => OutputOption::Active,
                (None, true) => continue,
                (None, false) => match self.screen(screen) {
                    Some(known) => OutputOption::OutputName(known.name.clone()),
                    // Not known yet: the state of the compositor will tell.
                    None => continue,
                },
            };
            let entries = self.entries_for(screen);
            let pinned = entries.iter().filter(|entry| entry.pinned).count();
            let size = ui::bar::surface_size(orientation, pinned, entries.len() - pinned);
            let anchor = match orientation {
                Orientation::Vertical => Anchor::Left,
                Orientation::Horizontal => Anchor::Bottom,
            };
            let (id, task) = Message::layershell_open(NewLayerShellSettings {
                size: Some(size),
                layer: Layer::Top,
                anchor,
                exclusive_zone: Some(ui::bar::EXCLUSIVE_ZONE),
                margin: None,
                keyboard_interactivity: KeyboardInteractivity::None,
                output_option: output,
                events_transparent: false,
                namespace: Some("coquille-dock".to_owned()),
            });
            self.bars.push(BarState {
                id,
                screen,
                orientation,
                entries,
                size,
                hovered: false,
                unfold: 0.0,
                hover_entry: None,
            });
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    /// Gives each bar the size its entries need, and limits what takes the
    /// pointer to what shows.
    fn fit_bars(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        for index in 0..self.bars.len() {
            let bar = &self.bars[index];
            let (id, pinned) = (bar.id, bar.pinned());
            let size = ui::bar::surface_size(bar.orientation, pinned, bar.entries.len() - pinned);
            let resize = if size == bar.size {
                Task::none()
            } else {
                self.bars[index].size = size;
                Task::done(Message::SizeChange { id, size })
            };
            tasks.push(resize.chain(self.input_region(id)));
        }
        Task::batch(tasks)
    }

    /// A bar takes the pointer where it shows only: the windows beside it
    /// stay clickable.
    fn input_region(&self, id: window::Id) -> Task<Message> {
        let Some(bar) = self.bar(id) else {
            return Task::none();
        };
        let unfolded = bar.hovered || bar.unfold > 0.0;
        let (x, y, width, height) = ui::bar::input_region(bar.orientation, bar.size, unfolded);
        Task::done(Message::SetInputRegion {
            id,
            callback: ActionCallback::new(move |region| region.add(x, y, width, height)),
        })
    }

    /// The surface of the launcher says how large its screen is: the
    /// launcher keeps inside it.
    fn fit_drawer(&mut self, screen: Size) {
        if let Some(drawer) = &mut self.drawer {
            drawer.surface = Some(screen);
        }
        let wanted = self
            .drawer
            .as_ref()
            .map(|drawer| (drawer.width, drawer.height));
        let Some((width, height)) = wanted else {
            return;
        };
        let (width, height) = (
            self.drawer_width(Some(width)) as f32,
            self.drawer_height(Some(height)) as f32,
        );
        if let Some(drawer) = &mut self.drawer {
            (drawer.width, drawer.height) = (width, height);
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

    /// Opens the launcher, or closes it. It opens on the screen asked for
    /// when that screen has a dock and no game, else on the first screen
    /// that does; its shape follows the bar of that screen.
    fn toggle_drawer(&mut self, asked: Option<OutputRole>) -> Task<Message> {
        if self.drawer.is_some() {
            return self.update(Message::CloseDrawer);
        }
        let held = |role| self.screen(role).is_some_and(|screen| screen.anchored);
        let docks: Vec<(OutputRole, bool)> = self
            .bars
            .iter()
            .map(|bar| (bar.screen, held(bar.screen)))
            .collect();
        let picked = bar::launcher_screen(&docks, asked).and_then(|screen| self.bar_on(screen));
        let Some((screen, orientation)) = picked.map(|bar| (bar.screen, bar.orientation)) else {
            return Task::none();
        };
        let first = self.config.docks.first().map(|(first, _)| *first) == Some(screen);
        let named = self.options.output.clone().filter(|_| first);
        let output = match (named, self.screen(screen)) {
            (Some(name), _) => OutputOption::OutputName(name),
            (None, Some(known)) if self.options.is_real(Backend::Compositor) => {
                OutputOption::OutputName(known.name.clone())
            }
            _ => OutputOption::Active,
        };
        let (id, task) = Message::layershell_open(NewLayerShellSettings {
            size: None,
            layer: Layer::Overlay,
            anchor: Anchor::Left | Anchor::Right | Anchor::Top | Anchor::Bottom,
            // Over everything, the bar included: the launcher draws its own.
            exclusive_zone: Some(-1),
            margin: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            output_option: output,
            events_transparent: false,
            namespace: Some("coquille-drawer".to_owned()),
        });
        self.drawer = Some(Drawer {
            id,
            screen,
            orientation,
            query: String::new(),
            page: 0,
            selected: None,
            mixer: false,
            confirm: None,
            note: None,
            width: self.drawer_width(None) as f32,
            height: self.drawer_height(None) as f32,
            resizing: false,
            surface: None,
        });
        self.close_menu().chain(task)
    }

    /// The size of the screen of the launcher: what its surface says, and
    /// before that the compositor's word.
    fn drawer_screen(&self) -> Option<Size> {
        let drawer = self.drawer.as_ref()?;
        let told = || {
            let real = self.options.is_real(Backend::Compositor);
            let screen = self.screen(drawer.screen).filter(|_| real)?;
            Some(Size::new(screen.width as f32, screen.height as f32))
        };
        drawer.surface.or_else(told)
    }

    /// The width of the drawer when `wanted` is asked for; without it, the
    /// width kept in the configuration.
    fn drawer_width(&self, wanted: Option<f32>) -> u32 {
        let kept = self.config.drawer_width.map(|width| width as f32);
        let screen = self.drawer_screen().map(|screen| screen.width);
        drawer::width(wanted.or(kept), screen)
    }

    /// The height of the horizontal launcher, likewise.
    fn drawer_height(&self, wanted: Option<f32>) -> u32 {
        let kept = self.config.drawer_height.map(|height| height as f32);
        let screen = self.drawer_screen().map(|screen| screen.height);
        drawer::height(wanted.or(kept), screen)
    }

    fn close_menu(&mut self) -> Task<Message> {
        match self.menu.take() {
            Some((id, ..)) => window::close(id),
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
                let mut settled = Vec::new();
                for bar in &mut self.bars {
                    let target = unfold_target(bar);
                    if bar.unfold == target {
                        continue;
                    }
                    bar.unfold = if target > bar.unfold {
                        (bar.unfold + step).min(1.0)
                    } else {
                        (bar.unfold - step).max(0.0)
                    };
                    if bar.unfold == target {
                        settled.push(bar.id);
                    }
                }
                if self.bars.iter().all(|bar| bar.unfold == unfold_target(bar)) {
                    self.last_frame = None;
                }
                // Folded again: the windows beside the bar get the pointer
                // back.
                Task::batch(settled.into_iter().map(|id| self.input_region(id)))
            }
            Message::Compositor(update) => self.compositor(update),
            Message::Backend(update) => self.backend(update),
            Message::IconsLoaded(icons) => {
                self.icons = icons;
                Task::none()
            }
            Message::Window(id, window::Event::Opened { size, .. }) => {
                if self.bar(id).is_some() {
                    return self.input_region(id);
                }
                if self.drawer.as_ref().is_some_and(|drawer| drawer.id == id) {
                    self.fit_drawer(size);
                    return operation::focus(SEARCH);
                }
                Task::none()
            }
            Message::Window(id, window::Event::Resized(size)) => {
                if self.drawer.as_ref().is_some_and(|drawer| drawer.id == id) {
                    self.fit_drawer(size);
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
            Message::ResizeTo(pointer) => {
                let Some(drawer) = self.drawer.as_ref().filter(|drawer| drawer.resizing) else {
                    return Task::none();
                };
                match drawer.orientation {
                    // The right edge of the drawer follows the pointer.
                    Orientation::Vertical => {
                        let width = self.drawer_width(Some(pointer.x)) as f32;
                        if let Some(drawer) = &mut self.drawer {
                            drawer.width = width;
                        }
                    }
                    // The top edge of the panel does; its foot stays on
                    // the dock.
                    Orientation::Horizontal => {
                        let screen = self
                            .drawer_screen()
                            .map_or(pointer.y, |screen| screen.height);
                        let foot = screen - ui::bar::EXCLUSIVE_ZONE as f32;
                        let height = self.drawer_height(Some(foot - pointer.y)) as f32;
                        if let Some(drawer) = &mut self.drawer {
                            drawer.height = height;
                        }
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
                let (width, height) = (drawer.width.round() as u32, drawer.height.round() as u32);
                match drawer.orientation {
                    Orientation::Vertical if self.config.drawer_width != Some(width) => {
                        self.config.drawer_width = Some(width);
                        self.save_config(move |text| config::with_drawer_width(text, width));
                    }
                    Orientation::Horizontal if self.config.drawer_height != Some(height) => {
                        self.config.drawer_height = Some(height);
                        self.save_config(move |text| config::with_drawer_height(text, height));
                    }
                    _ => {}
                }
                Task::none()
            }
            Message::Closed(id) => {
                // A click outside the menu closes it without telling more;
                // the bar, kept as it was for it, folds again.
                if let Some((_, bar, _)) = self.menu.take_if(|(menu, ..)| *menu == id) {
                    if let Some(bar) = self.bar_mut(bar) {
                        bar.hovered = false;
                        bar.hover_entry = None;
                    }
                    return self.input_region(bar);
                }
                if self.drawer.as_ref().is_some_and(|drawer| drawer.id == id) {
                    self.drawer = None;
                }
                self.bars.retain(|bar| bar.id != id);
                Task::none()
            }
            Message::BarHover(id, hovered) => {
                // The menu keeps its bar as it is while it is open.
                let has_menu = self.menu.as_ref().is_some_and(|(_, bar, _)| *bar == id);
                if has_menu && !hovered {
                    return Task::none();
                }
                let Some(bar) = self.bar_mut(id) else {
                    return Task::none();
                };
                bar.hovered = hovered;
                if !hovered {
                    bar.hover_entry = None;
                    if let Some(drag) = self.drag.take() {
                        if drag.moved {
                            self.save_favorites();
                        }
                    }
                }
                self.input_region(id)
            }
            Message::EntryHover(id, index) => {
                let Some(bar) = self.bar_mut(id) else {
                    return Task::none();
                };
                bar.hover_entry = index;
                let pinned = bar.pinned();
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
            Message::EntryPress(id, index) => {
                let entry = self.bar(id).and_then(|bar| bar.entries.get(index));
                let pinned = entry.is_some_and(|entry| entry.pinned);
                self.drag = pinned.then_some(Drag {
                    at: index,
                    moved: false,
                });
                self.close_menu()
            }
            Message::EntryRelease(id, index) => {
                if let Some(Drag { moved: true, .. }) = self.drag.take() {
                    self.save_favorites();
                    return Task::none();
                }
                let entry = self.bar(id).and_then(|bar| bar.entries.get(index));
                let Some(entry) = entry.cloned() else {
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
            Message::EntryMiddle(id, index) => {
                let entry = self.bar(id).and_then(|bar| bar.entries.get(index));
                if let Some(command) = entry.and_then(|entry| entry.identity.command.as_ref()) {
                    spawn(command);
                }
                Task::none()
            }
            Message::EntryMenu(id, index) => {
                let Some(bar) = self.bar(id) else {
                    return Task::none();
                };
                let Some(entry) = bar.entries.get(index) else {
                    return Task::none();
                };
                let menu = bar::menu(entry);
                let size = (ui::menu::WIDTH, ui::menu::height(&menu));
                let unfolded = bar.unfold > 0.5;
                let anchor = ui::bar::entry_rect(bar.orientation, index, bar.pinned(), unfolded);
                // Beside an upright bar, above a lying one.
                let (corner, grows) = match bar.orientation {
                    Orientation::Vertical => (PopupAnchor::TopRight, PopupGravity::BottomRight),
                    Orientation::Horizontal => (PopupAnchor::Top, PopupGravity::Top),
                };
                let settings = IcedNewPopupSettings::new(id, size, anchor)
                    .anchor(corner)
                    .gravity(grows);
                let key = entry.identity.key.clone();
                let close = self.close_menu();
                let (popup, open) = Message::popup_open(settings);
                self.menu = Some((popup, id, key));
                self.drag = None;
                close.chain(open)
            }
            Message::Launcher(asked) => self.toggle_drawer(asked),
            Message::Menu(action) => {
                let Some((_, bar, key)) = self.menu.clone() else {
                    return Task::none();
                };
                let entries = self.bar(bar).map(|bar| bar.entries.as_slice());
                let entry = entries
                    .unwrap_or_default()
                    .iter()
                    .find(|entry| entry.identity.key == key);
                let Some(entry) = entry.cloned() else {
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
                if let Some(bar) = self.bar_mut(bar) {
                    bar.hovered = false;
                }
                Task::batch([self.close_menu(), self.fit_bars()])
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
                let Some(command) = action.command() else {
                    // Logging out: the compositor quits, and the session
                    // with it.
                    if self.link.is_none() {
                        let reason = "le compositeur ne répond pas.";
                        return self.update(Message::SessionRefused(action.failure(reason)));
                    }
                    self.send(Command::Quit);
                    return Task::none();
                };
                Task::perform(
                    async move {
                        let run = tokio::process::Command::new(command[0])
                            .args(&command[1..])
                            .output()
                            .await;
                        match run {
                            Ok(output) if output.status.success() => None,
                            Ok(output) => {
                                let said = String::from_utf8_lossy(&output.stderr);
                                Some(action.failure(&said))
                            }
                            Err(error) => Some(action.failure(&error.to_string())),
                        }
                    },
                    |refusal| refusal,
                )
                .and_then(|refusal| Task::done(Message::SessionRefused(refusal)))
            }
            Message::SessionRefused(why) => {
                warn!(why, "session action refused");
                if let Some(drawer) = &mut self.drawer {
                    drawer.note = Some(why);
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
            compositor::Update::Event(Event::ToggleLauncher { screen }) => {
                self.toggle_drawer(screen)
            }
            compositor::Update::Event(Event::State(snapshot)) => {
                self.snapshot = snapshot;
                let show = self.show_bars();
                self.rebuild_entries();
                // A game took the screen of the launcher: the launcher goes.
                let screen = self.drawer.as_ref().map(|drawer| drawer.screen);
                let held = screen.and_then(|role| self.screen(role));
                let close = if held.is_some_and(|screen| screen.anchored) {
                    self.update(Message::CloseDrawer)
                } else {
                    Task::none()
                };
                Task::batch([close, show, self.fit_bars()])
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
            open.selected = drawer::step(open.selected, count, open.columns(), step);
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
                    Some(Message::ResizeTo(position))
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

        if self.bars.iter().any(|bar| bar.unfold != unfold_target(bar)) {
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
        if let Some(bar) = self.bar(id) {
            return ui::bar::view(self, bar);
        }
        if let Some((_, bar, key)) = self.menu.as_ref().filter(|(menu, ..)| *menu == id) {
            let entries = self.bar(*bar).map(|bar| bar.entries.as_slice());
            let mut entries = entries.unwrap_or_default().iter();
            if let Some(entry) = entries.find(|entry| entry.identity.key == *key) {
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

/// How far a bar should be unfolded: an upright bar unfolds under the
/// pointer, a lying one shows a label above the icon instead.
fn unfold_target(bar: &BarState) -> f32 {
    if bar.hovered && bar.orientation == Orientation::Vertical {
        1.0
    } else {
        0.0
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
