//! Implementations of the Smithay protocol handlers.

mod compositor;
mod layer_shell;
mod xdg_shell;

use std::os::fd::OwnedFd;

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

/// Decorations are always drawn by the compositor: the title bar carries the
/// shell's own buttons.
impl XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        with_states(toplevel.wl_surface(), |states| {
            states
                .data_map
                .insert_if_missing_threadsafe(|| AskedForDecorations);
        });
        force_server_side(&toplevel);
        self.decorate(toplevel.wl_surface());
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: DecorationMode) {
        force_server_side(&toplevel);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        force_server_side(&toplevel);
    }
}

/// Marks a toplevel whose client asked the compositor to decorate it. The
/// others (GTK applications, mostly) draw their own title bar whatever the
/// compositor says, and get none from the shell.
struct AskedForDecorations;

pub fn asked_for_decorations(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states.data_map.get::<AskedForDecorations>().is_some()
    })
}

fn force_server_side(toplevel: &ToplevelSurface) {
    toplevel.with_pending_state(|state| {
        state.decoration_mode = Some(DecorationMode::ServerSide);
    });
    if toplevel.is_initial_configure_sent() {
        toplevel.send_pending_configure();
    }
}

smithay::delegate_dispatch2!(State);
