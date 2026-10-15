//! The compositor state and its Wayland globals.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use coquille_apps::Catalog;
use coquille_model::{AppMemory, OutputId, OutputRole, Point as ModelPoint, Rect, WindowId};
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{ImportDma, ImportMemWl};
use smithay::desktop::{LayerSurface, PopupManager};
use smithay::input::keyboard::XkbConfig;
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus};
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Logical, Point};
use smithay::wayland::compositor::{CompositorClientState, CompositorState, with_states};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::pointer_constraints::PointerConstraintsState;
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::selection::ext_data_control::DataControlState as ExtDataControlState;
use smithay::wayland::selection::wlr_data_control::DataControlState as WlrDataControlState;
use smithay::wayland::shell::wlr_layer::WlrLayerShellState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::xwayland_shell::XWaylandShellState;
use smithay::xwayland::X11Wm;
use tracing::{info, warn};

use crate::backend::Backend;
use crate::config::{GlassConfig, WallpaperConfig};
use crate::cursor::CursorImages;
use crate::decoration::layout::TitlebarButton;
use crate::decoration::paint::Painter;
use crate::game::{ModTap, StickyEdges};
use crate::input::{Gesture, ScreenshotMode};
use crate::ipc::Ipc;
use crate::keyboard_layout::KeyboardLayout;
use crate::placement::Bar;
use crate::render::glass::GlassCache;
use crate::render::scene::Cursor;
use crate::shell::{CapsuleHit, ScreenInfo, Shell};
use crate::shortcuts::ModKey;
use crate::theme::{Rgba, Theme, ThemeKind};
use crate::wallpaper::{ScreenWallpaper, WallpaperManager};

pub struct Options {
    pub theme: ThemeKind,
    /// The accent the user chose, when it is not the one of the theme.
    pub accent: Option<Rgba>,
    pub keyboard: KeyboardLayout,
    /// The layout comes from the command line: the configuration file does
    /// not change it.
    pub keyboard_pinned: bool,
    /// The configuration file, when it is not the usual one.
    pub config_path: Option<PathBuf>,
    pub mod_key: ModKey,
    /// Command of the terminal opened by the shortcut.
    pub terminal: String,
    /// Where habits are kept between sessions; `None` keeps nothing.
    pub state_dir: Option<PathBuf>,
    /// How long the pointer is held at the edge of a game's screen.
    pub sticky_edges_ms: u64,
    pub glass: GlassConfig,
    pub wallpaper: WallpaperConfig,
}

/// A screen as clients see it.
pub struct ScreenOutput {
    pub id: OutputId,
    pub output: Output,
}

pub struct State {
    pub start_time: Instant,
    pub display_handle: DisplayHandle,
    pub loop_signal: LoopSignal,
    pub loop_handle: LoopHandle<'static, State>,
    pub socket_name: OsString,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub layer_shell_state: WlrLayerShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<State>,
    pub data_device_state: DataDeviceState,
    pub wlr_data_control_state: WlrDataControlState,
    pub ext_data_control_state: ExtDataControlState,
    pub dmabuf_state: DmabufState,
    pub popups: PopupManager,
    pub xwayland_shell_state: XWaylandShellState,
    /// The X11 window manager, once XWayland runs. Never taken out again:
    /// X11 events rely on it.
    pub xwm: Option<X11Wm>,
    /// The number of the X server, for the `DISPLAY` of the programs started
    /// from here.
    pub xdisplay: Option<u32>,
    // Kept alive for as long as their globals must exist.
    _dmabuf_global: DmabufGlobal,
    _output_manager_state: OutputManagerState,
    _decoration_state: XdgDecorationState,
    _cursor_shape_state: CursorShapeManagerState,
    _pointer_constraints_state: PointerConstraintsState,
    _relative_pointer_state: RelativePointerManagerState,
    _viewporter_state: ViewporterState,

    pub seat: Seat<State>,
    pub cursor_status: CursorImageStatus,
    pub cursor_images: CursorImages,
    /// Keys whose press triggered a shortcut: their release is swallowed too.
    pub suppressed_keys: Vec<u32>,
    pub mod_key: ModKey,
    /// Tells the shell modifier tapped alone from its use in a shortcut.
    pub mod_tap: ModTap,
    /// The window the user freed the pointer from with a tap of the shell
    /// modifier: it may not hold the pointer again before the next click.
    pub pointer_freed: Option<WindowId>,
    /// Nested: where the host said its pointer was last, to tell clients
    /// how far it went.
    pub last_absolute: Option<Point<f64, Logical>>,
    /// Holds the pointer on the screen of the game being played.
    pub sticky_edges: StickyEdges,
    pub sticky_edges_ms: u64,
    pub terminal: String,
    /// The layout of the keyboard in use, and whether the configuration
    /// file may change it.
    pub keyboard: KeyboardLayout,
    pub keyboard_pinned: bool,
    /// The configuration file, when it is not the usual one.
    pub config_path: Option<PathBuf>,

