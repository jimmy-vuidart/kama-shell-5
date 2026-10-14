//! Implementations of the Smithay protocol handlers.

mod compositor;
mod layer_shell;
mod xdg_shell;

use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};

use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::ImportDma;
use smithay::input::dnd::{DnDGrab, DndGrabHandler, GrabType, Source};
use smithay::input::pointer::{CursorImageStatus, Focus, PointerHandle};
use smithay::input::tablet::TabletSeatHandler;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::compositor::with_states;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::drm_syncobj::{DrmSyncobjHandler, DrmSyncobjState};
use smithay::wayland::output::OutputHandler;
use smithay::wayland::pointer_constraints::{
    ConstraintRemove, PointerConstraint, PointerConstraintsHandler,
};
use smithay::wayland::selection::{SelectionHandler, SelectionSource, SelectionTarget};
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
};
use smithay::wayland::selection::ext_data_control::{
    DataControlHandler as ExtDataControlHandler, DataControlState as ExtDataControlState,
};
use smithay::wayland::selection::wlr_data_control::{
    DataControlHandler as WlrDataControlHandler, DataControlState as WlrDataControlState,
};
use smithay::wayland::shell::xdg::ToplevelSurface;
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;

use tracing::debug;

use crate::backend::Backend;
use crate::focus::KeyFocus;
use crate::state::State;

impl SeatHandler for State {
    type KeyboardFocus = KeyFocus;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<State> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, image: CursorImageStatus) {
        self.set_cursor(image);
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&KeyFocus>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|focus| dh.get_client(focus.surface().id()).ok());
        set_data_device_focus(dh, seat, client);
        // A menu that closes hands the keyboard back to what it belongs to;
        // when that is a panel that never wanted it (the dock), the keyboard
        // returns to the focused window instead.
        self.loop_handle
            .insert_idle(|state| state.sync_keyboard_focus());
    }
}

// Required by the cursor-shape protocol.
impl TabletSeatHandler for State {
    type ToolFocus = WlSurface;
}

/// Games lock the pointer in place, or confine it to their window.
impl PointerConstraintsHandler for State {
    fn new_constraint(&mut self, _surface: &WlSurface, _pointer: &PointerHandle<Self>) {
        self.refresh_constraint();
    }

    fn remove_constraint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        removed: ConstraintRemove,
    ) {
        // Only when the client lets go by itself: on a pointer leave, the
        // pointer is in the middle of a move and must be left alone.
        let ConstraintRemove::Destroyed(PointerConstraint::Locked(locked)) = removed else {
            return;
        };
        // The pointer shows again where the client drew its own cursor.
        if let Some(hint) = locked.cursor_position_hint() {
            if let Some(origin) = self.surface_origin_under_pointer(surface) {
                pointer.set_location(origin + hint);
                self.queue_redraw();
            }
        }
    }
}

/// The clipboard is shared with X11 applications.
impl SelectionHandler for State {
    type SelectionUserData = ();

    /// A Wayland client copied something: X11 clients may paste it.
    fn new_selection(
        &mut self,
        target: SelectionTarget,
        source: Option<SelectionSource>,
        _seat: Seat<Self>,
    ) {
        if let Some(xwm) = self.xwm.as_mut() {
            let mime_types = source.map(|source| source.mime_types());
            if let Err(error) = xwm.new_selection(target, mime_types) {
                debug!(%error, "cannot tell X11 about the clipboard");
            }
        }
    }

    /// A Wayland client pastes what an X11 client copied.
    fn send_selection(
        &mut self,
        target: SelectionTarget,
        mime_type: String,
        fd: OwnedFd,
        _seat: Seat<Self>,
        _user_data: &(),
    ) {
        if let Some(xwm) = self.xwm.as_mut() {
            if let Err(error) = xwm.send_selection(target, mime_type, fd) {
                debug!(%error, "cannot fetch the clipboard from X11");
            }
        }
    }
}

impl DataDeviceHandler for State {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl WlrDataControlHandler for State {
    fn data_control_state(&mut self) -> &mut WlrDataControlState {
        &mut self.wlr_data_control_state
    }
}

impl ExtDataControlHandler for State {
    fn data_control_state(&mut self) -> &mut ExtDataControlState {
        &mut self.ext_data_control_state
    }
}

impl DndGrabHandler for State {}

impl WaylandDndGrabHandler for State {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        seat: Seat<Self>,
        serial: Serial,
        type_: GrabType,
    ) {
        let pointer = match type_ {
            GrabType::Pointer => seat.get_pointer(),
            GrabType::Touch => None,
        };
        let Some((pointer, start_data)) =
            pointer.and_then(|pointer| Some((pointer.clone(), pointer.grab_start_data()?)))
        else {
            source.cancel();
            return;
        };
        let grab = DnDGrab::new_pointer(&self.display_handle, start_data, source, seat);
        pointer.set_grab(self, grab, serial, Focus::Keep);
    }
}

impl OutputHandler for State {}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.dmabuf_state
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self.backend.renderer().import_dmabuf(&dmabuf, None).is_ok() {
            let _ = notifier.successful::<State>();
        } else {
            notifier.failed();
        }
    }
}

impl DrmSyncobjHandler for State {
    fn drm_syncobj_state(&mut self) -> Option<&mut DrmSyncobjState> {
        match &mut self.backend {
            Backend::Udev(udev) => udev.syncobj.as_mut(),
            Backend::Winit(_) => None,
        }
    }
}

/// The compositor decorates a toplevel that asks for it or leaves it the
/// choice: the title bar carries the shell's own buttons. One that says it
/// decorates itself (Electron applications with their own title bar) is taken
/// at its word.
impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.decorations_wished(&toplevel, DecorationMode::ServerSide);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        self.decorations_wished(&toplevel, mode);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.decorations_wished(&toplevel, DecorationMode::ServerSide);
    }
}

impl State {
    /// A toplevel said who is to draw its decorations: its client gets what
    /// it wished, and its window, if it is already open, follows.
    fn decorations_wished(&mut self, toplevel: &ToplevelSurface, wished: DecorationMode) {
        let own = wished == DecorationMode::ClientSide;
        let surface = toplevel.wl_surface();
        with_states(surface, |states| {
            states
                .data_map
                .insert_if_missing_threadsafe(DecorationWish::default);
            if let Some(wish) = states.data_map.get::<DecorationWish>() {
                wish.own.store(own, Ordering::Relaxed);
            }
        });
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(decoration_mode(own));
        });
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
        self.set_own_titlebar(surface, own);
    }
}

/// What a toplevel said through `xdg-decoration`. Those that never spoke
/// (GTK applications, mostly) have none: they draw their own title bar
/// whatever the compositor says.
#[derive(Default)]
struct DecorationWish {
    /// It asked to decorate itself.
    own: AtomicBool,
}

/// Whether a toplevel draws its own title bar, so that the shell adds none.
pub fn draws_own_titlebar(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states
            .data_map
            .get::<DecorationWish>()
            .is_none_or(|wish| wish.own.load(Ordering::Relaxed))
    })
}

/// The mode announced to a toplevel that speaks `xdg-decoration`.
pub const fn decoration_mode(own_titlebar: bool) -> DecorationMode {
    if own_titlebar {
        DecorationMode::ClientSide
    } else {
        DecorationMode::ServerSide
    }
}

smithay::delegate_dispatch2!(State);
