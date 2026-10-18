//! The state of the dock, shared by its surfaces (the bar, its menu, the
//! drawer), and everything that changes it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use coquille_ipc::{Command, Event, ScreenshotTarget, Snapshot, ThemeName};
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
use crate::model::notify::{
    CloseReason, Notice, NotifyState, ToastAnchor, ToastGeometry, ToastSurfacePlan, toast_geometry,
    toast_surface_plan, toasts_that_fit,
};
use crate::model::tray::TrayItem;
use crate::model::{Identity, Orientation, Side, identify};
use crate::steam::{self, Game};
use crate::ui;
use crate::ui::icons::{self, IconStyle, Icons};
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
    /// Open the mixer at once (to capture it without a click).
    pub mixer: bool,
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
/// its left or its right edge, or lying on its bottom edge.
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
    pub hover_tray: Option<usize>,
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
    /// The screenshot selector is open in the drawer footer.
    pub screenshot_prompt: bool,
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

/// Overlay of the toasts on screen, in the bottom corner on one bar's side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToastSurface {
    pub id: window::Id,
    pub screen: OutputRole,
    pub orientation: Orientation,
    pub geometry: ToastGeometry,
    /// How many capsules it shows: the newest that fit.
    pub shown: usize,
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
    /// What happens on the system tray.
    TrayHover(window::Id, Option<usize>),
    TrayClick(String),
    TrayMiddle(String),
    TrayMenu(window::Id, usize),
    TrayMenuItemClick {
        address: String,
        menu_path: String,
        id: i32,
    },
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
    SelectOutput(String),
    Brightness(f32),
    StreamVolume(String, f32),
    StreamMute(String),
    Session(SessionAction),
    Confirm(bool),
    /// A session action did not go through: why.
    SessionRefused(String),
    ScreenshotMenu,
    Screenshot(ScreenshotTarget),
    CancelScreenshot,
    Settings,
    /// The whole Steam library.
    Library,
    /// The close button of a toast, or of a line of history.
    NoticeDismiss(u32),
    /// A click on a toast or a line of history: its default action, or
    /// its application's window.
    NoticeOpen(u32),
    /// One of the buttons of a toast.
    NoticeAction {
        id: u32,
        key: String,
    },
    /// Time to hide the toasts whose timeout has elapsed.
    ExpireToasts,
    /// « Tout effacer » in the control center.
    ClearNotices,
}

/// An audio output sink choice displayed in the launcher selector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputChoice {
    pub name: String,
    pub label: String,
}

impl std::fmt::Display for OutputChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

pub struct Dock {
    pub options: Options,
    pub config: Config,
    pub catalog: Catalog,
    /// The applications of the drawer, from A to Z.
    pub apps: Vec<App>,
    pub games: Vec<Game>,
    pub icons: Icons,
    /// What the icons last asked for were chosen from.
    icon_style: IconStyle,
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
    pub tray: Vec<TrayItem>,
    pub tray_menu: Option<(window::Id, window::Id, String)>,
    pub drawer: Option<Drawer>,
    pub toasts: Option<ToastSurface>,

    pub audio: Audio,
    pub levels: HashMap<u32, f32>,
    volumes: Volumes,
    pub network: Switch,
    pub bluetooth: Switch,
    pub brightness: Option<f32>,
    pub do_not_disturb: bool,
    pub night_light: bool,
    pub notify: NotifyState,
    ticks: u64,
}