    pub outputs: Vec<ScreenOutput>,
    pub backend: Backend,
    pub shell: Shell,
    /// The applications installed, for the names and icons of title bars.
    pub catalog: Catalog,
    pub theme: Theme,
    /// The accent the user chose, kept across changes of theme.
    pub accent: Option<Rgba>,
    pub painter: Painter,
    /// The socket of the shell, once it is open.
    pub ipc: Option<Ipc>,
    /// The title bar button under the pointer.
    pub hover: Option<(WindowId, TitlebarButton)>,
    /// The capsule under the pointer.
    pub capsule_hover: Option<CapsuleHit>,
    /// The window, freshly opened floating, the placement bar is offered for.
    pub placement_offer: Option<WindowId>,
    pub bar: Option<Bar>,
    /// The button of the bar under the pointer.
    pub bar_hover: Option<usize>,
    /// The panel the user clicked to type in: it has the keyboard until a
    /// click elsewhere.
    pub panel_focus: Option<LayerSurface>,
    /// What the left mouse button is doing since it went down.
    pub gesture: Option<Gesture>,
    /// An interactive screenshot selection in progress.
    pub screenshot_mode: Option<ScreenshotMode>,
    /// Pre-captured screen freeze for interactive region screenshot.
    pub screenshot_freeze: Option<crate::screenshot::ScreenshotFreeze>,
    /// The left button went down for the shell: its release is not for a
    /// client either.
    pub swallowed_press: bool,

    /// Where habits are kept between sessions, and what was written last.
    pub state_dir: Option<PathBuf>,
    pub saved_habits: AppMemory,

    /// Something changed: a frame is due.
    pub redraw_needed: bool,
    /// The next frame must be drawn whole, whatever the damage.
    pub full_redraw: bool,
    pub glass: GlassConfig,
    pub panel_glass: HashMap<WlSurface, GlassCache>,
    pub wallpaper_config: WallpaperConfig,
    pub wallpaper_manager: WallpaperManager,
    pub wallpapers: Vec<ScreenWallpaper>,
}

