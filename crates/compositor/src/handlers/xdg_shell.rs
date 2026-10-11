use coquille_model::WindowGeometry;
use smithay::desktop::{
    PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy, Window,
    find_popup_root_surface, get_popup_toplevel_coords,
};
use smithay::input::Seat;
use smithay::input::pointer::Focus;
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::utils::{Logical, Point, Serial};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};

use crate::shell::Unmapped;
use crate::state::State;

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        // Placed on its first commit, once its `app_id` is known.
        self.shell.unmapped.push(Unmapped {
            window: Window::new_wayland_window(surface),
            fullscreen: None,
        });
    }

    /// Fullscreen anchors the screen (game mode): the one the client names
    /// if it is one of ours, else the one its window is on.
    fn fullscreen_request(&mut self, surface: ToplevelSurface, output: Option<WlOutput>) {
        let wanted = output
            .as_ref()
            .and_then(Output::from_resource)
            .and_then(|output| self.outputs.iter().find(|screen| screen.output == output))
            .map(|screen| screen.id);
        let wl_surface = surface.wl_surface();
        if let Some(before) = self.shell.by_surface(wl_surface).map(|m| m.geometry) {
            self.enter_fullscreen(before.id, wanted);
            self.answer(&surface, before);
        } else if let Some(unmapped) = self.shell.unmapped_mut(wl_surface) {
            // Applied when the window is placed, on its first commit.
            unmapped.fullscreen = Some(wanted);
        }
    }

    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface();
        if let Some(before) = self.shell.by_surface(wl_surface).map(|m| m.geometry) {
            self.leave_fullscreen(before.id);
            self.answer(&surface, before);
        } else if let Some(unmapped) = self.shell.unmapped_mut(wl_surface) {
            unmapped.fullscreen = None;
        }
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.unmap_window(surface.wl_surface());
    }

    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if let Some(id) = self.shell.by_surface(surface.wl_surface()).map(|m| m.id) {
            self.minimize_window(id);
        }
    }

    fn title_changed(&mut self, surface: ToplevelSurface) {
        self.refresh_names(&surface);
    }

    fn app_id_changed(&mut self, surface: ToplevelSurface) {
        self.refresh_names(&surface);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
        if let Err(error) = self.popups.track_popup(PopupKind::Xdg(surface)) {
            tracing::warn!(%error, "cannot track a popup");
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    /// A menu wants to be dismissed by a click outside of it.
    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let Some(seat) = Seat::<State>::from_resource(&seat) else {
            return;
        };
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Ok(mut grab) = self.popups.grab_popup(root, kind, &seat, serial) else {
            return;
        };

        if let Some(keyboard) = seat.get_keyboard() {
            let previous = grab.previous_serial().unwrap_or(serial);
            if keyboard.is_grabbed() && !(keyboard.has_grab(serial) || keyboard.has_grab(previous))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            let previous = grab.previous_serial().unwrap_or_else(|| grab.serial());
            if pointer.is_grabbed() && !(pointer.has_grab(serial) || pointer.has_grab(previous)) {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }
}

impl State {
    /// The protocol wants a configure in answer to some requests, even when
    /// nothing changed; a change has sent one already.
    fn answer(&self, toplevel: &ToplevelSurface, before: WindowGeometry) {
        let after = self.shell.by_id(before.id).map(|m| m.geometry);
        if after == Some(before) && toplevel.is_initial_configure_sent() {
            toplevel.send_configure();
        }
    }

    /// Keeps a popup inside the screen of its window.
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let kind = PopupKind::Xdg(popup.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Some(managed) = self.shell.by_surface(&root) else {
            return;
        };
        let Some(screen) = self.shell.screen(managed.geometry.output) else {
            return;
        };

        // The positioner works relative to the parent, so the screen is
        // expressed from there.
        let client = managed.client();
        let origin: Point<i32, Logical> = (client.x, client.y).into();
        let mut target = crate::render::scene::rectangle(screen.rect);
        target.loc -= origin;
        target.loc -= get_popup_toplevel_coords(&kind);

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