impl Dock {
    pub fn new(options: Options) -> (Self, Task<Message>) {
        let catalog = Catalog::load();
        let games = steam::load();
        let config = read_config(&options.config, &catalog, &games);

        // Steam writes a desktop file for each game: those are listed with
        // the games, not among the applications.
        let mut apps = catalog.apps().to_vec();
        apps.retain(|app| steam::game_of_command(&app.command).is_none());
        drawer::sort_by_name(&mut apps, |app| &app.name);
        let theme = config.theme.unwrap_or_default();
        let icon_style = icons::style(theme, config.accent.as_deref().unwrap_or_default());
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
            tray: if simulated(Backend::Tray) {
                mock::tray()
            } else {
                Vec::new()
            },
            tray_menu: None,
            options,
            config,
            catalog,
            apps,
            games,
            icons: Icons::default(),
            icon_style,
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
            toasts: None,
            levels: HashMap::new(),
            volumes,
            do_not_disturb: false,
            night_light: false,
            notify: NotifyState::new(),
            ticks: 0,
        };
        let mut tasks = vec![dock.load_icons(), dock.show_bars()];
        if dock.options.drawer || dock.options.mixer {
            tasks.push(Task::done(Message::Launcher(None)));
        }
        if dock.options.mixer {
            tasks.push(Task::done(Message::ToggleMixer));
        }
        (dock, Task::batch(tasks))
    }

    /// Looks the icons of the applications up again, in the icon theme that
    /// goes with `icon_style`. This reads many directories: off the
    /// interface.
    fn load_icons(&self) -> Task<Message> {
        let mut names: Vec<String> = self
            .catalog
            .apps()
            .iter()
            .filter_map(|app| app.icon.clone())
            .collect();
        let favorites = self.config.favorites.iter();
        names.extend(favorites.filter_map(|name| self.identify(name).icon));
        let style = self.icon_style;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || Icons::load(names, style))
                    .await
                    .unwrap_or_default()
            },
            Message::IconsLoaded,
        )
    }

    /// The theme or the accent changed: the icons follow.
    fn follow_icon_style(&mut self) -> Task<Message> {
        let style = icons::style(self.snapshot.theme, &self.snapshot.accent);
        if style == self.icon_style {
            return Task::none();
        }
        self.icon_style = style;
        self.load_icons()
    }

    pub fn palette(&self) -> Palette {
        let accent = parse_color(&self.snapshot.accent);
        Palette::new(self.snapshot.theme, accent)
    }

    pub fn metrics(&self) -> ui::bar::Metrics {
        ui::bar::Metrics::new(self.config.icon_size())
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

    /// Where tray item `index` of bar `id` is on the whole desktop, as X11
    /// counts (in real pixels): where a program that draws its own menu
    /// (Wine) opens it.
    fn tray_point(&self, id: window::Id, index: usize) -> (i32, i32) {
        let Some(bar) = self.bar(id) else {
            return (0, 0);
        };
        let Some(screen) = self.screen(bar.screen) else {
            return (0, 0);
        };
        let pinned = bar.pinned();
        let (x, y, width, height) = self.metrics().tray_rect(
            bar.orientation,
            index,
            pinned,
            bar.entries.len() - pinned,
            self.tray.len(),
            bar.unfold > 0.5,
        );
        let surface = (bar.size.0 as i32, bar.size.1 as i32);
        let (left, top) = crate::model::tray::surface_origin(
            bar.orientation,
            (screen.width, screen.height),
            surface,
        );
        let x11 =
            |logical: i32| (f64::from(logical) * self.snapshot.x11_scale.get()).round() as i32;
        (
            x11(screen.x + left + x + width / 2),
            x11(screen.y + top + y + height / 2),
        )
    }

    /// Where the tray item at `address` was clicked: on the bar the pointer
    /// is over, else on the bar its menu came from.
    fn tray_pointed(&self, address: &str) -> (i32, i32) {
        let Some(index) = self.tray.iter().position(|item| item.address == address) else {
            return (0, 0);
        };
        let hovered = self
            .bars
            .iter()
            .find(|bar| bar.hover_tray == Some(index))
            .map(|bar| bar.id);
        let from_menu = self.tray_menu.as_ref().map(|(_, bar, _)| *bar);
        hovered
            .or(from_menu)
            .map_or((0, 0), |id| self.tray_point(id, index))
    }

    /// A screen, as the compositor describes it.
    /// Games hold every screen (a laptop playing): the launcher may open
    /// over a game, having nowhere else to go.
    fn every_screen_held(&self) -> bool {
        let screens = &self.snapshot.screens;
        !screens.is_empty() && screens.iter().all(|screen| screen.anchored)
    }

    /// The screens the compositor has; none before it says.
    pub fn present_screens(&self) -> Vec<OutputRole> {
        self.snapshot
            .screens
            .iter()
            .map(|screen| screen.role)
            .collect()
    }

    /// The screens that have a dock, as the configuration and the screens
    /// plugged in say: a lone screen always has one, unless turned off.
    fn docks(&self) -> Vec<(OutputRole, Orientation)> {
        self.config.docks(&self.present_screens())
    }

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
        for (index, (screen, orientation)) in self.docks().into_iter().enumerate() {
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
            let metrics = self.metrics();
            let size =
                metrics.surface_size(orientation, pinned, entries.len() - pinned, self.tray.len());
            let anchor = match orientation {
                Orientation::Vertical(Side::Left) => Anchor::Left,
                Orientation::Vertical(Side::Right) => Anchor::Right,
                Orientation::Horizontal => Anchor::Bottom,
            };
            let (id, task) = Message::layershell_open(NewLayerShellSettings {
                size: Some(size),
                layer: Layer::Top,
                anchor,
                exclusive_zone: Some(metrics.exclusive_zone),
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
                hover_tray: None,
            });
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    /// Gives each bar the size its entries need, and limits what takes the
    /// pointer to what shows.
    fn fit_bars(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let metrics = self.metrics();
        for index in 0..self.bars.len() {
            let bar = &self.bars[index];
            let (id, pinned) = (bar.id, bar.pinned());
            let size = metrics.surface_size(
                bar.orientation,
                pinned,
                bar.entries.len() - pinned,
                self.tray.len(),
            );
            let resize = if size == bar.size {
                Task::none()
            } else {
                self.bars[index].size = size;
                Task::done(Message::SizeChange { id, size })
            };
            let exclusive = Task::done(Message::ExclusiveZoneChange {
                id,
                zone_size: metrics.exclusive_zone,
            });
            tasks.push(resize.chain(exclusive).chain(self.input_region(id)));
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
        let (x, y, width, height) =
            self.metrics()
                .input_region(bar.orientation, bar.size, unfolded);
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
            Command::OpenOn { .. }
            | Command::ToggleLauncher
            | Command::ReloadConfig
            | Command::Screenshot { .. }
            | Command::Quit => {}
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
            Request::SelectOutput(ref name) => {
                self.audio.output_name = name.clone();
                if let Some(sink) = self.audio.sinks.iter().find(|s| &s.name == name) {
                    self.audio.output = sink.description.clone();
                }
            }
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
            Request::TrayActivate { .. }
            | Request::TraySecondary { .. }
            | Request::TrayContextMenu { .. }
            | Request::TrayMenuItem { .. }
            | Request::NotifyClose { .. }
            | Request::NotifyAction { .. } => {}
        }
    }

    /// The lines of the mixer.
    pub fn mixer_lines(&self) -> Vec<mixer::Line> {
        mixer::lines(&self.audio.streams, &|name| self.identify(name))
    }

    /// The audio output choices visible in the launcher selector.
    pub fn visible_sinks(&self) -> Vec<OutputChoice> {
        let mut choices: Vec<OutputChoice> = self
            .audio
            .sinks
            .iter()
            .filter(|sink| self.config.is_audio_sink_visible(&sink.name))
            .map(|sink| {
                let label = self
                    .config
                    .audio_sink_nick(&sink.name)
                    .unwrap_or(&sink.description)
                    .to_owned();
                OutputChoice {
                    name: sink.name.clone(),
                    label,
                }
            })
            .collect();
        if choices.is_empty()
            && (!self.audio.output_name.is_empty() || !self.audio.output.is_empty())
        {
            let label = self
                .config
                .audio_sink_nick(&self.audio.output_name)
                .unwrap_or(&self.audio.output)
                .to_owned();
            choices.push(OutputChoice {
                name: self.audio.output_name.clone(),
                label,
            });
        }
        choices
    }

    /// The currently active output choice.
    pub fn current_output_choice(&self) -> Option<OutputChoice> {
        let visible = self.visible_sinks();
        if let Some(found) = visible.iter().find(|c| {
            (!self.audio.output_name.is_empty() && c.name == self.audio.output_name)
                || c.label == self.audio.output
        }) {
            return Some(found.clone());
        }
        visible.into_iter().next()
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

    /// The configuration file changed (the settings screen wrote it): the
    /// bars follow what it now says of each screen.
    fn reload_config(&mut self) -> Task<Message> {
        let config = read_config(&self.options.config, &self.catalog, &self.games);
        if config == self.config {
            return Task::none();
        }
        info!("configuration read again");
        self.config = config;
        let mut tasks = vec![self.close_menu()];
        tasks.push(self.drop_stale_bars());
        tasks.push(self.show_bars());
        self.rebuild_entries();
        tasks.push(self.fit_bars());
        tasks.push(self.sync_toasts());
        Task::batch(tasks)
    }

    /// A bar whose screen lost its dock, or whose dock now lies the other
    /// way, goes; `show_bars` opens the ones that are missing.
    fn drop_stale_bars(&mut self) -> Task<Message> {
        let docks = self.docks();
        let (kept, gone): (Vec<BarState>, Vec<BarState>) = std::mem::take(&mut self.bars)
            .into_iter()
            .partition(|bar| docks.contains(&(bar.screen, bar.orientation)));
        self.bars = kept;
        if gone.is_empty() {
            return Task::none();
        }
        self.drag = None;
        let mut tasks = Vec::new();
        // The launcher goes with the bar that opened it.
        let orphan = |drawer: &Drawer| gone.iter().any(|bar| bar.screen == drawer.screen);
        if self.drawer.as_ref().is_some_and(orphan) {
            tasks.push(self.update(Message::CloseDrawer));
        }
        tasks.extend(gone.iter().map(|bar| window::close(bar.id)));
        Task::batch(tasks)
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
        let every_screen = self.every_screen_held();
        let picked = bar::launcher_screen(&docks, asked, every_screen)
            .and_then(|screen| self.bar_on(screen));
        let Some((screen, orientation)) = picked.map(|bar| (bar.screen, bar.orientation)) else {
            return Task::none();
        };
        let output = self.output_for_bar(screen);
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
            screenshot_prompt: false,
            note: None,
            width: self.drawer_width(None) as f32,
            height: self.drawer_height(None) as f32,
            resizing: false,
            surface: None,
        });
        // The control center shows the notices: no toast over it.
        let toasts = self.sync_toasts();
        self.close_menu().chain(task).chain(toasts)
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

    fn close_tray_menu(&mut self) -> Task<Message> {
        match self.tray_menu.take() {
            Some((id, ..)) => window::close(id),
            None => Task::none(),
        }
    }

    fn notify_bars(&self) -> Vec<(OutputRole, Orientation)> {
        self.bars
            .iter()
            .map(|bar| (bar.screen, bar.orientation))
            .collect()
    }

    fn anchored_roles(&self) -> Vec<OutputRole> {
        self.snapshot
            .screens
            .iter()
            .filter(|screen| screen.anchored)
            .map(|screen| screen.role)
            .collect()
    }

    /// Bar that should host toasts: none on an anchored screen, when DND
    /// is in effect, or while the launcher (and its control center, that
    /// lists the notices) is open.
    fn toast_target(&self) -> Option<(OutputRole, Orientation)> {
        let anchored = self.anchored_roles();
        if self.notify.effective_dnd(!anchored.is_empty()) || self.drawer.is_some() {
            return None;
        }
        self.notify.toast_bar(&self.notify_bars(), &anchored)
    }

    fn close_toasts(&mut self) -> Task<Message> {
        match self.toasts.take() {
            Some(surface) => window::close(surface.id),
            None => Task::none(),
        }
    }

    fn output_for_bar(&self, screen: OutputRole) -> OutputOption {
        let first = self.docks().first().map(|(first, _)| *first) == Some(screen);
        let named = self.options.output.clone().filter(|_| first);
        match (named, self.screen(screen)) {
            (Some(name), _) => OutputOption::OutputName(name),
            (None, Some(known)) if self.options.is_real(Backend::Compositor) => {
                OutputOption::OutputName(known.name.clone())
            }
            _ => OutputOption::Active,
        }
    }

    /// Where toasts go now: the bar's screen, its orientation, how many
    /// capsules show and the overlay that holds them. `None` when none is
    /// to show.
    fn toast_layout(&self) -> Option<(OutputRole, Orientation, usize, ToastGeometry)> {
        if self.notify.visible.is_empty() {
            return None;
        }
        let (screen, orientation) = self.toast_target()?;
        let metrics = self.metrics();
        let thickness = metrics.width.round() as u32;
        let bar_height = self.bar_on(screen).map_or(0.0, |bar| {
            let pinned = bar.pinned();
            let others = bar.entries.len() - pinned;
            metrics.bar_height(pinned, others, self.tray.len())
        });
        let screen_height = self.screen(screen).map(|known| known.height.max(0) as u32);
        let fit = toasts_that_fit(
            orientation,
            screen_height,
            bar_height.ceil() as u32,
            thickness,
        );
        let shown = self.notify.visible.len().min(fit);
        let geometry = toast_geometry(orientation, shown, thickness);
        Some((screen, orientation, shown, geometry))
    }

    fn open_toasts(
        &mut self,
        screen: OutputRole,
        orientation: Orientation,
        shown: usize,
        geometry: ToastGeometry,
    ) -> Task<Message> {
        let anchor = match geometry.anchor {
            ToastAnchor::BottomLeft => Anchor::Left | Anchor::Bottom,
            ToastAnchor::BottomRight => Anchor::Right | Anchor::Bottom,
        };
        let (id, task) = Message::layershell_open(NewLayerShellSettings {
            size: Some((geometry.width, geometry.height)),
            layer: Layer::Overlay,
            anchor,
            // In the corner itself, like the dock: not beside its zone.
            exclusive_zone: Some(-1),
            margin: None,
            keyboard_interactivity: KeyboardInteractivity::None,
            output_option: self.output_for_bar(screen),
            events_transparent: false,
            namespace: Some("coquille-toasts".to_owned()),
        });
        self.toasts = Some(ToastSurface {
            id,
            screen,
            orientation,
            geometry,
            shown,
        });
        task.chain(self.toast_input_region())
    }

    /// The overlay takes the pointer on its capsules only: the shadow
    /// around them lets clicks through to the windows.
    fn toast_input_region(&self) -> Task<Message> {
        let Some(surface) = self.toasts else {
            return Task::none();
        };
        let (x, y, width, height) = surface.geometry.pile;
        let (x, y, width, height) = (x as i32, y as i32, width as i32, height as i32);
        Task::done(Message::SetInputRegion {
            id: surface.id,
            callback: ActionCallback::new(move |region| region.add(x, y, width, height)),
        })
    }

    fn sync_toasts(&mut self) -> Task<Message> {
        self.notify.user_dnd = self.do_not_disturb;
        let anchored = self.anchored_roles();
        if self.notify.effective_dnd(!anchored.is_empty()) {
            self.notify.hide_toasts();
        }
        let layout = self.toast_layout();
        let open = self
            .toasts
            .map(|surface| (surface.screen, surface.geometry));
        let wanted = layout.map(|(screen, _, _, geometry)| (screen, geometry));
        match toast_surface_plan(open, wanted) {
            ToastSurfacePlan::Close => self.close_toasts(),
            ToastSurfacePlan::Open | ToastSurfacePlan::Relocate => {
                let (screen, orientation, shown, geometry) =
                    layout.expect("the plan opens a wanted overlay");
                let close = self.close_toasts();
                close.chain(self.open_toasts(screen, orientation, shown, geometry))
            }
            plan @ (ToastSurfacePlan::Keep | ToastSurfacePlan::Resize { .. }) => {
                let (Some(surface), Some((_, orientation, shown, geometry))) =
                    (&mut self.toasts, layout)
                else {
                    return Task::none();
                };
                // The same corner: a lying bar and an upright one on the
                // right share it.
                surface.orientation = orientation;
                surface.shown = shown;
                surface.geometry = geometry;
                let id = surface.id;
                match plan {
                    ToastSurfacePlan::Resize { size } => {
                        Task::done(Message::SizeChange { id, size })
                            .chain(self.toast_input_region())
                    }
                    _ => Task::none(),
                }
            }
        }
    }

    /// The user closes a notice: its toast and its line in history go.
    fn dismiss_notice(&mut self, id: u32) -> Task<Message> {
        if self.notify.close(id) {
            self.request(Request::NotifyClose {
                id,
                reason: CloseReason::Dismissed.as_u32(),
            });
        }
        self.sync_toasts()
    }

    /// Tells the application which button was pressed. The notice is then
    /// done with, unless it asked to stay (`resident`).
    fn invoke_notice(&mut self, id: u32, key: String) -> Task<Message> {
        let Some(notice) = self.notify.notice(id) else {
            return Task::none();
        };
        let resident = notice.resident;
        self.request(Request::NotifyAction { id, key });
        if resident {
            self.notify.hide(id);
            return self.sync_toasts();
        }
        self.dismiss_notice(id)
    }

    /// A click on the notice: its default action when it has one (the
    /// application then shows what it was about), and its window comes to
    /// the front either way.
    fn open_notice(&mut self, id: u32) -> Task<Message> {
        let Some(notice) = self.notify.notice(id) else {
            return Task::none();
        };
        let default = notice.actions.iter().any(|(key, _)| key == "default");
        if let Some(window) = self.window_of_notice(notice) {
            self.send(Command::Activate { window });
        }
        if default {
            return self.invoke_notice(id, "default".into());
        }
        self.dismiss_notice(id)
    }

    /// The window of the application that sent a notice: the one its
    /// desktop entry names, else the one of its name.
    fn window_of_notice(&self, notice: &Notice) -> Option<u64> {
        let names = notice.desktop_entry.iter().chain([&notice.app]);
        names.filter(|name| !name.is_empty()).find_map(|name| {
            let identity = self.identify(name);
            let owned = |app: &str| {
                app.eq_ignore_ascii_case(name)
                    || identity
                        .names
                        .iter()
                        .any(|own| own.eq_ignore_ascii_case(app))
            };
            let windows = self
                .snapshot
                .windows
                .iter()
                .filter(|window| owned(&window.app));
            // The one in sight rather than a minimized one.
            let mut windows: Vec<_> = windows.collect();
            windows.sort_by_key(|window| window.minimized);
            windows.first().map(|window| window.id)
        })
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
        // Playing on a laptop, the launcher is the only way to the windows
        // the game hides: an application already open comes over the game
        // instead of starting again.
        if self.every_screen_held() {
            let named = |app: &str| {
                identity
                    .names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(app))
            };
            let open = self
                .snapshot
                .windows
                .iter()
                .find(|window| named(&window.app));
            if let Some(window) = open {
                self.send(Command::Activate { window: window.id });
                return;
            }
        }
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
                // Icons asked for before the theme changed again are stale.
                if icons.style() == Some(self.icon_style) {
                    self.icons = icons;
                }
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
                    // The edge of the drawer away from its bar follows the
                    // pointer.
                    Orientation::Vertical(side) => {
                        let wanted = match (side, self.drawer_screen()) {
                            (Side::Left, _) => pointer.x,
                            (Side::Right, Some(screen)) => screen.width - pointer.x,
                            (Side::Right, None) => drawer.width,
                        };
                        let width = self.drawer_width(Some(wanted)) as f32;
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
                        let foot = screen - self.metrics().exclusive_zone as f32;
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
                    Orientation::Vertical(_) if self.config.drawer_width != Some(width) => {
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
                if let Some((_, bar, _)) = self.tray_menu.take_if(|(menu, ..)| *menu == id) {
                    if let Some(bar) = self.bar_mut(bar) {
                        bar.hover_tray = None;
                    }
                    return self.input_region(bar);
                }
                let drawer = self.drawer.as_ref().is_some_and(|drawer| drawer.id == id);
                if drawer {
                    self.drawer = None;
                }
                if self.toasts.as_ref().is_some_and(|surface| surface.id == id) {
                    self.toasts = None;
                }
                self.bars.retain(|bar| bar.id != id);
                if drawer {
                    return self.sync_toasts();
                }
                Task::none()
            }
            Message::BarHover(id, hovered) => {
                // The menu keeps its bar as it is while it is open.
                let has_menu = self.menu.as_ref().is_some_and(|(_, bar, _)| *bar == id)
                    || self
                        .tray_menu
                        .as_ref()
                        .is_some_and(|(_, bar, _)| *bar == id);
                if has_menu && !hovered {
                    return Task::none();
                }
                let Some(bar) = self.bar_mut(id) else {
                    return Task::none();
                };
                bar.hovered = hovered;
                if !hovered {
                    bar.hover_entry = None;
                    bar.hover_tray = None;
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
                self.close_menu().chain(self.close_tray_menu())
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
                let menu = bar::menu(entry, &self.present_screens());
                let size = (ui::menu::WIDTH, ui::menu::height(&menu));
                let unfolded = bar.unfold > 0.5;
                let anchor =
                    self.metrics()
                        .entry_rect(bar.orientation, index, bar.pinned(), unfolded);
                // Beside an upright bar, above a lying one.
                let (corner, grows) = match bar.orientation {
                    Orientation::Vertical(Side::Left) => {
                        (PopupAnchor::TopRight, PopupGravity::BottomRight)
                    }
                    Orientation::Vertical(Side::Right) => {
                        (PopupAnchor::TopLeft, PopupGravity::BottomLeft)
                    }
                    Orientation::Horizontal => (PopupAnchor::Top, PopupGravity::Top),
                };
                let settings = IcedNewPopupSettings::new(id, size, anchor)
                    .anchor(corner)
                    .gravity(grows);
                let key = entry.identity.key.clone();
                let close = self.close_menu().chain(self.close_tray_menu());
                let (popup, open) = Message::popup_open(settings);
                self.menu = Some((popup, id, key));
                self.drag = None;
                close.chain(open)
            }
            Message::TrayHover(id, hover) => {
                let Some(bar) = self.bar_mut(id) else {
                    return Task::none();
                };
                bar.hover_tray = hover;
                Task::none()
            }
            Message::TrayClick(address) => {
                let (x, y) = self.tray_pointed(&address);
                self.request(backend::Request::TrayActivate { address, x, y });
                Task::none()
            }
            Message::TrayMiddle(address) => {
                let (x, y) = self.tray_pointed(&address);
                self.request(backend::Request::TraySecondary { address, x, y });
                Task::none()
            }
            Message::TrayMenu(id, index) => {
                let Some(bar) = self.bar(id) else {
                    return Task::none();
                };
                let Some(item) = self.tray.get(index) else {
                    return Task::none();
                };
                // No menu to show: the item draws its own (Wine and other
                // X11 tray icons do).
                if item.menu.is_none() || item.menu_path.is_none() {
                    let address = item.address.clone();
                    let (x, y) = self.tray_point(id, index);
                    self.request(backend::Request::TrayContextMenu { address, x, y });
                    return self.close_tray_menu().chain(self.close_menu());
                }
                let height = ui::tray_menu::height(item);
                let size = (ui::tray_menu::WIDTH, height);
                let unfolded = bar.unfold > 0.5;
                let pinned = bar.pinned();
                let others = bar.entries.len() - pinned;
                let anchor = self.metrics().tray_rect(
                    bar.orientation,
                    index,
                    pinned,
                    others,
                    self.tray.len(),
                    unfolded,
                );
                let (corner, grows) = match bar.orientation {
                    Orientation::Vertical(Side::Left) => {
                        (PopupAnchor::TopRight, PopupGravity::BottomRight)
                    }
                    Orientation::Vertical(Side::Right) => {
                        (PopupAnchor::TopLeft, PopupGravity::BottomLeft)
                    }
                    Orientation::Horizontal => (PopupAnchor::Top, PopupGravity::Top),
                };
                let settings = IcedNewPopupSettings::new(id, size, anchor)
                    .anchor(corner)
                    .gravity(grows);
                let address = item.address.clone();
                let close = self.close_tray_menu().chain(self.close_menu());
                let (popup, open) = Message::popup_open(settings);
                self.tray_menu = Some((popup, id, address));
                close.chain(open)
            }
            Message::TrayMenuItemClick {
                address,
                menu_path,
                id,
            } => {
                self.request(backend::Request::TrayMenuItem {
                    address,
                    menu_path,
                    id,
                });
                self.close_tray_menu()
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
                    window::close(drawer.id).chain(self.sync_toasts())
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
                    Toggle::DoNotDisturb => {
                        self.do_not_disturb = !self.do_not_disturb;
                        return self.sync_toasts();
                    }
                    // Kept for when there is a night light: for now it only
                    // remembers its position.
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
                // Simulated, the theme already changed.
                self.follow_icon_style()
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
            Message::SelectOutput(name) => {
                self.request(Request::SelectOutput(name));
                Task::none()
            }
            Message::Brightness(level) => {
                self.request(Request::Brightness(
                    level.clamp(services::MIN_BRIGHTNESS, 1.0),
                ));
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
            Message::ScreenshotMenu => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.screenshot_prompt = !drawer.screenshot_prompt;
                    drawer.confirm = None;
                }
                Task::none()
            }
            Message::CancelScreenshot => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.screenshot_prompt = false;
                }
                Task::none()
            }
            Message::Screenshot(target) => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.screenshot_prompt = false;
                }
                self.send(Command::Screenshot { target });
                self.update(Message::CloseDrawer)
            }
            Message::Settings => {
                spawn(&settings_command(&self.options.config));
                self.update(Message::CloseDrawer)
            }
            Message::NoticeDismiss(id) => self.dismiss_notice(id),
            Message::NoticeOpen(id) => self.open_notice(id),
            Message::NoticeAction { id, key } => self.invoke_notice(id, key),
            Message::ExpireToasts => {
                for id in self.notify.expire(Instant::now()) {
                    self.request(Request::NotifyClose {
                        id,
                        reason: CloseReason::Expired.as_u32(),
                    });
                }
                self.sync_toasts()
            }
            Message::ClearNotices => {
                for id in self.notify.clear() {
                    self.request(Request::NotifyClose {
                        id,
                        reason: CloseReason::Dismissed.as_u32(),
                    });
                }
                self.sync_toasts()
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
            compositor::Update::Event(Event::ConfigChanged) => self.reload_config(),
            compositor::Update::Event(Event::Brightness { up }) => {
                if let Some(level) = self.brightness {
                    self.request(Request::Brightness(services::brightness_step(level, up)));
                }
                Task::none()
            }
            compositor::Update::Event(Event::State(snapshot)) => {
                self.snapshot = snapshot;
                // The screens are known: a lone one gets its dock.
                let stale = self.drop_stale_bars();
                let show = Task::batch([stale, self.show_bars()]);
                self.rebuild_entries();
                // A game took the screen of the launcher: the launcher goes.
                let screen = self.drawer.as_ref().map(|drawer| drawer.screen);
                let held = screen.and_then(|role| self.screen(role));
                let elsewhere = !self.every_screen_held();
                let close = if held.is_some_and(|screen| screen.anchored) && elsewhere {
                    self.update(Message::CloseDrawer)
                } else {
                    Task::none()
                };
                let icons = self.follow_icon_style();
                let toasts = self.sync_toasts();
                Task::batch([close, show, self.fit_bars(), icons, toasts])
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
            backend::Update::Tray(items) => {
                self.tray = items;
                return self.fit_bars();
            }
            backend::Update::Notify(notice) => {
                self.notify.user_dnd = self.do_not_disturb;
                let bars = self.notify_bars();
                let anchored = self.anchored_roles();
                self.notify.ingest(notice, &bars, &anchored);
                return self.sync_toasts();
            }
            backend::Update::NotifyClosed { id, reason } => {
                if self.notify.close(id) {
                    self.request(Request::NotifyClose { id, reason });
                }
                return self.sync_toasts();
            }
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
        // Toasts on screen go when their time is up.
        if !self.notify.visible.is_empty() {
            let pace = Duration::from_millis(250);
            subscriptions.push(iced::time::every(pace).map(|_| Message::ExpireToasts));
        }

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
        if self.options.is_real(Backend::Tray) {
            subscriptions.push(backend::tray::subscription().map(Message::Backend));
        }
        if self.options.is_real(Backend::Notifications) {
            subscriptions.push(backend::notify::subscription().map(Message::Backend));
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
        if let Some((_, _, address)) = self.tray_menu.as_ref().filter(|(menu, ..)| *menu == id) {
            if let Some(item) = self.tray.iter().find(|item| item.address == *address) {
                return ui::tray_menu::view(self, item);
            }
        }
        if let Some(drawer) = self.drawer.as_ref().filter(|drawer| drawer.id == id) {
            return ui::drawer::view(self, drawer);
        }
        if let Some(surface) = self.toasts.as_ref().filter(|surface| surface.id == id) {
            return ui::toasts::view(self, surface);
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
    if bar.hovered && matches!(bar.orientation, Orientation::Vertical(_)) {
        1.0
    } else {
        0.0
    }
}

/// Reads the configuration of the dock.
fn read_config(path: &Path, catalog: &Catalog, games: &[Game]) -> Config {
    let mut config = config::load(path);
    // A favorite written twice under two names would show once and break
    // the count: keep the first name of each application.
    let mut seen = Vec::new();
    config.favorites.retain(|name| {
        let key = identify(catalog, games, name).key;
        let new = !seen.contains(&key);
        seen.push(key);
        new
    });
    config
}

/// A text as one word of a shell command.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The command of the settings screen of the shell: the program installed
/// next to this one, else the one of the `PATH`, on the configuration file
/// this dock reads.
fn settings_command(config: &Path) -> String {
    const PROGRAM: &str = "coquille-settings";
    let beside = std::env::current_exe()
        .ok()
        .and_then(|dock| Some(dock.parent()?.join(PROGRAM)))
        .filter(|program| program.is_file());
    let program = beside.map_or_else(|| PROGRAM.to_owned(), |path| path.display().to_string());
    let config = config.display().to_string();
    format!("{} --config {}", quoted(&program), quoted(&config))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_with_spaces_or_quotes_stays_one_word_of_the_command() {
        assert_eq!(quoted("/home/a b/config.toml"), "'/home/a b/config.toml'");
        assert_eq!(quoted("l'essai"), "'l'\\''essai'");
    }

    #[test]
    fn the_settings_screen_opens_on_the_file_the_dock_reads() {
        let command = settings_command(Path::new("/tmp/essai/config.toml"));

        assert!(command.contains("coquille-settings"));
        assert!(command.ends_with(" --config '/tmp/essai/config.toml'"));
    }
}
