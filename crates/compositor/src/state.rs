//! The compositor state and its Wayland globals.

use std::ffi::OsString;
use std::time::Instant;

use coquille_model::{OutputRole, WindowId};
use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::{ImportDma, ImportMemWl};
use smithay::desktop::PopupManager;
use smithay::input::keyboard::XkbConfig;
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatState};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::LoopSignal;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::utils::{Physical, Size, Transform};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::{DmabufFeedbackBuilder, DmabufGlobal, DmabufState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shell::xdg::decoration::XdgDecorationState;
use smithay::wayland::shm::ShmState;
use tracing::{info, warn};

use crate::decoration::layout::TitlebarButton;
use crate::decoration::paint::Painter;
use crate::keyboard_layout::KeyboardLayout;
use crate::shell::Shell;
use crate::shortcuts::ModKey;
use crate::theme::Theme;
use crate::winit::Winit;

/// Refresh rate announced for the nested output, in mHz.
pub const REFRESH: i32 = 60_000;

pub struct Options {
    pub role: OutputRole,
    pub theme: Theme,
    pub keyboard: KeyboardLayout,
}

pub struct State {
    pub start_time: Instant,
    pub display_handle: DisplayHandle,
    pub loop_signal: LoopSignal,
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
    /// Keys whose press triggered a shortcut: their release is swallowed too.
    pub suppressed_keys: Vec<u32>,
    pub mod_key: ModKey,

    pub output: Output,
    pub backend: Winit,
    pub shell: Shell,
    pub theme: Theme,
    pub painter: Painter,
    /// The title bar button under the pointer.
    pub hover: Option<(WindowId, TitlebarButton)>,
    /// The title bar button the left mouse button went down on.
    pub pressed: Option<(WindowId, TitlebarButton)>,
}

impl State {
    pub fn new(
        display_handle: DisplayHandle,
        loop_signal: LoopSignal,
        socket_name: OsString,
        mut backend: Winit,
        options: Options,
    ) -> anyhow::Result<Self> {
        let dh = &display_handle;

        let compositor_state = CompositorState::new::<Self>(dh);
        let xdg_shell_state = XdgShellState::new::<Self>(dh);
        let decoration_state = XdgDecorationState::new::<Self>(dh);
        let shm_state = ShmState::new::<Self>(dh, backend.backend.renderer().shm_formats());
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(dh);
        let data_device_state = DataDeviceState::new::<Self>(dh);
        let cursor_shape_state = CursorShapeManagerState::new::<Self>(dh);
        let (dmabuf_state, dmabuf_global) = init_dmabuf(dh, &mut backend);

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(dh, "winit");
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

        let size = backend.backend.window_size();
        let output = Output::new(
            "winit".to_owned(),
            PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "coquille".into(),
                model: "nested".into(),
                serial_number: "Unknown".into(),
            },
        );
        let _global = output.create_global::<Self>(dh);
        let mode = output_mode(size);
        // The winit backend draws upside down, hence the flip.
        output.change_current_state(
            Some(mode),
            Some(Transform::Flipped180),
            None,
            Some((0, 0).into()),
        );
        output.set_preferred(mode);

        let shell = Shell::new(options.role, coquille_model::Size::new(size.w, size.h));

        Ok(Self {
            start_time: Instant::now(),
            display_handle,
            loop_signal,
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
            suppressed_keys: Vec::new(),
            // Nested, the host session keeps Super for itself.
            mod_key: ModKey::Alt,
            output,
            backend,
            shell,
            theme: options.theme,
            painter: Painter::new(),
            hover: None,
            pressed: None,
        })
    }

    /// The host window changed size: so does the output.
    pub fn resize_output(&mut self, size: Size<i32, Physical>) {
        self.output
            .change_current_state(Some(output_mode(size)), None, None, None);
        let size = coquille_model::Size::new(size.w, size.h);
        if let Err(error) = self.shell.desk.set_output_size(self.shell.output, size) {
            warn!(%error, "cannot resize the output");
        }
        self.apply_arrangement();
    }

    /// Asks for a frame at the next tick.
    pub fn queue_redraw(&mut self) {
        self.backend.redraw_needed = true;
    }
}

fn output_mode(size: Size<i32, Physical>) -> Mode {
    Mode {
        size,
        refresh: REFRESH,
    }
}

/// Lets clients hand over GPU buffers, which every OpenGL or Vulkan client
/// needs.
fn init_dmabuf(dh: &DisplayHandle, backend: &mut Winit) -> (DmabufState, DmabufGlobal) {
    let renderer = backend.backend.renderer();
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