impl State {
    pub fn new(
        display_handle: DisplayHandle,
        loop_signal: LoopSignal,
        loop_handle: LoopHandle<'static, State>,
        socket_name: OsString,
        mut backend: Backend,
        screens: Vec<(ScreenInfo, Output)>,
        options: Options,
    ) -> anyhow::Result<Self> {
        let dh = &display_handle;
        let renderer = backend.renderer();

        let compositor_state = CompositorState::new::<Self>(dh);
        let xdg_shell_state = XdgShellState::new::<Self>(dh);
        // Panels of the shell: the dock and its drawer.
        let layer_shell_state = WlrLayerShellState::new::<Self>(dh);
        let decoration_state = XdgDecorationState::new::<Self>(dh);
        let shm_state = ShmState::new::<Self>(dh, renderer.shm_formats());
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(dh);
        let data_device_state = DataDeviceState::new::<Self>(dh);
        let wlr_data_control_state = WlrDataControlState::new::<Self, _>(dh, None, |_| true);
        let ext_data_control_state = ExtDataControlState::new::<Self, _>(dh, None, |_| true);
        let cursor_shape_state = CursorShapeManagerState::new::<Self>(dh);
        // Only XWayland may use it, to pair X11 windows with their surfaces.
        let xwayland_shell_state = XWaylandShellState::new::<Self>(dh);
        // Games: hold the pointer in place, and read how far the mouse went.
        let pointer_constraints_state = PointerConstraintsState::new::<Self>(dh);
        let relative_pointer_state = RelativePointerManagerState::new::<Self>(dh);
        // Lets a client stretch a small buffer over its surface: how
        // XWayland shows a game that asked for a lower resolution.
        let viewporter_state = ViewporterState::new::<Self>(dh);
        let (dmabuf_state, dmabuf_global) = init_dmabuf(dh, renderer);

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(dh, "seat0");
        let keyboard = &options.keyboard;
        info!(
            layout = keyboard.layout,
            variant = keyboard.variant,
            "keyboard layout"
        );
        seat.add_keyboard(
            XkbConfig {
                layout: &keyboard.layout,
                variant: &keyboard.variant,
                options: keyboard.options.clone(),
                ..XkbConfig::default()
            },
            200,
            25,
        )?;
        seat.add_pointer();

        let mut outputs = Vec::new();
        let mut infos = Vec::new();
        for (info, output) in screens {
            info!(
                name = output.name(),
                role = ?info.role,
                rect = ?info.rect,
                "screen"
            );
            // The global lives as long as the output does.
            let _global = output.create_global::<Self>(dh);
            outputs.push(ScreenOutput {
                id: info.id,
                output,
            });
            infos.push(info);
        }

        let habits = options
            .state_dir
            .as_deref()
            .map(crate::persist::load)
            .unwrap_or_default();
        info!(habits = habits.len(), "habits loaded");

        let mut state = Self {
            start_time: Instant::now(),
            display_handle,
            loop_signal,
            loop_handle,
            socket_name,
            compositor_state,
            xdg_shell_state,
            layer_shell_state,
            shm_state,
            seat_state,
            data_device_state,
            wlr_data_control_state,
            ext_data_control_state,
            dmabuf_state,
            popups: PopupManager::default(),
            xwayland_shell_state,
            xwm: None,
            xdisplay: None,
            _dmabuf_global: dmabuf_global,
            _output_manager_state: output_manager_state,
            _decoration_state: decoration_state,
            _cursor_shape_state: cursor_shape_state,
            _pointer_constraints_state: pointer_constraints_state,
            _relative_pointer_state: relative_pointer_state,
            _viewporter_state: viewporter_state,
            seat,
            cursor_status: CursorImageStatus::default_named(),
            cursor_images: CursorImages::load(),
            suppressed_keys: Vec::new(),
            mod_key: options.mod_key,
            mod_tap: ModTap::default(),
            pointer_freed: None,
            last_absolute: None,
            sticky_edges: StickyEdges::default(),
            sticky_edges_ms: options.sticky_edges_ms,
            terminal: options.terminal,
            keyboard: options.keyboard,
            keyboard_pinned: options.keyboard_pinned,
            config_path: options.config_path,
            outputs,
            backend,
            shell: Shell::new(infos, habits.clone()),
            state_dir: options.state_dir,
            saved_habits: habits,
            theme: match options.accent {
                Some(accent) => Theme::new(options.theme).with_accent(accent),
                None => Theme::new(options.theme),
            },
            accent: options.accent,
            catalog: Catalog::load(),
            painter: Painter::new(),
            ipc: None,
            hover: None,
            capsule_hover: None,
            placement_offer: None,
            bar: None,
            bar_hover: None,
            panel_focus: None,
            gesture: None,
            screenshot_mode: None,
            screenshot_freeze: None,
            swallowed_press: false,
            redraw_needed: true,
            full_redraw: true,
            glass: options.glass,
            panel_glass: HashMap::new(),
            wallpaper_config: options.wallpaper,
            wallpaper_manager: WallpaperManager::new(),
            wallpapers: Vec::new(),
        };
        state.refresh_wallpapers();
        Ok(state)
    }

    /// Re-evaluates wallpapers for all current screens.
    pub fn refresh_wallpapers(&mut self) {
        let screens: Vec<(OutputId, OutputRole, Rect)> = self
            .outputs
            .iter()
            .filter_map(|screen| {
                let info = self.shell.screen(screen.id)?;
                Some((screen.id, info.role, info.rect))
            })
            .collect();
        self.wallpapers =
            self.wallpaper_manager
                .update(&screens, &self.wallpaper_config, self.theme.backdrop);
    }

    /// Active screen wallpapers (omitted on screens anchored by a game).
    pub fn active_wallpapers(
        &self,
    ) -> Vec<(
        smithay::backend::renderer::element::memory::MemoryRenderBuffer,
        Rect,
    )> {
        self.wallpapers
            .iter()
            .filter(|wp| !self.is_anchored(wp.output_id))
            .map(|wp| (wp.buffer.clone(), wp.rect))
            .collect()
    }

    /// Frozen screen buffers when interactive region screenshot is active.
    pub fn active_freeze(
        &self,
    ) -> Vec<(
        smithay::backend::renderer::element::memory::MemoryRenderBuffer,
        Rect,
    )> {
        self.screenshot_freeze
            .as_ref()
            .map(|freeze| freeze.buffers())
            .unwrap_or_default()
    }

