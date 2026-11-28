//! The state of the dock, shared by its surfaces (the bar, its menu, the
//! drawer), and everything that changes it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use coquille_config::ShellMode;
use coquille_ipc::{Command, Event, GlassShape, ScreenshotTarget, Snapshot, ThemeName};
use coquille_model::OutputRole;
use iced::widget::operation;
use iced::{Color, Element, Point, Size, Subscription, Task, keyboard, window};
use iced_layershell::actions::{ActionCallback, IcedNewPopupSettings};
use iced_layershell::reexport::{
    Anchor, KeyboardInteractivity, Layer, NewLayerShellSettings, OutputOption, PopupAnchor,
    PopupGravity,
};
use iced_layershell::to_layer_message;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::backend::{
    self, Audio, Backend, Request, SessionAction, Switch, compositor,
    foundry::{self, FoundryError},
    mock, services,
    tasks::{self, TasksError},
};
use crate::config::{self, Config};
use crate::model::bar::{self, Click, Entry, MenuAction};
use crate::model::battery::{self, Alert, Battery, Charge};
use crate::model::drawer::{self, LyingLayout, Step};
use crate::model::mixer::{self, Volumes};
use crate::model::motion::{self, Launch, Pace, Reveal, durations};
use crate::model::notify::{
    CloseReason, Icon as NoticeIcon, Notice, NotifyState, ToastAnchor, ToastGeometry,
    ToastSurfacePlan, toast_geometry, toast_surface_plan, toasts_that_fit,
};
use crate::model::tray::TrayItem;
use crate::model::widgets::{
    DailyTask, DailyTasksStatus, GameNotes, ProjectTask, ProjectTasksStatus, WidgetsState,
};
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
    /// How many times slower the dock moves.
    pub slowdown: f32,
    /// Force spatial mode regardless of config file.
    pub spatial: bool,
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
/// The layer-shell names of the bars and of the notifications.
const BAR_NAMESPACE: &str = "coquille-dock";
const TOASTS_NAMESPACE: &str = "coquille-toasts";
const HOME_NAMESPACE: &str = "coquille-home";
const CONTROLS_NAMESPACE: &str = "coquille-controls";

pub struct BarState {
    pub id: window::Id,
    pub screen: OutputRole,
    pub output_name: Option<String>,
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
    /// The entries that came in lately, by key: their tiles grow in.
    pub arrivals: HashMap<String, Instant>,
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
    /// How far it is out from behind its bar.
    pub reveal: Reveal,
    /// How far the mixer is open, beside an upright bar.
    pub mixer_reveal: Reveal,
    /// The user is dragging its edge.
    pub resizing: bool,
    /// The size of the screen it is on, once the surface says.
    pub surface: Option<Size>,
}

impl Drawer {
    /// Whether the mixer shows: always under the control center of the
    /// launcher above a lying bar, beside an upright one when asked for.
    pub fn mixer_shown(&self) -> bool {
        self.mixer || self.orientation == Orientation::Horizontal
    }

    /// How the launcher above a lying bar shares its screen out, with or
    /// without `pinned` applications above the grid; before its surface
    /// says how large the screen is, as on a 16:9 one.
    pub fn lying_layout(&self, pinned: bool) -> Option<LyingLayout> {
        if self.orientation != Orientation::Horizontal {
            return None;
        }
        let screen = self.surface.unwrap_or(Size::new(1920.0, 1080.0));
        let layout = drawer::lying_layout(screen.width, screen.height, self.height, pinned);
        Some(layout)
    }

    /// How many columns the grid of applications has.
    pub fn app_columns(&self, pinned: bool) -> usize {
        let layout = self.lying_layout(pinned);
        layout.map_or(drawer::COLUMNS, |layout| layout.app_columns)
    }

    /// How many applications a page of the grid holds.
    pub fn page_size(&self, pinned: bool) -> usize {
        let layout = self.lying_layout(pinned);
        layout.map_or(drawer::COLUMNS * drawer::ROWS, |layout| layout.page())
    }

    /// How many columns the results of a search have.
    pub fn result_columns(&self) -> usize {
        let layout = self.lying_layout(false);
        layout.map_or(drawer::COLUMNS, |layout| layout.result_columns)
    }

    /// How many results a search shows.
    pub fn results_shown(&self) -> usize {
        let layout = self.lying_layout(false);
        let upright = drawer::COLUMNS * drawer::RESULT_ROWS;
        layout.map_or(upright, |layout| layout.results_shown())
    }
}

/// Which decoupled popover is open beside the spatial sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialPopoverKind {
    Notifications,
    Controls,
}

/// The decoupled notifications or control-center popover of spatial mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialPopover {
    pub id: window::Id,
    pub screen: OutputRole,
    pub side: Side,
    pub kind: SpatialPopoverKind,
    pub mixer: bool,
    pub confirm: Option<SessionAction>,
    pub screenshot_prompt: bool,
    pub note: Option<String>,
}

/// Permanent spatial home surface (`Layer::Bottom`) on an output.
#[derive(Debug, Clone, PartialEq)]
pub struct SpatialHome {
    pub id: window::Id,
    pub screen: OutputRole,
    pub output_name: Option<String>,
    pub query: String,
    pub page: usize,
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
    /// The applications and the games read again, and the summary of what
    /// was installed they were read for.
    Installed(u64, Catalog, Vec<Game>),
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
    /// Toggle the decoupled spatial notifications or control-center popover.
    SpatialPopoverToggle(OutputRole, SpatialPopoverKind),
    /// Close the spatial popover.
    CloseSpatialPopover,
    /// Update search query in spatial launcher (`None` = overlay, `Some` = bottom home).
    SpatialQuery(Option<OutputRole>, String),
    /// Submit search (Enter) in spatial launcher.
    SpatialSubmit(Option<OutputRole>),
    /// Jump to a specific application page in the spatial launcher.
    SpatialPage(Option<OutputRole>, usize),
    /// Turn the application grid page forward (`true`) or backward (`false`) in the spatial launcher.
    SpatialTurnPage(Option<OutputRole>, bool),
    /// Show a page of the « Jeux » or « Agenda » card of the spatial launcher.
    SpatialCardPage(crate::ui::spatial::cards::CardKind, usize),
    /// Launch or teleport an application from the spatial launcher grid onto a screen.
    SpatialLaunchApp(OutputRole, String),
    /// Open context menu for an app tile from the spatial launcher grid.
    SpatialAppMenu(window::Id, OutputRole, String),
    /// Toggle a « Vie quotidienne » task in the spatial launcher.
    DailyTaskToggle(String),
    /// Start inline editing of a game's personal comment.
    GameNoteEdit(String, String),
    /// Update draft text while editing a game's personal comment.
    GameNoteDraft(String),
    /// Commit and persist the edited game comment.
    GameNoteSave,
    /// Cancel inline editing of a game comment.
    GameNoteCancel,
    /// Launch a game card from the spatial launcher.
    SpatialLaunchGame(String),
    /// Refresh daily tasks from the remote API.
    RefreshDailyTasks,
    /// Result of fetching daily tasks from the remote API.
    DailyTasksLoaded(u64, Result<Vec<DailyTask>, TasksError>),
    /// Refresh project tickets from Kama-Foundry.
    RefreshProjectTasks,
    /// Result of fetching project tickets from Kama-Foundry.
    ProjectTasksLoaded(u64, Result<Vec<ProjectTask>, FoundryError>),
    /// Open a Kama-Foundry ticket URL in the default browser.
    OpenProjectTask(String),
    /// Open an external URL in the default browser.
    OpenExternalUrl(String),
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

enum PendingPopup {
    Entry {
        settings: IcedNewPopupSettings,
        bar_id: window::Id,
        key: String,
    },
    Tray {
        settings: IcedNewPopupSettings,
        bar_id: window::Id,
        address: String,
    },
    SpatialApp {
        settings: IcedNewPopupSettings,
        surface_id: window::Id,
        key: String,
        entry: Box<Entry>,
    },
}

pub struct Dock {
    pub options: Options,
    pub config: Config,
    pub catalog: Catalog,
    /// The applications of the drawer, from A to Z.
    pub apps: Vec<App>,
    pub games: Vec<Game>,
    /// What was installed when `catalog` and `games` were read, summed up.
    installed: u64,
    pub icons: Icons,
    /// What the icons last asked for were chosen from.
    icon_style: IconStyle,
    pub snapshot: Snapshot,
    link: Option<UnboundedSender<Command>>,
    /// The liquid glass last told to the compositor, by panel and screen.
    glass_told: HashMap<(&'static str, OutputRole), Vec<GlassShape>>,
    links: HashMap<Backend, UnboundedSender<Request>>,
    /// The bars shown, one per screen that has a dock.
    pub bars: Vec<BarState>,
    pub unread: Vec<(String, u32)>,
    pub now: DateTime<Local>,
    pub user: String,

    last_frame: Option<Instant>,
    /// The time of the frame being drawn, for what moves.
    pub moment: Instant,
    /// The launcher going back behind its bar, closed once it is there.
    leaving: Option<Drawer>,
    /// The applications started from the bar whose icons hop.
    pub launches: Vec<Launch>,
    pub drag: Option<Drag>,
    /// The menu, with the bar/surface it opened from and the key of its
    /// application, plus an optional synthesized entry for spatial app-grid menus.
    pub menu: Option<(window::Id, window::Id, String, Option<Entry>)>,
    pub tray: Vec<TrayItem>,
    pub tray_menu: Option<(window::Id, window::Id, String)>,
    /// Popups whose close was requested and whose `Closed` event has not arrived yet.
    closing_popups: Vec<window::Id>,
    /// Next popup to open once every closing popup has been destroyed.
    pending_popup: Option<PendingPopup>,
    pub drawer: Option<Drawer>,
    pub toasts: Option<ToastSurface>,

    pub audio: Audio,
    pub levels: HashMap<u32, f32>,
    volumes: Volumes,
    pub network: Switch,
    pub bluetooth: Switch,
    pub brightness: Option<f32>,
    /// A laptop's battery; `None` on a desktop.
    pub battery: Option<Battery>,
    pub do_not_disturb: bool,
    pub night_light: bool,
    pub notify: NotifyState,
    pub playtimes: HashMap<u64, u32>,
    pub widgets: WidgetsState,
    /// Inline game note editor: `(game_key, draft_text)`.
    pub editing_game: Option<(String, String)>,
    /// Current page of the spatial « Jeux » card.
    pub games_page: usize,
    /// Current page of the spatial « Agenda » card.
    pub calendar_page: usize,
    pub spatial_popover: Option<SpatialPopover>,
    pub spatial_homes: Vec<SpatialHome>,
    pub tasks_fetching: bool,
    tasks_fetch_seq: u64,
    last_tasks_fetch: Option<Instant>,
    pub foundry_fetching: bool,
    foundry_fetch_seq: u64,
    last_foundry_fetch: Option<Instant>,
    ticks: u64,
}

impl Dock {
    pub fn new(options: Options) -> (Self, Task<Message>) {
        // Taken first: what is installed meanwhile is read again later.
        let installed = backend::installed::fingerprint();
        let catalog = Catalog::load();
        let games = steam::load();
        let playtimes = steam::load_playtimes();
        let config = read_config(&options.config, &catalog, &games);
        let game_notes = GameNotes::load(&options.state_dir);
        let mut widgets = WidgetsState::mock_default(game_notes);
        if options.is_real(Backend::Compositor) {
            widgets.daily_tasks.clear();
            widgets.daily_tasks_status = if config.tasks_token.is_some() {
                DailyTasksStatus::Loading
            } else {
                DailyTasksStatus::MissingToken
            };
            widgets.project_tasks.clear();
            widgets.project_tasks_status = if config.foundry_token.is_some() {
                ProjectTasksStatus::Loading
            } else {
                ProjectTasksStatus::MissingToken
            };
        }
        let apps = drawer_apps(&catalog);
        let theme = config.theme.unwrap_or_default();
        let icon_style = icons::style(theme, config.accent.as_deref().unwrap_or_default());
        let simulated = |backend| !options.is_real(backend);
        let snapshot = if simulated(Backend::Compositor) {
            let mut snapshot = mock::snapshot(theme, &config.favorites);
            snapshot.accent = config.accent.clone().unwrap_or_default();
            snapshot.spatial = options.spatial || config.mode == ShellMode::Spatial;
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
            battery: simulated(Backend::Battery).then_some(Battery {
                percent: 76,
                charge: Charge::Discharging,
                time: Some(Duration::from_secs(3 * 3600 + 12 * 60)),
            }),
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
            closing_popups: Vec::new(),
            pending_popup: None,
            options,
            config,
            catalog,
            apps,
            games,
            installed,
            icons: Icons::default(),
            icon_style,
            snapshot,
            link: None,
            glass_told: HashMap::new(),
            links: HashMap::new(),
            bars: Vec::new(),
            now: Local::now(),
            user: user_name(),
            last_frame: None,
            moment: Instant::now(),
            leaving: None,
            launches: Vec::new(),
            drag: None,
            menu: None,
            drawer: None,
            toasts: None,
            levels: HashMap::new(),
            volumes,
            do_not_disturb: false,
            night_light: false,
            notify: NotifyState::new(),
            playtimes,
            widgets,
            editing_game: None,
            games_page: 0,
            calendar_page: 0,
            spatial_popover: None,
            spatial_homes: Vec::new(),
            tasks_fetching: false,
            tasks_fetch_seq: 0,
            last_tasks_fetch: None,
            foundry_fetching: false,
            foundry_fetch_seq: 0,
            last_foundry_fetch: None,
            ticks: 0,
        };
        let mut tasks = vec![
            dock.load_icons(),
            dock.show_bars(),
            dock.sync_spatial_homes(),
        ];
        if dock.is_spatial() && dock.config.tasks_token.is_some() {
            tasks.push(dock.fetch_daily_tasks());
        }
        if dock.is_spatial() && dock.config.foundry_token.is_some() {
            tasks.push(dock.fetch_project_tasks());
        }
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
        ui::bar::Metrics::new(self.config.icon_size()).with_battery(self.battery.is_some())
    }

