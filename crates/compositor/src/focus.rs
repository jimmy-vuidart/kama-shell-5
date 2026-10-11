//! What the keyboard is focused on: a Wayland surface, which for an X11
//! window comes with the window itself.

use std::borrow::Cow;

use smithay::backend::input::{InputTime, KeyState};
use smithay::desktop::{PopupKind, Window};
use smithay::input::Seat;
use smithay::input::keyboard::{KeyboardTarget, KeysymHandle, ModifiersState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{IsAlive, Serial};
use smithay::wayland::seat::WaylandFocus;
use smithay::xwayland::X11Surface;

use crate::state::State;

/// The target of the keyboard focus.
///
/// Key events always go to `surface`. An X11 window needs more than that:
/// the X server must be told too, or X11 clients ignore the keys. `x11`
/// carries the window that does it.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyFocus {
    surface: WlSurface,
    x11: Option<X11Surface>,
}

impl KeyFocus {
    /// The focus on a window. `None` for an X11 window that has no surface
    /// yet: it cannot receive keys before it has one.
    pub fn of(window: &Window) -> Option<Self> {
        let surface = window.wl_surface()?.into_owned();
        Some(Self {
            surface,
            x11: window.x11_surface().cloned(),
        })
    }

    pub fn surface(&self) -> &WlSurface {
        &self.surface
    }

    pub fn x11(&self) -> Option<&X11Surface> {
        self.x11.as_ref()
    }
}

impl From<WlSurface> for KeyFocus {
    fn from(surface: WlSurface) -> Self {
        Self { surface, x11: None }
    }
}

/// Popup grabs focus the popup itself.
impl From<PopupKind> for KeyFocus {
    fn from(popup: PopupKind) -> Self {
        popup.wl_surface().clone().into()
    }
}

/// Popup grabs send pointer events to what holds the keyboard.
impl From<KeyFocus> for WlSurface {
    fn from(focus: KeyFocus) -> Self {
        focus.surface
    }
}

impl IsAlive for KeyFocus {
    fn alive(&self) -> bool {
        self.surface.alive() && self.x11.as_ref().is_none_or(IsAlive::alive)
    }
}

impl WaylandFocus for KeyFocus {
    fn wl_surface(&self) -> Option<Cow<'_, WlSurface>> {
        Some(Cow::Borrowed(&self.surface))
    }
}

impl KeyboardTarget<State> for KeyFocus {
    fn enter(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        keys: Vec<KeysymHandle<'_>>,
        serial: Serial,
    ) {
        // The X11 window tells the X server, then passes on to its surface.
        match &self.x11 {
            Some(window) => KeyboardTarget::enter(window, seat, data, keys, serial),
            None => KeyboardTarget::enter(&self.surface, seat, data, keys, serial),
        }
    }

    fn leave(&self, seat: &Seat<State>, data: &mut State, serial: Serial) {
        match &self.x11 {
            Some(window) => KeyboardTarget::leave(window, seat, data, serial),
            None => KeyboardTarget::leave(&self.surface, seat, data, serial),
        }
    }

    fn key(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        key: KeysymHandle<'_>,
        state: KeyState,
        serial: Serial,
        time: InputTime,
    ) {
        match &self.x11 {
            Some(window) => KeyboardTarget::key(window, seat, data, key, state, serial, time),
            None => KeyboardTarget::key(&self.surface, seat, data, key, state, serial, time),
        }
    }

    fn modifiers(
        &self,
        seat: &Seat<State>,
        data: &mut State,
        modifiers: ModifiersState,
        serial: Serial,
    ) {
        match &self.x11 {
            Some(window) => KeyboardTarget::modifiers(window, seat, data, modifiers, serial),
            None => KeyboardTarget::modifiers(&self.surface, seat, data, modifiers, serial),
        }
    }
}