    /// Starts interactive region screenshot mode by pre-freezing the screens.
    pub fn start_region_screenshot(&mut self) {
        let freeze = crate::screenshot::freeze_screens(self);
        self.screenshot_freeze = Some(freeze);
        self.screenshot_mode = Some(ScreenshotMode::SelectingRegion);
        self.full_redraw = true;
        self.set_cursor(CursorImageStatus::Named(
            smithay::input::pointer::CursorIcon::Crosshair,
        ));
        self.queue_redraw();
    }

    pub fn output(&self, id: OutputId) -> Option<&Output> {
        self.outputs
            .iter()
            .find(|screen| screen.id == id)
            .map(|screen| &screen.output)
    }

    /// Asks for a frame on every screen.
    pub fn queue_redraw(&mut self) {
        self.redraw_needed = true;
    }

    pub fn pointer_location(&self) -> Point<f64, Logical> {
        self.seat
            .get_pointer()
            .map(|pointer| pointer.current_location())
            .unwrap_or_default()
    }

    /// The screen the pointer is on.
    pub fn pointer_screen(&self) -> Option<ScreenInfo> {
        let location = self.pointer_location();
        let point = ModelPoint::new(location.x.floor() as i32, location.y.floor() as i32);
        self.shell.screen_at(point).copied()
    }

    /// A client (or the shell) changed the pointer image.
    pub fn set_cursor(&mut self, image: CursorImageStatus) {
        if self.cursor_status == image {
            return;
        }
        self.backend.show_host_cursor(&image);
        self.cursor_status = image;
        self.queue_redraw();
    }

    /// What to draw at the pointer. Nested, named cursors are left to the
    /// host; on real hardware (`draw_named`) the compositor draws them.
    pub fn cursor(&mut self, draw_named: bool) -> Option<Cursor> {
        let location = self.pointer_location().to_i32_round();
        match &self.cursor_status {
            CursorImageStatus::Hidden => None,
            CursorImageStatus::Surface(surface) if surface.alive() => {
                let hotspot = with_states(surface, |states| {
                    states
                        .data_map
                        .get::<std::sync::Mutex<CursorImageAttributes>>()
                        .and_then(|attributes| attributes.lock().ok().map(|a| a.hotspot))
                        .unwrap_or_default()
                });
                Some(Cursor::Surface {
                    surface: surface.clone(),
                    location: location - hotspot,
                })
            }
            // A cursor surface that died: back to the default arrow.
            CursorImageStatus::Surface(_) => {
                self.set_cursor(CursorImageStatus::default_named());
                self.cursor(draw_named)
            }
            CursorImageStatus::Named(icon) if draw_named => {
                let sprite = self.cursor_images.get(*icon);
                Some(Cursor::Image {
                    buffer: sprite.buffer.clone(),
                    location: location - Point::from(sprite.hotspot),
                })
            }
            CursorImageStatus::Named(_) => None,
        }
    }

    /// Runs a shell command as a client of this compositor.
    pub fn spawn(&self, command: &str) {
        let mut child = Command::new("sh");
        child
            .arg("-c")
            .arg(command)
            .env("WAYLAND_DISPLAY", &self.socket_name);
        if let Some(ipc) = &self.ipc {
            child.env(coquille_ipc::SOCKET_ENV, &ipc.path);
        }
        match self.xdisplay {
            Some(number) => child.env("DISPLAY", crate::xwayland::display_name(number)),
            // Never the X server of another session.
            None => child.env_remove("DISPLAY"),
        };
        match child.spawn() {
            Ok(child) => info!(command, pid = child.id(), "spawned"),
            Err(error) => warn!(command, %error, "cannot spawn"),
        }
    }
}

/// Lets clients hand over GPU buffers, which every OpenGL or Vulkan client
/// needs.
fn init_dmabuf(dh: &DisplayHandle, renderer: &mut GlesRenderer) -> (DmabufState, DmabufGlobal) {
    let formats = renderer.dmabuf_formats();
    let render_node = EGLDevice::device_for_display(renderer.egl_context().display())
        .and_then(|device| device.try_get_render_node());

    let mut state = DmabufState::new();
    let feedback = match render_node {
        Ok(Some(node)) => DmabufFeedbackBuilder::new(node.dev_id(), formats.clone())
            .build()
            .ok(),
        Ok(None) => None,
        Err(error) => {
            warn!(%error, "cannot find the render node");
            None
        }
    };
    let global = match feedback {
        Some(feedback) => state.create_global_with_default_feedback::<State>(dh, &feedback),
        None => state.create_global::<State>(dh, formats),
    };
    (state, global)
}

/// Data attached to each client connection.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