    pub fn spatial_metrics(&self) -> ui::spatial::bar::Metrics {
        ui::spatial::bar::Metrics::new(self.config.icon_size())
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

    /// Whether the dock is running in spatial mode.
    pub fn is_spatial(&self) -> bool {
        self.options.spatial || self.config.mode == ShellMode::Spatial || self.snapshot.spatial
    }

    /// Whether any screen is currently held by a fullscreen game.
    pub fn any_anchored(&self) -> bool {
        self.snapshot.screens.iter().any(|screen| screen.anchored)
    }

    /// Visible unread notification count for an application entry (0 when DND/game).
    pub fn spatial_unread_for(&self, entry: &Entry) -> usize {
        let count = self
            .notify
            .unread_for_app(&entry.identity.names, self.any_anchored());
        if count > 0 || self.notify.effective_dnd(self.any_anchored()) {
            return count;
        }
        // Fallback to simulated unread badges in `--mock` mode.
        self.unread
            .iter()
            .find(|(key, _)| key == &entry.identity.key)
            .map_or(0, |(_, n)| *n as usize)
    }

    /// Total visible unread notification count for the bottom pill badge.
    pub fn spatial_visible_unread(&self) -> usize {
        let count = self.notify.visible_unread_count(self.any_anchored());
        if count > 0 || self.notify.effective_dnd(self.any_anchored()) {
            return count;
        }
        self.unread.iter().map(|(_, n)| *n as usize).sum()
    }

    /// Whether the spatial launcher is currently shown on `screen` (either as
    /// overlay or because no non-minimized window is visible on `screen`).
    pub fn spatial_launcher_visible_on(&self, screen: OutputRole) -> bool {
        let overlay = self
            .drawer
            .as_ref()
            .is_some_and(|drawer| drawer.screen == screen);
        overlay || !crate::model::spatial::screen_has_visible_window(&self.snapshot.windows, screen)
    }

    /// Finds an entry for a context menu: on the originating bar first, or
    /// synthesized for an app right-clicked from the spatial launcher grid.
    fn entry_for_menu(&self, parent: window::Id, key: &str) -> Option<Entry> {
        if let Some(bar) = self.bar(parent) {
            if let Some(entry) = bar.entries.iter().find(|entry| entry.identity.key == key) {
                return Some(entry.clone());
            }
        }
        // Synthesize from the screen of the spatial launcher/home surface.
        let screen = self
            .drawer
            .as_ref()
            .filter(|drawer| drawer.id == parent)
            .map(|drawer| drawer.screen)
            .or_else(|| {
                self.spatial_homes
                    .iter()
                    .find(|home| home.id == parent)
                    .map(|home| home.screen)
            })
            .unwrap_or(OutputRole::Center);
        let identity = self.identify(key);
        let windows: Vec<_> = self
            .snapshot
            .windows
            .iter()
            .filter(|window| self.identify(&window.app).key == identity.key)
            .cloned()
            .collect();
        let pinned = self
            .config
            .favorites
            .iter()
            .any(|fav| self.identify(fav).key == identity.key);
        let here = windows.iter().any(|window| window.screen == screen);
        let indicator = match (windows.is_empty(), here) {
            (true, _) => crate::model::bar::Indicator::None,
            (false, true) => crate::model::bar::Indicator::Here,
            (false, false) => crate::model::bar::Indicator::Elsewhere,
        };
        Some(Entry {
            identity,
            pinned,
            windows,
            indicator,
            subtitle: String::new(),
            habit: None,
        })
    }

    /// Whether the launcher covers bar `id`, drawing it again over itself.
    fn covered(&self, id: window::Id) -> bool {
        let screen = self.bar(id).map(|bar| bar.screen);
        self.drawer
            .as_ref()
            .is_some_and(|drawer| Some(drawer.screen) == screen)
    }

    /// What a menu of bar `id` hangs from, and next to what: `rect` is in
    /// the surface of the bar. While the launcher covers the bar, the menu
    /// hangs from the launcher, at the same place on the screen.
    fn menu_parent(
        &self,
        id: window::Id,
        rect: (i32, i32, i32, i32),
    ) -> (window::Id, (i32, i32, i32, i32)) {
        let drawer = self.drawer.as_ref().filter(|_| self.covered(id));
        let (Some(drawer), Some(bar), Some(screen)) = (drawer, self.bar(id), self.drawer_screen())
        else {
            return (id, rect);
        };
        let (left, top) = crate::model::tray::surface_origin(
            bar.orientation,
            (screen.width as i32, screen.height as i32),
            (bar.size.0 as i32, bar.size.1 as i32),
        );
        let (x, y, width, height) = rect;
        (drawer.id, (left + x, top + y, width, height))
    }

    /// Closes the launcher when it covers bar `id`: what was clicked on the
    /// bar it shows is what the user went for.
    fn leave_drawer_of(&mut self, id: window::Id) -> Task<Message> {
        if self.covered(id) {
            self.update(Message::CloseDrawer)
        } else {
            Task::none()
        }
    }

    /// Likewise for a click on the tray item at `address`, from whichever
    /// bar it was made on.
    fn leave_drawer_for_tray(&mut self, address: &str) -> Task<Message> {
        let index = self.tray.iter().position(|item| item.address == address);
        let clicked = self
            .bars
            .iter()
            .find(|bar| index.is_some() && bar.hover_tray == index)
            .map(|bar| bar.id);
        match clicked {
            Some(id) => self.leave_drawer_of(id),
            None => Task::none(),
        }
    }

    /// The launcher of `screen` is gone without the pointer leaving the bar
    /// it showed: that bar shows no icon under the pointer any more.
    fn forget_drawer_pointer(&mut self, screen: OutputRole) {
        if let Some(bar) = self.bars.iter_mut().find(|bar| bar.screen == screen) {
            bar.hover_entry = None;
            bar.hover_tray = None;
        }
        if let Some(drag) = self.drag.take() {
            if drag.moved {
                self.save_favorites();
            }
        }
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
            bar.unfold > 0.5 && !self.covered(id),
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
    /// plugged in say: a lone screen always has one in classic mode, and every
    /// screen has one by default in spatial mode, unless turned off.
    fn docks(&self) -> Vec<(OutputRole, Orientation)> {
        let mode = if self.is_spatial() {
            ShellMode::Spatial
        } else {
            ShellMode::Classic
        };
        self.config.docks_in_mode(&self.present_screens(), mode)
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
        let now = Instant::now();
        let pace = self.pace();
        for (bar, entries) in self.bars.iter_mut().zip(lists) {
            if pace.enabled {
                let known = |key: &str| bar.entries.iter().any(|entry| entry.identity.key == key);
                let new: Vec<String> = entries
                    .iter()
                    .map(|entry| entry.identity.key.clone())
                    .filter(|key| !known(key))
                    .collect();
                bar.arrivals.extend(new.into_iter().map(|key| (key, now)));
            }
            bar.entries = entries;
        }
        self.watch_launches(now);
    }

    /// How the dock moves now.
    pub fn pace(&self) -> Pace {
        Pace::new(self.config.animations, self.options.slowdown)
    }

    /// The windows the entry `key` has, on any bar; `None` without one.
    fn windows_of(&self, key: &str) -> Option<usize> {
        self.bars
            .iter()
            .flat_map(|bar| &bar.entries)
            .find(|entry| entry.identity.key == key)
            .map(|entry| entry.windows.len())
    }

    /// An application was started: its icon hops until its window comes.
    fn launched(&mut self, key: &str) {
        let Some(windows) = self.windows_of(key) else {
            return;
        };
        let now = Instant::now();
        let pace = self.pace();
        self.launches.retain(|launch| launch.key != key);
        let launch = Launch::new(key.to_owned(), windows, now, pace);
        if !launch.over(now) {
            self.moment = now;
            self.launches.push(launch);
        }
    }

    /// The icons that hop land once their window came, or it is too late.
    fn watch_launches(&mut self, now: Instant) {
        let counts: Vec<usize> = self
            .launches
            .iter()
            .map(|launch| self.windows_of(&launch.key).unwrap_or(usize::MAX))
            .collect();
        for (launch, windows) in self.launches.iter_mut().zip(counts) {
            launch.watch(windows, now);
        }
        self.launches.retain(|launch| !launch.over(now));
    }

    /// Whether anything moves at `now`: frames are then needed.
    fn animating(&self, now: Instant) -> bool {
        let pace = self.pace();
        let spatial = self.is_spatial();
        self.bars.iter().any(|bar| {
            (!spatial && bar.unfold != unfold_target(bar))
                || bar
                    .arrivals
                    .values()
                    .any(|since| motion::arriving(*since, now, pace))
        }) || self.leaving.is_some()
            || !self.launches.is_empty()
            || self
                .drawer
                .as_ref()
                .is_some_and(|drawer| drawer.reveal.moving(now) || drawer.mixer_reveal.moving(now))
    }

    /// Opens the bar of each screen that must have one, once the dock knows
    /// which output that screen is.
    fn show_bars(&mut self) -> Task<Message> {
        let simulated = !self.options.is_real(Backend::Compositor);
        let spatial = self.is_spatial();
        let mut tasks = Vec::new();
        for (index, (screen, orientation)) in self.docks().into_iter().enumerate() {
            if self.bar_on(screen).is_some() {
                continue;
            }
            // `--output` names the screen of the first dock; a made-up
            // world has no real output to put the others on.
            let named = self.options.output.clone().filter(|_| index == 0);
            let (output, output_name) = match (named, simulated) {
                (Some(name), _) => (OutputOption::OutputName(name.clone()), Some(name)),
                (None, true) if index == 0 => (OutputOption::Active, None),
                (None, true) => continue,
                (None, false) => match self.screen(screen) {
                    Some(known) => (
                        OutputOption::OutputName(known.name.clone()),
                        Some(known.name.clone()),
                    ),
                    // Not known yet: the state of the compositor will tell.
                    None => continue,
                },
            };
            let entries = self.entries_for(screen);
            let pinned = entries.iter().filter(|entry| entry.pinned).count();
            let (orientation, size, anchor, exclusive_zone) = if spatial {
                let side = match orientation {
                    Orientation::Vertical(side) => side,
                    Orientation::Horizontal => Side::Left,
                };
                let anchor = match side {
                    Side::Left => Anchor::Left | Anchor::Top | Anchor::Bottom,
                    Side::Right => Anchor::Right | Anchor::Top | Anchor::Bottom,
                };
                let height = self.screen(screen).map_or(1080, |s| s.height.max(1) as u32);
                let sm = self.spatial_metrics();
                (
                    Orientation::Vertical(side),
                    (sm.surface_width, height),
                    anchor,
                    sm.exclusive_zone,
                )
            } else {
                let metrics = self.metrics();
                let size = metrics.surface_size(
                    orientation,
                    pinned,
                    entries.len() - pinned,
                    self.tray.len(),
                );
                let anchor = match orientation {
                    Orientation::Vertical(Side::Left) => Anchor::Left,
                    Orientation::Vertical(Side::Right) => Anchor::Right,
                    Orientation::Horizontal => Anchor::Bottom,
                };
                (
                    orientation,
                    size,
                    anchor,
                    metrics.exclusive_zone_for(orientation),
                )
            };
            let layer_size = if spatial {
                // Anchored Top | Bottom: height 0 stretches to the full output height.
                Some((size.0, 0))
            } else {
                Some(size)
            };
            let (id, task) = Message::layershell_open(NewLayerShellSettings {
                size: layer_size,
                layer: Layer::Top,
                anchor,
                exclusive_zone: Some(exclusive_zone),
                margin: None,
                keyboard_interactivity: KeyboardInteractivity::None,
                output_option: output,
                events_transparent: false,
                namespace: Some(BAR_NAMESPACE.to_owned()),
            });
            self.bars.push(BarState {
                id,
                screen,
                output_name,
                orientation,
                entries,
                size,
                hovered: false,
                unfold: 0.0,
                hover_entry: None,
                hover_tray: None,
                arrivals: HashMap::new(),
            });
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    /// Maintains a permanent `Layer::Bottom` Spatial Home surface on every
    /// connected screen in spatial mode so empty screens (or screens whose
    /// windows are all minimized) always display the Spatial Launcher.
    fn sync_spatial_homes(&mut self) -> Task<Message> {
        if !self.is_spatial() {
            if self.spatial_homes.is_empty() {
                return Task::none();
            }
            let gone = std::mem::take(&mut self.spatial_homes);
            return Task::batch(gone.into_iter().map(|home| window::close(home.id)));
        }
        let simulated = !self.options.is_real(Backend::Compositor);
        let wanted: Vec<(OutputRole, Option<String>, OutputOption)> = if simulated {
            // In `--mock`, a single active output exists.
            let screen = self.bars.first().map_or(OutputRole::Left, |bar| bar.screen);
            let output = self
                .options
                .output
                .clone()
                .map_or(OutputOption::Active, OutputOption::OutputName);
            vec![(screen, self.options.output.clone(), output)]
        } else {
            self.snapshot
                .screens
                .iter()
                .map(|screen| {
                    (
                        screen.role,
                        Some(screen.name.clone()),
                        OutputOption::OutputName(screen.name.clone()),
                    )
                })
                .collect()
        };
        let (kept, gone): (Vec<SpatialHome>, Vec<SpatialHome>) =
            std::mem::take(&mut self.spatial_homes)
                .into_iter()
                .partition(|home| {
                    wanted
                        .iter()
                        .any(|(role, name, _)| *role == home.screen && *name == home.output_name)
                });
        self.spatial_homes = kept;
        let mut tasks: Vec<Task<Message>> = gone
            .into_iter()
            .map(|home| window::close(home.id))
            .collect();
        for (role, output_name, output) in wanted {
            if self
                .spatial_homes
                .iter()
                .any(|home| home.screen == role && home.output_name == output_name)
            {
                continue;
            }
            let (id, task) = Message::layershell_open(NewLayerShellSettings {
                size: None,
                layer: Layer::Bottom,
                anchor: Anchor::Left | Anchor::Right | Anchor::Top | Anchor::Bottom,
                // Respects the 56 px exclusive zone of the sidebar automatically.
                exclusive_zone: Some(0),
                margin: None,
                keyboard_interactivity: KeyboardInteractivity::OnDemand,
                output_option: output,
                events_transparent: false,
                namespace: Some(HOME_NAMESPACE.to_owned()),
            });
            self.spatial_homes.push(SpatialHome {
                id,
                screen: role,
                output_name,
                query: String::new(),
                page: 0,
                surface: None,
            });
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    fn spatial_page_size(&self, screen: OutputRole, surface: Option<Size>) -> usize {
        let fallback = self.snapshot.screens.iter().find(|s| s.role == screen);
        let width = surface
            .map(|size| size.width)
            .or_else(|| fallback.map(|s| s.width as f32))
            .unwrap_or(1920.0);
        let height = surface
            .map(|size| size.height)
            .or_else(|| fallback.map(|s| s.height as f32))
            .unwrap_or(1080.0);
        crate::model::spatial::SpatialLayout::for_size(width, height).page_size()
    }

    fn close_spatial_popover(&mut self) -> Task<Message> {
        let Some(popover) = self.spatial_popover.take() else {
            return Task::none();
        };
        self.request(Request::Levels(false));
        window::close(popover.id).chain(self.sync_toasts())
    }

    fn toggle_spatial_popover(
        &mut self,
        screen: OutputRole,
        kind: SpatialPopoverKind,
    ) -> Task<Message> {
        let close_menus = self.close_menu().chain(self.close_tray_menu());
        if let Some(popover) = &mut self.spatial_popover {
            if popover.screen == screen {
                if popover.kind == kind {
                    return close_menus.chain(self.close_spatial_popover());
                }
                popover.kind = kind;
                popover.mixer = false;
                popover.confirm = None;
                popover.screenshot_prompt = false;
                popover.note = None;
                self.request(Request::Levels(false));
                return close_menus.chain(self.sync_toasts());
            }
        }
        let close_prev = self.close_spatial_popover();
        let side = self
            .bar_on(screen)
            .map_or(Side::Left, |bar| match bar.orientation {
                Orientation::Vertical(side) => side,
                Orientation::Horizontal => Side::Left,
            });
        let output = self.output_for_bar(screen);
        let (id, open) = Message::layershell_open(NewLayerShellSettings {
            size: None,
            layer: Layer::Overlay,
            anchor: Anchor::Left | Anchor::Right | Anchor::Top | Anchor::Bottom,
            // Respects the 56 px sidebar so the sidebar pill stays directly clickable.
            exclusive_zone: Some(0),
            margin: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            output_option: output,
            events_transparent: false,
            namespace: Some(CONTROLS_NAMESPACE.to_owned()),
        });
        self.spatial_popover = Some(SpatialPopover {
            id,
            screen,
            side,
            kind,
            mixer: false,
            confirm: None,
            screenshot_prompt: false,
            note: None,
        });
        close_menus
            .chain(close_prev)
            .chain(open)
            .chain(self.sync_toasts())
    }

    /// Gives each bar the size its entries need, and limits what takes the
    /// pointer to what shows.
    fn fit_bars(&mut self) -> Task<Message> {
        if self.is_spatial() {
            let sm = self.spatial_metrics();
            let mut tasks = Vec::new();
            for index in 0..self.bars.len() {
                let id = self.bars[index].id;
                let resize = if self.bars[index].size.0 != sm.surface_width {
                    self.bars[index].size.0 = sm.surface_width;
                    Task::done(Message::SizeChange {
                        id,
                        size: (sm.surface_width, 0),
                    })
                } else {
                    Task::none()
                };
                let exclusive = Task::done(Message::ExclusiveZoneChange {
                    id,
                    zone_size: sm.exclusive_zone,
                });
                tasks.push(resize.chain(exclusive).chain(self.input_region(id)));
            }
            return Task::batch(tasks);
        }
        let mut tasks = Vec::new();
        let metrics = self.metrics();
        for index in 0..self.bars.len() {
            let bar = &self.bars[index];
            let (id, pinned, orientation) = (bar.id, bar.pinned(), bar.orientation);
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
                zone_size: metrics.exclusive_zone_for(orientation),
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
        let (x, y, width, height) = if self.is_spatial() {
            let side = match bar.orientation {
                Orientation::Vertical(side) => side,
                Orientation::Horizontal => Side::Left,
            };
            self.spatial_metrics()
                .input_region(side, bar.size.0, bar.size.1.max(1080))
        } else {
            let unfolded = bar.hovered || bar.unfold > 0.0;
            self.metrics()
                .input_region(bar.orientation, bar.size, unfolded)
        };
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
            .map(|drawer| (drawer.width, drawer.height, drawer.resizing));
        let Some((width, height, resizing)) = wanted else {
            return;
        };
        // Without a height of its own, the launcher takes a share of the
        // screen, now that it is known.
        let kept = resizing || self.config.drawer_height.is_some();
        let (width, height) = (
            self.drawer_width(Some(width)) as f32,
            self.drawer_height(kept.then_some(height)) as f32,
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
            Command::MoveToScreen { window, screen } => {
                if let Some(index) = windows.iter().position(|known| known.id == window) {
                    let mut moved = windows.remove(index);
                    moved.screen = screen;
                    moved.minimized = false;
                    windows.insert(0, moved);
                    for (rank, known) in windows.iter_mut().enumerate() {
                        known.active = rank == 0;
                    }
                }
            }
            Command::Minimize { window } => {
                if let Some(known) = windows.iter_mut().find(|known| known.id == window) {
                    known.minimized = true;
                    known.active = false;
                }
            }
            Command::Forget { app } => self.snapshot.habits.retain(|habit| habit.app != app),
            Command::SetTheme { theme } => self.snapshot.theme = theme,
            Command::OpenOn { .. }
            | Command::ToggleLauncher
            | Command::ReloadConfig
            | Command::Screenshot { .. }
            | Command::SetGlass { .. }
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
        let token_changed = config.tasks_token != self.config.tasks_token;
        let foundry_changed = config.foundry_token != self.config.foundry_token;
        self.config = config;
        let mut tasks = vec![self.close_menu()];
        tasks.push(self.drop_stale_bars());
        tasks.push(self.show_bars());
        self.rebuild_entries();
        tasks.push(self.fit_bars());
        tasks.push(self.sync_toasts());
        if token_changed {
            tasks.push(self.reset_daily_tasks_for_config());
        }
        if foundry_changed {
            tasks.push(self.reset_project_tasks_for_config());
        }
        Task::batch(tasks)
    }

    fn reset_daily_tasks_for_config(&mut self) -> Task<Message> {
        self.tasks_fetch_seq += 1;
        self.tasks_fetching = false;
        self.last_tasks_fetch = None;
        if !self.options.is_real(Backend::Compositor) && self.config.tasks_token.is_none() {
            return Task::none();
        }
        self.widgets.daily_tasks.clear();
        if self.config.tasks_token.is_some() {
            self.widgets.daily_tasks_status = DailyTasksStatus::Loading;
            if self.is_spatial() {
                return self.fetch_daily_tasks();
            }
        } else {
            self.widgets.daily_tasks_status = DailyTasksStatus::MissingToken;
        }
        Task::none()
    }

    fn fetch_daily_tasks_if_due(&mut self, interval: Duration) -> Task<Message> {
        if !self.is_spatial() || self.config.tasks_token.is_none() || self.tasks_fetching {
            return Task::none();
        }
        let due = self
            .last_tasks_fetch
            .is_none_or(|last| last.elapsed() >= interval);
        if due {
            self.fetch_daily_tasks()
        } else {
            Task::none()
        }
    }

    fn fetch_daily_tasks(&mut self) -> Task<Message> {
        let Some(token) = self.config.tasks_token.clone() else {
            self.widgets.daily_tasks_status = DailyTasksStatus::MissingToken;
            return Task::none();
        };
        self.tasks_fetch_seq += 1;
        let seq = self.tasks_fetch_seq;
        self.tasks_fetching = true;
        self.last_tasks_fetch = Some(Instant::now());
        if self.widgets.daily_tasks.is_empty() {
            self.widgets.daily_tasks_status = DailyTasksStatus::Loading;
        }
        Task::perform(
            async move {
                match tokio::task::spawn_blocking(move || tasks::fetch(&token)).await {
                    Ok(result) => result,
                    Err(error) => Err(TasksError::Failed(error.to_string())),
                }
            },
            move |result| Message::DailyTasksLoaded(seq, result),
        )
    }

    fn reset_project_tasks_for_config(&mut self) -> Task<Message> {
        self.foundry_fetch_seq += 1;
        self.foundry_fetching = false;
        self.last_foundry_fetch = None;
        if !self.options.is_real(Backend::Compositor) && self.config.foundry_token.is_none() {
            return Task::none();
        }
        self.widgets.project_tasks.clear();
        if self.config.foundry_token.is_some() {
            self.widgets.project_tasks_status = ProjectTasksStatus::Loading;
            if self.is_spatial() {
                return self.fetch_project_tasks();
            }
        } else {
            self.widgets.project_tasks_status = ProjectTasksStatus::MissingToken;
        }
        Task::none()
    }

    fn fetch_project_tasks_if_due(&mut self, interval: Duration) -> Task<Message> {
        if !self.is_spatial() || self.config.foundry_token.is_none() || self.foundry_fetching {
            return Task::none();
        }
        let due = self
            .last_foundry_fetch
            .is_none_or(|last| last.elapsed() >= interval);
        if due {
            self.fetch_project_tasks()
        } else {
            Task::none()
        }
    }

    fn fetch_project_tasks(&mut self) -> Task<Message> {
        let Some(token) = self.config.foundry_token.clone() else {
            self.widgets.project_tasks_status = ProjectTasksStatus::MissingToken;
            return Task::none();
        };
        self.foundry_fetch_seq += 1;
        let seq = self.foundry_fetch_seq;
        self.foundry_fetching = true;
        self.last_foundry_fetch = Some(Instant::now());
        if self.widgets.project_tasks.is_empty() {
            self.widgets.project_tasks_status = ProjectTasksStatus::Loading;
        }
        Task::perform(
            async move {
                match tokio::task::spawn_blocking(move || foundry::fetch(&token)).await {
                    Ok(result) => result,
                    Err(error) => Err(FoundryError::Failed(error.to_string())),
                }
            },
            move |result| Message::ProjectTasksLoaded(seq, result),
        )
    }

    fn fetch_spatial_widgets_if_due(&mut self, interval: Duration) -> Task<Message> {
        Task::batch([
            self.fetch_daily_tasks_if_due(interval),
            self.fetch_project_tasks_if_due(interval),
        ])
    }

    /// Something was installed or removed: the applications and the games
    /// are read again, off the interface.
    fn read_installed(&mut self, installed: u64) -> Task<Message> {
        if installed == self.installed {
            return Task::none();
        }
        self.installed = installed;
        Task::perform(
            tokio::task::spawn_blocking(|| (Catalog::load(), steam::load())),
            move |read| match read {
                Ok((catalog, games)) => Message::Installed(installed, catalog, games),
                Err(_) => Message::Installed(installed, Catalog::default(), Vec::new()),
            },
        )
    }

    /// The applications and the games, as they are now: the drawer lists
    /// them, the bars name and draw them so.
    fn installed_read(
        &mut self,
        installed: u64,
        catalog: Catalog,
        games: Vec<Game>,
    ) -> Task<Message> {
        // Read for something older than what is now installed: the newer
        // reading follows. Empty, the reading failed: the lists stay.
        if installed != self.installed || catalog.apps().is_empty() {
            return Task::none();
        }
        let before = (self.catalog.apps().len(), self.games.len());
        self.catalog = catalog;
        self.games = games;
        self.apps = drawer_apps(&self.catalog);
        info!(
            applications = self.catalog.apps().len(),
            games = self.games.len(),
            ?before,
            "installed applications read again"
        );
        let pinned = self.pinned_in_drawer();
        if let Some(drawer) = &mut self.drawer {
            let pages = drawer::page_count(self.apps.len(), drawer.page_size(pinned));
            drawer.page = drawer.page.min(pages - 1);
            drawer.selected = None;
        }
        // The favorites are told apart by application: read them again.
        let mut tasks = vec![self.reload_config()];
        self.rebuild_entries();
        tasks.push(self.fit_bars());
        tasks.push(self.load_icons());
        Task::batch(tasks)
    }

    /// A bar whose screen lost its dock, or whose dock now lies the other
    /// way, goes; `show_bars` opens the ones that are missing.
    fn drop_stale_bars(&mut self) -> Task<Message> {
        let simulated = !self.options.is_real(Backend::Compositor);
        let docks = self.docks();
        let (kept, gone): (Vec<BarState>, Vec<BarState>) =
            std::mem::take(&mut self.bars).into_iter().partition(|bar| {
                let same_output = simulated
                    || self.options.output.is_some()
                    || self
                        .screen(bar.screen)
                        .is_some_and(|s| bar.output_name.as_deref() == Some(s.name.as_str()));
                docks.contains(&(bar.screen, bar.orientation)) && same_output
            });
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

    /// Tells that the battery runs low, as a notification of the dock
    /// itself. The last warning passes « Ne pas déranger ».
    fn warn_battery(&mut self, alert: Alert) -> Task<Message> {
        let Some(level) = self.battery else {
            return Task::none();
        };
        let received = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs() as i64);
        let notice = Notice {
            id: 0,
            app: "Batterie".to_owned(),
            summary: alert.summary(&level),
            body: alert.body().to_owned(),
            icon: Some(NoticeIcon::Named("battery-caution".to_owned())),
            actions: Vec::new(),
            expire_ms: match alert {
                Alert::Low => Some(8000),
                Alert::Critical => None,
            },
            image: None,
            received,
            desktop_entry: None,
            transient: false,
            resident: false,
        };
        self.notify.user_dnd = self.do_not_disturb && alert != Alert::Critical;
        let bars = self.notify_bars();
        let anchored = self.anchored_roles();
        self.notify.ingest(notice, &bars, &anchored);
        self.notify.user_dnd = self.do_not_disturb;
        self.sync_toasts()
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
        let close_popover = self.close_spatial_popover();
        let held = |role| self.screen(role).is_some_and(|screen| screen.anchored);
        let docks: Vec<(OutputRole, bool)> = self
            .bars
            .iter()
            .map(|bar| (bar.screen, held(bar.screen)))
            .collect();
        let every_screen = self.every_screen_held();
        let (screen, orientation) = if self.is_spatial() {
            let screen = asked
                .filter(|role| !held(*role) || every_screen)
                .or_else(|| bar::launcher_screen(&docks, asked, every_screen))
                .or_else(|| self.snapshot.screens.first().map(|s| s.role));
            let Some(screen) = screen else {
                return close_popover;
            };
            // On an empty screen (or one whose windows are all minimized),
            // the permanent `coquille-home` surface is already visible: just
            // focus its search input instead of opening a duplicate overlay.
            if !crate::model::spatial::screen_has_visible_window(&self.snapshot.windows, screen)
                && !held(screen)
            {
                let focus = operation::focus(ui::spatial::launcher::search_id(Some(screen)));
                let refresh = self.fetch_spatial_widgets_if_due(Duration::from_secs(10));
                return Task::batch([close_popover, focus, refresh]);
            }
            let orientation = self
                .bar_on(screen)
                .map_or(Orientation::Vertical(Side::Left), |bar| bar.orientation);
            (screen, orientation)
        } else {
            let picked = bar::launcher_screen(&docks, asked, every_screen)
                .and_then(|screen| self.bar_on(screen));
            let Some((screen, orientation)) = picked.map(|bar| (bar.screen, bar.orientation))
            else {
                return close_popover;
            };
            (screen, orientation)
        };
        let output = self.output_for_bar(screen);
        let exclusive_zone = if self.is_spatial() {
            // Leave the 56 px sidebar uncovered and directly clickable.
            Some(0)
        } else {
            // Classic: over everything, the bar included (drawer draws its own copy).
            Some(-1)
        };
        let (id, task) = Message::layershell_open(NewLayerShellSettings {
            size: None,
            layer: Layer::Overlay,
            anchor: Anchor::Left | Anchor::Right | Anchor::Top | Anchor::Bottom,
            exclusive_zone,
            margin: None,
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            output_option: output,
            events_transparent: false,
            namespace: Some("coquille-drawer".to_owned()),
        });
        // One still going back goes at once.
        let gone = match self.leaving.take() {
            Some(leaving) => window::close(leaving.id),
            None => Task::none(),
        };
        let now = Instant::now();
        self.moment = now;
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
            reveal: Reveal::coming(now, self.pace().of(durations::DRAWER)),
            mixer_reveal: Reveal::at_rest(0.0, now),
        });
        if !self.is_spatial() && orientation == Orientation::Horizontal {
            // Its mixer is always there: its levels move.
            self.request(Request::Levels(true));
        }
        // The control center shows the notices: no toast over it.
        let toasts = self.sync_toasts();
        let refresh = self.fetch_spatial_widgets_if_due(Duration::from_secs(10));
        close_popover
            .chain(gone)
            .chain(self.close_menu())
            .chain(task)
            .chain(toasts)
            .chain(refresh)
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

    /// Whether the launcher shows pinned applications above the grid.
    pub fn pinned_in_drawer(&self) -> bool {
        !self.config.pinned.is_empty()
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
        self.pending_popup = None;
        match self.menu.take() {
            Some((id, ..)) => {
                self.closing_popups.push(id);
                window::close(id)
            }
            None => Task::none(),
        }
    }

    fn close_tray_menu(&mut self) -> Task<Message> {
        self.pending_popup = None;
        match self.tray_menu.take() {
            Some((id, ..)) => {
                self.closing_popups.push(id);
                window::close(id)
            }
            None => Task::none(),
        }
    }

    /// Opens a popup only once no previous popup is still tearing down, so the
    /// compositor never receives a second `xdg_popup.grab` while the first is alive.
    fn open_popup(&mut self, pending: PendingPopup) -> Task<Message> {
        if !self.closing_popups.is_empty() {
            self.pending_popup = Some(pending);
            return Task::none();
        }
        match pending {
            PendingPopup::Entry {
                settings,
                bar_id,
                key,
            } => {
                let (popup, open) = Message::popup_open(settings);
                self.menu = Some((popup, bar_id, key, None));
                open
            }
            PendingPopup::Tray {
                settings,
                bar_id,
                address,
            } => {
                let (popup, open) = Message::popup_open(settings);
                self.tray_menu = Some((popup, bar_id, address));
                open
            }
            PendingPopup::SpatialApp {
                settings,
                surface_id,
                key,
                entry,
            } => {
                let (popup, open) = Message::popup_open(settings);
                self.menu = Some((popup, surface_id, key, Some(*entry)));
                open
            }
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
        if self.notify.effective_dnd(!anchored.is_empty())
            || self.drawer.is_some()
            || self.spatial_popover.is_some()
        {
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
        let thickness = metrics.thickness(orientation).0.round() as u32;
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
            // Classic: in the corner itself under the centered dock.
            // Spatial: beside the full-height 56 px sidebar.
            exclusive_zone: Some(if self.is_spatial() { 0 } else { -1 }),
            margin: None,
            keyboard_interactivity: KeyboardInteractivity::None,
            output_option: self.output_for_bar(screen),
            events_transparent: false,
            namespace: Some(TOASTS_NAMESPACE.to_owned()),
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
        let Some(notice) = self.notify.notice(id).cloned() else {
            return Task::none();
        };
        self.notify.mark_read(id);
        let default = notice.actions.iter().any(|(key, _)| key == "default");
        if let Some(window) = self.window_of_notice(&notice) {
            if let Some(popover) = &self.spatial_popover {
                self.send(Command::MoveToScreen {
                    window,
                    screen: popover.screen,
                });
            } else {
                self.send(Command::Activate { window });
            }
        }
        let close_pop = self.close_spatial_popover();
        let rest = if default {
            self.invoke_notice(id, "default".into())
        } else {
            self.dismiss_notice(id)
        };
        close_pop.chain(rest)
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
        let apps = drawer::search_with(&self.apps, |app| &app.name, search_terms, query);
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
            Some(command) => {
                spawn(command);
                self.launched(&identity.key);
            }
            None => warn!(name = identity.name, "no command known to start this"),
        }
    }

    /// The launcher goes back behind its bar, and closes once there.
    fn leave(&mut self, mut drawer: Drawer) -> Task<Message> {
        let now = Instant::now();
        let gone = match self.leaving.take() {
            Some(leaving) => window::close(leaving.id),
            None => Task::none(),
        };
        let duration = self.pace().of(durations::DRAWER_CLOSE);
        if duration.is_zero() {
            return gone.chain(window::close(drawer.id));
        }
        drawer.reveal.go(0.0, now, duration);
        self.moment = now;
        self.leaving = Some(drawer);
        gone
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);
        self.tell_glass();
        task
    }

    /// Where the liquid glass of each panel is now: the bars, and the
    /// capsules of the notifications.
    fn glass(&self) -> HashMap<(&'static str, OutputRole), Vec<GlassShape>> {
        let metrics = self.metrics();
        let mut glass = HashMap::new();
        let spatial = self.is_spatial();
        for bar in &self.bars {
            if spatial {
                // Edge-to-edge spatial sidebars use Dual Kawase frosted glass
                // (`Mask::Alpha` via `palette.glass`) with a single inner rim,
                // avoiding the 4-sided specular border of floating liquid glass.
                continue;
            }
            let pinned = bar.pinned();
            let shape = metrics.glass_shape(
                bar.orientation,
                bar.size,
                (pinned, bar.entries.len() - pinned),
                self.tray.len(),
                bar.unfold,
            );
            glass.insert((BAR_NAMESPACE, bar.screen), vec![shape]);
        }
        if let Some(toasts) = &self.toasts {
            let (thickness, radius) = metrics.thickness(toasts.orientation);
            let (x, y, width, _) = toasts.geometry.pile;
            let step = thickness.round() as u32 + crate::model::notify::TOAST_GAP;
            let capsules = (0..toasts.shown as u32)
                .map(|index| GlassShape {
                    x: x as i32,
                    y: (y + index * step) as i32,
                    width: width as i32,
                    height: thickness.round() as i32,
                    radius: radius.round() as i32,
                })
                .collect();
            glass.insert((TOASTS_NAMESPACE, toasts.screen), capsules);
        }
        glass
    }

    /// Tells the compositor what changed in the liquid glass, and that the
    /// panels gone have none.
    fn tell_glass(&mut self) {
        if self.link.is_none() {
            return;
        }
        let glass = self.glass();
        if glass == self.glass_told {
            return;
        }
        let gone: Vec<_> = self
            .glass_told
            .keys()
            .filter(|key| !glass.contains_key(*key))
            .copied()
            .collect();
        for (namespace, screen) in gone {
            self.send(Command::SetGlass {
                namespace: namespace.to_owned(),
                screen,
                shapes: Vec::new(),
            });
        }
        for (&(namespace, screen), shapes) in &glass {
            if self.glass_told.get(&(namespace, screen)) != Some(shapes) {
                if let Some(link) = &self.link {
                    let _ = link.send(Command::SetGlass {
                        namespace: namespace.to_owned(),
                        screen,
                        shapes: shapes.clone(),
                    });
                }
            }
        }
        self.glass_told = glass;
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.now = Local::now();
                self.ticks += 1;
                if !self.options.is_real(Backend::Audio) {
                    self.levels = mock::levels(&self.audio, self.ticks).into_iter().collect();
                }
                self.fetch_spatial_widgets_if_due(Duration::from_secs(60))
            }
            Message::Frame(now) => {
                self.moment = now;
                let elapsed = self
                    .last_frame
                    .replace(now)
                    .map_or(Duration::ZERO, |last| now - last);
                let unfold = self.pace().of(UNFOLD);
                let step = if unfold.is_zero() {
                    1.0
                } else {
                    elapsed.as_secs_f32() / unfold.as_secs_f32()
                };
                let pace = self.pace();
                for bar in &mut self.bars {
                    bar.arrivals
                        .retain(|_, since| motion::arriving(*since, now, pace));
                }
                self.watch_launches(now);
                let mut tasks = Vec::new();
                if self
                    .leaving
                    .as_ref()
                    .is_some_and(|drawer| !drawer.reveal.moving(now))
                {
                    if let Some(drawer) = self.leaving.take() {
                        tasks.push(window::close(drawer.id));
                    }
                }
                let mut settled = Vec::new();
                if !self.is_spatial() {
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
                } else {
                    self.last_frame = None;
                }
                // Folded again: the windows beside the bar get the pointer
                // back.
                tasks.extend(settled.into_iter().map(|id| self.input_region(id)));
                Task::batch(tasks)
            }
            Message::Compositor(update) => self.compositor(update),
            Message::Backend(update) => self.backend(update),
            Message::Installed(installed, catalog, games) => {
                self.installed_read(installed, catalog, games)
            }
            Message::IconsLoaded(icons) => {
                // Icons asked for before the theme changed again are stale.
                if icons.style() == Some(self.icon_style) {
                    self.icons = icons;
                }
                Task::none()
            }
            Message::Window(id, window::Event::Opened { size, .. }) => {
                let spatial = self.is_spatial();
                if let Some(bar) = self.bar_mut(id) {
                    if spatial {
                        bar.size = (size.width.round() as u32, size.height.round() as u32);
                    }
                    return self.input_region(id);
                }
                if let Some(home) = self.spatial_homes.iter_mut().find(|home| home.id == id) {
                    home.surface = Some(size);
                    return Task::none();
                }
                if self.drawer.as_ref().is_some_and(|drawer| drawer.id == id) {
                    self.fit_drawer(size);
                    return operation::focus(SEARCH);
                }
                Task::none()
            }
            Message::Window(id, window::Event::Resized(size)) => {
                let spatial = self.is_spatial();
                if let Some(bar) = self.bar_mut(id) {
                    if spatial {
                        bar.size = (size.width.round() as u32, size.height.round() as u32);
                        return self.input_region(id);
                    }
                }
                if let Some(home) = self.spatial_homes.iter_mut().find(|home| home.id == id) {
                    home.surface = Some(size);
                    return Task::none();
                }
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
                        let foot = screen
                            - self.metrics().exclusive_zone_for(Orientation::Horizontal) as f32;
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
                self.leaving.take_if(|drawer| drawer.id == id);
                self.closing_popups.retain(|closing| *closing != id);
                let pending_open = if self.closing_popups.is_empty() {
                    self.pending_popup
                        .take()
                        .map(|pending| self.open_popup(pending))
                        .unwrap_or_else(Task::none)
                } else {
                    Task::none()
                };
                // A click outside the menu closes it without telling more;
                // the bar, kept as it was for it, folds again.
                if let Some((_, bar, ..)) = self.menu.take_if(|(menu, ..)| *menu == id) {
                    if let Some(bar) = self.bar_mut(bar) {
                        bar.hovered = false;
                        bar.hover_entry = None;
                    }
                    return self.input_region(bar).chain(pending_open);
                }
                if let Some((_, bar, _)) = self.tray_menu.take_if(|(menu, ..)| *menu == id) {
                    if let Some(bar) = self.bar_mut(bar) {
                        bar.hover_tray = None;
                    }
                    return self.input_region(bar).chain(pending_open);
                }
                let drawer = self.drawer.as_ref().is_some_and(|drawer| drawer.id == id);
                if drawer {
                    if let Some(drawer) = self.drawer.take() {
                        self.forget_drawer_pointer(drawer.screen);
                    }
                }
                let popover = self
                    .spatial_popover
                    .as_ref()
                    .is_some_and(|popover| popover.id == id);
                if popover {
                    self.spatial_popover = None;
                    self.request(Request::Levels(false));
                }
                self.spatial_homes.retain(|home| home.id != id);
                if self.toasts.as_ref().is_some_and(|surface| surface.id == id) {
                    self.toasts = None;
                }
                self.bars.retain(|bar| bar.id != id);
                if drawer || popover {
                    return self.sync_toasts().chain(pending_open);
                }
                pending_open
            }
            Message::BarHover(id, hovered) => {
                // The menu keeps its bar as it is while it is open.
                let has_menu = self.menu.as_ref().is_some_and(|(_, bar, ..)| *bar == id)
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
                let Some((screen, entry)) = self.bar(id).and_then(|bar| {
                    bar.entries
                        .get(index)
                        .map(|entry| (bar.screen, entry.clone()))
                }) else {
                    return Task::none();
                };
                if self.is_spatial() {
                    let launcher_visible = self.spatial_launcher_visible_on(screen);
                    self.notify.mark_app_read(&entry.identity.names);
                    let close_pop = self.close_spatial_popover();
                    match bar::spatial_left_click(&entry, screen, launcher_visible) {
                        bar::SpatialClick::Launch => {
                            self.send(Command::OpenOn {
                                apps: entry.identity.names.clone(),
                                screen,
                            });
                            if let Some(command) = &entry.identity.command {
                                spawn(command);
                                self.launched(&entry.identity.key);
                            } else {
                                warn!(name = entry.identity.name, "no command to start this");
                            }
                            return close_pop.chain(self.leave_drawer_of(id));
                        }
                        bar::SpatialClick::Activate(window) => {
                            self.send(Command::Activate { window });
                            return close_pop.chain(self.leave_drawer_of(id));
                        }
                        bar::SpatialClick::Teleport(window) => {
                            self.send(Command::MoveToScreen { window, screen });
                            return close_pop.chain(self.leave_drawer_of(id));
                        }
                        bar::SpatialClick::ToggleLauncher => {
                            return close_pop.chain(self.toggle_drawer(Some(screen)));
                        }
                    }
                }
                match bar::left_click(&entry) {
                    Click::Activate(window) => self.send(Command::Activate { window }),
                    Click::Launch => match &entry.identity.command {
                        Some(command) => {
                            spawn(command);
                            self.launched(&entry.identity.key);
                        }
                        None => warn!(name = entry.identity.name, "no command to start this"),
                    },
                }
                self.leave_drawer_of(id)
            }
            Message::EntryMiddle(id, index) => {
                let entry = self.bar(id).and_then(|bar| bar.entries.get(index)).cloned();
                if let Some(entry) = entry {
                    if self.is_spatial() {
                        if let Some(window) = entry.windows.first() {
                            self.send(Command::Close { window: window.id });
                        }
                    } else if let Some(command) = &entry.identity.command {
                        spawn(command);
                        self.launched(&entry.identity.key);
                    }
                }
                self.leave_drawer_of(id)
            }
            Message::EntryMenu(id, index) => {
                let Some(bar) = self.bar(id) else {
                    return Task::none();
                };
                let Some(entry) = bar.entries.get(index) else {
                    return Task::none();
                };
                let mut menu = bar::menu(entry, &self.present_screens());
                if self.is_spatial() {
                    menu.actions
                        .retain(|action| !matches!(action, MenuAction::Forget));
                }
                let size = (ui::menu::WIDTH, ui::menu::height(&menu));
                let rect = if self.is_spatial() {
                    let side = match bar.orientation {
                        Orientation::Vertical(side) => side,
                        Orientation::Horizontal => Side::Left,
                    };
                    self.spatial_metrics()
                        .entry_menu_rect(side, bar.size.0, index, bar.pinned())
                } else {
                    let unfolded = bar.unfold > 0.5 && !self.covered(id);
                    self.metrics()
                        .entry_rect(bar.orientation, index, bar.pinned(), unfolded)
                };
                let (parent, anchor) = self.menu_parent(id, rect);
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
                let settings = IcedNewPopupSettings::new(parent, size, anchor)
                    .anchor(corner)
                    .gravity(grows);
                let key = entry.identity.key.clone();
                let close = self.close_menu().chain(self.close_tray_menu());
                let open = self.open_popup(PendingPopup::Entry {
                    settings,
                    bar_id: id,
                    key,
                });
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
                let leave = self.leave_drawer_for_tray(&address);
                self.request(backend::Request::TrayActivate { address, x, y });
                leave
            }
            Message::TrayMiddle(address) => {
                let (x, y) = self.tray_pointed(&address);
                let leave = self.leave_drawer_for_tray(&address);
                self.request(backend::Request::TraySecondary { address, x, y });
                leave
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
                    // The item's own menu opens under the launcher.
                    let leave = self.leave_drawer_of(id);
                    return self.close_tray_menu().chain(self.close_menu()).chain(leave);
                }
                let height = ui::tray_menu::height(item);
                let size = (ui::tray_menu::WIDTH, height);
                let rect = if self.is_spatial() {
                    let side = match bar.orientation {
                        Orientation::Vertical(side) => side,
                        Orientation::Horizontal => Side::Left,
                    };
                    self.spatial_metrics().tray_menu_rect(
                        side,
                        bar.size.0,
                        bar.size.1.max(1080),
                        self.tray.len(),
                        index,
                    )
                } else {
                    let unfolded = bar.unfold > 0.5 && !self.covered(id);
                    let pinned = bar.pinned();
                    let others = bar.entries.len() - pinned;
                    self.metrics().tray_rect(
                        bar.orientation,
                        index,
                        pinned,
                        others,
                        self.tray.len(),
                        unfolded,
                    )
                };
                let (parent, anchor) = self.menu_parent(id, rect);
                let (corner, grows) = match bar.orientation {
                    Orientation::Vertical(Side::Left) => {
                        (PopupAnchor::TopRight, PopupGravity::BottomRight)
                    }
                    Orientation::Vertical(Side::Right) => {
                        (PopupAnchor::TopLeft, PopupGravity::BottomLeft)
                    }
                    Orientation::Horizontal => (PopupAnchor::Top, PopupGravity::Top),
                };
                let settings = IcedNewPopupSettings::new(parent, size, anchor)
                    .anchor(corner)
                    .gravity(grows);
                let address = item.address.clone();
                let close = self.close_tray_menu().chain(self.close_menu());
                let open = self.open_popup(PendingPopup::Tray {
                    settings,
                    bar_id: id,
                    address,
                });
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
                let Some((_, parent, key, cached)) = self.menu.clone() else {
                    return Task::none();
                };
                let Some(entry) = cached.or_else(|| self.entry_for_menu(parent, &key)) else {
                    return self.close_menu();
                };
                // What starts or shows an application leaves the launcher.
                let leaves = matches!(
                    action,
                    MenuAction::OpenOn(_) | MenuAction::Run { .. } | MenuAction::NewWindow
                );
                match action {
                    MenuAction::OpenOn(screen) => {
                        if self.is_spatial() {
                            if let Some(window) = entry.windows.first() {
                                self.send(Command::MoveToScreen {
                                    window: window.id,
                                    screen,
                                });
                            } else {
                                self.send(Command::OpenOn {
                                    apps: entry.identity.names.clone(),
                                    screen,
                                });
                                if let Some(command) = &entry.identity.command {
                                    spawn(command);
                                }
                            }
                        } else {
                            self.send(Command::OpenOn {
                                apps: entry.identity.names.clone(),
                                screen,
                            });
                            if let Some(command) = &entry.identity.command {
                                spawn(command);
                            }
                        }
                    }
                    MenuAction::Run { command, .. } => spawn(&command),
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
                let leave = if leaves {
                    if self.drawer.is_some() {
                        self.update(Message::CloseDrawer)
                    } else {
                        self.leave_drawer_of(parent)
                    }
                } else {
                    Task::none()
                };
                if let Some(bar) = self.bar_mut(parent) {
                    bar.hovered = false;
                }
                Task::batch([self.close_menu(), self.fit_bars(), leave])
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
                    let size = drawer.page_size(self.pinned_in_drawer());
                    let apps = drawer::page(&self.apps, drawer.page, size);
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
                let pinned = self.pinned_in_drawer();
                if let Some(drawer) = &mut self.drawer {
                    let pages = drawer::page_count(self.apps.len(), drawer.page_size(pinned));
                    drawer.page = page.min(pages - 1);
                    drawer.selected = None;
                }
                Task::none()
            }
            Message::TurnPage(forward) => {
                let pinned = self.pinned_in_drawer();
                if let Some(drawer) = &mut self.drawer {
                    let size = drawer.page_size(pinned);
                    drawer.page = drawer::turn_page(drawer.page, self.apps.len(), size, forward);
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
                    self.forget_drawer_pointer(drawer.screen);
                    // Its menus go with it.
                    let menus = self.close_menu().chain(self.close_tray_menu());
                    menus.chain(self.leave(drawer)).chain(self.sync_toasts())
                }
                None => Task::none(),
            },
            Message::ToggleMixer => {
                if let Some(popover) = &mut self.spatial_popover {
                    popover.mixer = !popover.mixer;
                    let shown = popover.mixer;
                    self.request(Request::Levels(shown));
                    return Task::none();
                }
                let pace = self.pace();
                let Some(drawer) = &mut self.drawer else {
                    return Task::none();
                };
                drawer.mixer = !drawer.mixer;
                let now = Instant::now();
                drawer.mixer_reveal = if drawer.mixer {
                    Reveal::coming(now, pace.of(durations::MIXER))
                } else {
                    Reveal::at_rest(0.0, now)
                };
                self.moment = now;
                let shown = drawer.mixer_shown();
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
                if let Some(popover) = &mut self.spatial_popover {
                    popover.confirm = Some(action);
                    popover.note = None;
                }
                Task::none()
            }
            Message::Confirm(confirmed) => {
                let action = self
                    .drawer
                    .as_mut()
                    .and_then(|drawer| drawer.confirm.take())
                    .or_else(|| {
                        self.spatial_popover
                            .as_mut()
                            .and_then(|popover| popover.confirm.take())
                    });
                let Some(action) = action.filter(|_| confirmed) else {
                    return Task::none();
                };
                if !self.options.is_real(Backend::Session) {
                    info!(?action, "session action simulated: nothing was done");
                    let msg = format!("« {} » : simulé, rien n'a été fait.", action.verb());
                    if let Some(drawer) = &mut self.drawer {
                        drawer.note = Some(msg.clone());
                    }
                    if let Some(popover) = &mut self.spatial_popover {
                        popover.note = Some(msg);
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
                    drawer.note = Some(why.clone());
                }
                if let Some(popover) = &mut self.spatial_popover {
                    popover.note = Some(why);
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
                if let Some(popover) = &mut self.spatial_popover {
                    popover.screenshot_prompt = !popover.screenshot_prompt;
                    popover.confirm = None;
                }
                Task::none()
            }
            Message::CancelScreenshot => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.screenshot_prompt = false;
                }
                if let Some(popover) = &mut self.spatial_popover {
                    popover.screenshot_prompt = false;
                }
                Task::none()
            }
            Message::Screenshot(target) => {
                if let Some(drawer) = &mut self.drawer {
                    drawer.screenshot_prompt = false;
                }
                if let Some(popover) = &mut self.spatial_popover {
                    popover.screenshot_prompt = false;
                }
                self.send(Command::Screenshot { target });
                let close_pop = self.close_spatial_popover();
                close_pop.chain(self.update(Message::CloseDrawer))
            }
            Message::Settings => {
                spawn(&settings_command(&self.options.config));
                let close_pop = self.close_spatial_popover();
                close_pop.chain(self.update(Message::CloseDrawer))
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
            Message::SpatialPopoverToggle(screen, kind) => {
                self.toggle_spatial_popover(screen, kind)
            }
            Message::CloseSpatialPopover => self.close_spatial_popover(),
            Message::SpatialQuery(target, query) => {
                match target {
                    None => {
                        if let Some(drawer) = &mut self.drawer {
                            drawer.query = query;
                            drawer.page = 0;
                        }
                    }
                    Some(screen) => {
                        if let Some(home) = self
                            .spatial_homes
                            .iter_mut()
                            .find(|home| home.screen == screen)
                        {
                            home.query = query;
                            home.page = 0;
                        }
                    }
                }
                Task::none()
            }
            Message::SpatialPage(target, page) => {
                let (screen, query, surface) = match target {
                    None => match &self.drawer {
                        Some(drawer) => (drawer.screen, drawer.query.as_str(), drawer.surface),
                        None => return Task::none(),
                    },
                    Some(screen) => {
                        match self.spatial_homes.iter().find(|home| home.screen == screen) {
                            Some(home) => (screen, home.query.as_str(), home.surface),
                            None => return Task::none(),
                        }
                    }
                };
                let count = crate::model::spatial::filter_apps(&self.apps, query).len();
                let size = self.spatial_page_size(screen, surface);
                let pages = drawer::page_count(count, size);
                let clamped = page.min(pages - 1);
                match target {
                    None => {
                        if let Some(drawer) = &mut self.drawer {
                            drawer.page = clamped;
                        }
                    }
                    Some(screen) => {
                        if let Some(home) = self
                            .spatial_homes
                            .iter_mut()
                            .find(|home| home.screen == screen)
                        {
                            home.page = clamped;
                        }
                    }
                }
                Task::none()
            }
            Message::SpatialCardPage(card, page) => {
                match card {
                    crate::ui::spatial::cards::CardKind::Games => self.games_page = page,
                    crate::ui::spatial::cards::CardKind::Calendar => self.calendar_page = page,
                }
                self.editing_game = None;
                Task::none()
            }
            Message::SpatialTurnPage(target, forward) => {
                let (screen, query, current, surface) = match target {
                    None => match &self.drawer {
                        Some(drawer) => (
                            drawer.screen,
                            drawer.query.as_str(),
                            drawer.page,
                            drawer.surface,
                        ),
                        None => return Task::none(),
                    },
                    Some(screen) => {
                        match self.spatial_homes.iter().find(|home| home.screen == screen) {
                            Some(home) => (screen, home.query.as_str(), home.page, home.surface),
                            None => return Task::none(),
                        }
                    }
                };
                let count = crate::model::spatial::filter_apps(&self.apps, query).len();
                let size = self.spatial_page_size(screen, surface);
                let next = drawer::turn_page(current, count, size, forward);
                match target {
                    None => {
                        if let Some(drawer) = &mut self.drawer {
                            drawer.page = next;
                        }
                    }
                    Some(screen) => {
                        if let Some(home) = self
                            .spatial_homes
                            .iter_mut()
                            .find(|home| home.screen == screen)
                        {
                            home.page = next;
                        }
                    }
                }
                Task::none()
            }
            Message::SpatialSubmit(target) => {
                let (screen, query) = match target {
                    None => match &self.drawer {
                        Some(drawer) => (drawer.screen, drawer.query.clone()),
                        None => return Task::none(),
                    },
                    Some(screen) => {
                        match self.spatial_homes.iter().find(|home| home.screen == screen) {
                            Some(home) => (screen, home.query.clone()),
                            None => return Task::none(),
                        }
                    }
                };
                let filtered = crate::model::spatial::filter_apps(&self.apps, &query);
                let Some(first) = filtered.first() else {
                    return Task::none();
                };
                let app_id = first.id.clone();
                // Clear search field after launching.
                match target {
                    None => {
                        if let Some(drawer) = &mut self.drawer {
                            drawer.query.clear();
                            drawer.page = 0;
                        }
                    }
                    Some(screen) => {
                        if let Some(home) = self
                            .spatial_homes
                            .iter_mut()
                            .find(|home| home.screen == screen)
                        {
                            home.query.clear();
                            home.page = 0;
                        }
                    }
                }
                self.update(Message::SpatialLaunchApp(screen, app_id))
            }
            Message::SpatialLaunchApp(screen, key) => {
                let identity = self.identify(&key);
                self.notify.mark_app_read(&identity.names);
                let existing = self.snapshot.windows.iter().find(|window| {
                    identity
                        .names
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(&window.app))
                });
                if let Some(window) = existing {
                    if window.screen == screen {
                        self.send(Command::Activate { window: window.id });
                    } else {
                        self.send(Command::MoveToScreen {
                            window: window.id,
                            screen,
                        });
                    }
                } else {
                    self.send(Command::OpenOn {
                        apps: identity.names.clone(),
                        screen,
                    });
                    if let Some(command) = &identity.command {
                        spawn(command);
                        self.launched(&identity.key);
                    } else {
                        warn!(name = identity.name, "no command known to start this");
                    }
                }
                let close_pop = self.close_spatial_popover();
                close_pop.chain(self.update(Message::CloseDrawer))
            }
            Message::SpatialAppMenu(surface_id, _screen, key) => {
                let Some(entry) = self.entry_for_menu(surface_id, &key) else {
                    return Task::none();
                };
                let mut menu = bar::menu(&entry, &self.present_screens());
                menu.actions
                    .retain(|action| !matches!(action, MenuAction::Forget));
                let size = (ui::menu::WIDTH, ui::menu::height(&menu));
                let anchor = (200, 320, 1, 1);
                let settings = IcedNewPopupSettings::new(surface_id, size, anchor)
                    .anchor(PopupAnchor::TopLeft)
                    .gravity(PopupGravity::BottomRight);
                let close = self.close_menu().chain(self.close_tray_menu());
                let entry_key = entry.identity.key.clone();
                let open = self.open_popup(PendingPopup::SpatialApp {
                    settings,
                    surface_id,
                    key: entry_key,
                    entry: Box::new(entry),
                });
                close.chain(open)
            }
            Message::DailyTaskToggle(id) => {
                self.widgets.toggle_daily_task(&id);
                Task::none()
            }
            Message::GameNoteEdit(key, current) => {
                self.editing_game = Some((key, current));
                operation::focus(ui::spatial::cards::GAME_NOTE_INPUT)
            }
            Message::GameNoteDraft(text) => {
                if let Some((_, draft)) = &mut self.editing_game {
                    *draft = text;
                }
                Task::none()
            }
            Message::GameNoteSave => {
                if let Some((key, draft)) = self.editing_game.take() {
                    self.widgets.game_notes.set(key, draft);
                    if let Err(error) = self.widgets.game_notes.save(&self.options.state_dir) {
                        warn!(%error, "cannot save game notes");
                    }
                }
                Task::none()
            }
            Message::GameNoteCancel => {
                self.editing_game = None;
                Task::none()
            }
            Message::SpatialLaunchGame(key) => {
                let games = self.widgets.started_games(
                    &self.games,
                    &self.playtimes,
                    self.now.timestamp(),
                    true,
                );
                if let Some(game) = games.into_iter().find(|g| g.key == key) {
                    if let Some(id) = game.steam_id {
                        self.send(Command::OpenOn {
                            apps: vec![format!("steam_app_{id}")],
                            screen: OutputRole::Center,
                        });
                    }
                    if let Some(command) = &game.command {
                        spawn(command);
                    }
                }
                let close_pop = self.close_spatial_popover();
                close_pop.chain(self.update(Message::CloseDrawer))
            }
            Message::RefreshDailyTasks => self.fetch_daily_tasks(),
            Message::DailyTasksLoaded(seq, result) => {
                if seq != self.tasks_fetch_seq {
                    return Task::none();
                }
                self.tasks_fetching = false;
                match result {
                    Ok(items) => {
                        self.widgets.daily_tasks = items;
                        self.widgets.daily_tasks_status = DailyTasksStatus::Ready;
                    }
                    Err(error) => {
                        warn!(?error, "failed to load daily tasks");
                        if matches!(error, TasksError::Unauthorized) {
                            self.widgets.daily_tasks.clear();
                        }
                        self.widgets.daily_tasks_status =
                            DailyTasksStatus::Error(error.user_message().to_owned());
                    }
                }
                Task::none()
            }
            Message::RefreshProjectTasks => self.fetch_project_tasks(),
            Message::ProjectTasksLoaded(seq, result) => {
                if seq != self.foundry_fetch_seq {
                    return Task::none();
                }
                self.foundry_fetching = false;
                match result {
                    Ok(items) => {
                        self.widgets.project_tasks = items;
                        self.widgets.project_tasks_status = ProjectTasksStatus::Ready;
                    }
                    Err(error) => {
                        warn!(?error, "failed to load Kama-Foundry tickets");
                        if matches!(error, FoundryError::Unauthorized) {
                            self.widgets.project_tasks.clear();
                        }
                        self.widgets.project_tasks_status =
                            ProjectTasksStatus::Error(error.user_message().to_owned());
                    }
                }
                Task::none()
            }
            Message::OpenProjectTask(url) | Message::OpenExternalUrl(url) => {
                if let Err(error) = std::process::Command::new("xdg-open").arg(&url).spawn() {
                    warn!(%url, %error, "failed to open URL");
                }
                let close_pop = self.close_spatial_popover();
                close_pop.chain(self.update(Message::CloseDrawer))
            }
            // The messages added for the layer shell are handled by it.
            _ => Task::none(),
        }
    }

    fn compositor(&mut self, update: compositor::Update) -> Task<Message> {
        match update {
            compositor::Update::Connected(link) => {
                self.link = Some(link);
                // A new compositor knows none of it.
                self.glass_told.clear();
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
                let prev_focused = self
                    .snapshot
                    .windows
                    .iter()
                    .find(|w| w.active && !w.minimized)
                    .map(|w| w.id);
                self.snapshot = snapshot;
                // Mark notifications read when an app's window gains focus.
                if let Some(focused) = self
                    .snapshot
                    .windows
                    .iter()
                    .find(|w| w.active && !w.minimized)
                {
                    if Some(focused.id) != prev_focused {
                        let names = self.identify(&focused.app).names;
                        self.notify.mark_app_read(&names);
                    }
                }
                // The screens are known: a lone one gets its dock.
                let stale = self.drop_stale_bars();
                let show = Task::batch([stale, self.show_bars(), self.sync_spatial_homes()]);
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
                let pop_held = self
                    .spatial_popover
                    .as_ref()
                    .and_then(|p| self.screen(p.screen))
                    .is_some_and(|s| s.anchored);
                let close_pop = if pop_held && elsewhere {
                    self.close_spatial_popover()
                } else {
                    Task::none()
                };
                let icons = self.follow_icon_style();
                let toasts = self.sync_toasts();
                Task::batch([close, close_pop, show, self.fit_bars(), icons, toasts])
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
            backend::Update::Battery(now) => {
                let before = std::mem::replace(&mut self.battery, now);
                // The bar grows or shrinks by the line of the battery.
                let mut tasks = vec![self.fit_bars()];
                if let Some(alert) = now.and_then(|now| battery::alert(before, now)) {
                    tasks.push(self.warn_battery(alert));
                }
                return Task::batch(tasks);
            }
            backend::Update::Installed(installed) => return self.read_installed(installed),
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
        if named == Named::Escape {
            if self.editing_game.is_some() {
                return self.update(Message::GameNoteCancel);
            }
            if let Some(popover) = &self.spatial_popover {
                if popover.confirm.is_some() {
                    return self.update(Message::Confirm(false));
                }
                return self.update(Message::CloseSpatialPopover);
            }
        }
        let Some(open) = &self.drawer else {
            return Task::none();
        };
        let step = match named {
            Named::Escape if open.confirm.is_some() => return self.update(Message::Confirm(false)),
            Named::Escape => return self.update(Message::CloseDrawer),
            Named::PageDown if self.is_spatial() => {
                return self.update(Message::SpatialTurnPage(None, true));
            }
            Named::PageUp if self.is_spatial() => {
                return self.update(Message::SpatialTurnPage(None, false));
            }
            Named::PageDown => return self.update(Message::TurnPage(true)),
            Named::PageUp => return self.update(Message::TurnPage(false)),
            Named::ArrowLeft => Step::Left,
            Named::ArrowRight => Step::Right,
            Named::ArrowUp => Step::Up,
            Named::ArrowDown => Step::Down,
            _ => return Task::none(),
        };
        let (count, columns) = if open.query.trim().is_empty() {
            let pinned = self.pinned_in_drawer();
            let page = drawer::page(&self.apps, open.page, open.page_size(pinned));
            (page.len(), open.app_columns(pinned))
        } else {
            let shown = open.results_shown();
            (
                self.hits(&open.query).len().min(shown),
                open.result_columns(),
            )
        };
        if let Some(open) = &mut self.drawer {
            open.selected = drawer::step(open.selected, count, columns, step);
        }
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = vec![
            window::events().map(|(id, event)| Message::Window(id, event)),
            window::close_events().map(Message::Closed),
        ];
        // The clock; with the mixer open, the made-up levels move faster.
        let simulated_levels = !self.options.is_real(Backend::Audio)
            && (self.drawer.as_ref().is_some_and(Drawer::mixer_shown)
                || self.spatial_popover.as_ref().is_some_and(|p| p.mixer));
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

        if self.animating(Instant::now()) {
            subscriptions.push(window::frames().map(Message::Frame));
        }
        if self.drawer.is_some() || self.spatial_popover.is_some() || self.editing_game.is_some() {
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
        if self.options.is_real(Backend::Battery) {
            subscriptions.push(backend::battery::subscription().map(Message::Backend));
        }
        if self.options.is_real(Backend::Notifications) {
            subscriptions.push(backend::notify::subscription().map(Message::Backend));
        }
        subscriptions.push(backend::installed::subscription().map(Message::Backend));
        Subscription::batch(subscriptions)
    }

    pub fn view(&self, id: window::Id) -> Element<'_, Message> {
        let palette = self.palette();
        if let Some(bar) = self.bar(id) {
            if self.is_spatial() {
                return ui::spatial::bar::view(self, bar, palette);
            }
            return ui::bar::view(self, bar);
        }
        if let Some((_, parent, key, cached)) = self.menu.as_ref().filter(|(menu, ..)| *menu == id)
        {
            if let Some(entry) = cached.as_ref() {
                return ui::menu::view(self, entry);
            }
            let entries = self.bar(*parent).map(|bar| bar.entries.as_slice());
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
        if let Some(popover) = self
            .spatial_popover
            .as_ref()
            .filter(|popover| popover.id == id)
        {
            return ui::spatial::controls::view(self, popover, palette);
        }
        if let Some(home) = self.spatial_homes.iter().find(|home| home.id == id) {
            return ui::spatial::launcher::view(
                self,
                home.id,
                home.screen,
                &home.query,
                home.page,
                home.surface,
                ui::spatial::launcher::LauncherMode::Home,
                palette,
            );
        }
        let drawers = self.drawer.iter().chain(&self.leaving);
        if let Some(drawer) = drawers.into_iter().find(|drawer| drawer.id == id) {
            if self.is_spatial() {
                return ui::spatial::launcher::view(
                    self,
                    drawer.id,
                    drawer.screen,
                    &drawer.query,
                    drawer.page,
                    drawer.surface,
                    ui::spatial::launcher::LauncherMode::Overlay,
                    palette,
                );
            }
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
/// What else than its name an application is found by in the drawer: what
/// kind of application it is, and its keywords.
fn search_terms(app: &App) -> Vec<&str> {
    let kind = app.generic_name.as_deref().into_iter();
    kind.chain(app.keywords.iter().map(String::as_str))
        .collect()
}

fn unfold_target(bar: &BarState) -> f32 {
    if bar.hovered && matches!(bar.orientation, Orientation::Vertical(_)) {
        1.0
    } else {
        0.0
    }
}

/// The applications of the drawer, from A to Z. Steam writes a desktop
/// file for each game: those are listed with the games, not here.
fn drawer_apps(catalog: &Catalog) -> Vec<App> {
    let mut apps = catalog.apps().to_vec();
    apps.retain(|app| steam::game_of_command(&app.command).is_none());
    drawer::sort_by_name(&mut apps, |app| &app.name);
    apps
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
