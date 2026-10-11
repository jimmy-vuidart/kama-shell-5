//! The compositor state and its Wayland globals.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use coquille_model::{AppMemory, OutputId, Point as ModelPoint, WindowId};
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{ImportDma, ImportMemWl};
use smithay::desktop::PopupManager;
use smithay::input::keyboard::XkbConfig;
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus};
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::utils::{IsAlive, Logical, Point};
use smithay::wayland::compositor::{CompositorClientState, CompositorState, with_states};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;
use tracing::{info, warn};

use crate::backend::Backend;
use crate::cursor::CursorImages;
use crate::decoration::layout::TitlebarButton;
use crate::decoration::paint::Painter;
use crate::input::Gesture;
use crate::keyboard_layout::KeyboardLayout;
use crate::placement::Bar;
use crate::render::scene::Cursor;
use crate::shell::{ScreenInfo, Shell};
use crate::shortcuts::ModKey;
use crate::theme::Theme;

/// What event sources get: the display must stay outside the state it
/// dispatches to.
pub struct App {
    pub display: Display<State>,
    pub state: State,
}

pub struct Options {
    pub theme: Theme,
    pub keyboard: KeyboardLayout,
    pub mod_key: ModKey,
    /// Command of the terminal opened by the shortcut.
    pub terminal: String,
    /// Where habits are kept between sessions; `None` keeps nothing.
    pub state_dir: Option<PathBuf>,
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
    pub loop_handle: LoopHandle<'static, App>,
    pub socket_name: OsString,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<State>,
    pub data_device_state: DataDeviceState,
    pub dmabuf_state: DmabufState,
    pub popups: PopupManager,
    // Kept alive for as long as their globals must exist.
    _dmabuf_global: DmabufGlobal,
    _output_manager_state: OutputManagerState,
    _decoration_state: XdgDecorationState,
    _cursor_shape_state: CursorShapeManagerState,

    pub seat: Seat<State>,
    pub cursor_status: CursorImageStatus,
    pub cursor_images: CursorImages,
    /// Keys whose press triggered a shortcut: their release is swallowed too.
    pub suppressed_keys: Vec<u32>,
    pub mod_key: ModKey,
    pub terminal: String,

    pub outputs: Vec<ScreenOutput>,
    pub backend: Backend,
    pub shell: Shell,
    pub theme: Theme,
    pub painter: Painter,
    /// The title bar button under the pointer.
    pub hover: Option<(WindowId, TitlebarButton)>,
    /// The window, freshly opened floating, the placement bar is offered for.
    pub placement_offer: Option<WindowId>,
    pub bar: Option<Bar>,
    /// The button of the bar under the pointer.
    pub bar_hover: Option<usize>,
    /// What the left mouse button is doing since it went down.
    pub gesture: Option<Gesture>,
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
}

impl State {
    pub fn new(
        display_handle: DisplayHandle,
        loop_signal: LoopSignal,
        loop_handle: LoopHandle<'static, App>,
        socket_name: OsString,
        mut backend: Backend,
        screens: Vec<(ScreenInfo, Output)>,
        options: Options,
    ) -> anyhow::Result<Self> {
        let dh = &display_handle;
        let renderer = backend.renderer();

        let compositor_state = CompositorState::new::<Self>(dh);
        let xdg_shell_state = XdgShellState::new::<Self>(dh);
        let decoration_state = XdgDecorationState::new::<Self>(dh);
        let shm_state = ShmState::new::<Self>(dh, renderer.shm_formats());
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(dh);
        let data_device_state = DataDeviceState::new::<Self>(dh);
        let cursor_shape_state = CursorShapeManagerState::new::<Self>(dh);
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

        Ok(Self {
            start_time: Instant::now(),
            display_handle,
            loop_signal,
            loop_handle,
            socket_name,
            compositor_state,
            xdg_shell_state,
            shm_state,
            seat_state,
            data_device_state,
            dmabuf_state,
            popups: PopupManager::default(),
            _dmabuf_global: dmabuf_global,
            _output_manager_state: output_manager_state,
            _decoration_state: decoration_state,
            _cursor_shape_state: cursor_shape_state,
            seat,
            cursor_status: CursorImageStatus::default_named(),
            cursor_images: CursorImages::load(),
            suppressed_keys: Vec::new(),
            mod_key: options.mod_key,
            terminal: options.terminal,
            outputs,
            backend,
            shell: Shell::new(infos, habits.clone()),
            state_dir: options.state_dir,
            saved_habits: habits,
            theme: options.theme,
            painter: Painter::new(),
            hover: None,
            placement_offer: None,
            bar: None,
            bar_hover: None,
            gesture: None,
            swallowed_press: false,
            redraw_needed: true,
            full_redraw: true,
        })
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
        let child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("WAYLAND_DISPLAY", &self.socket_name)
            // Without this, toolkits may prefer an X server.
            .env_remove("DISPLAY")
            .spawn();
        match child {
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
