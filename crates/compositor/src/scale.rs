//! Telling clients the scale of the screen they are on, so that they draw
//! with as many pixels as it shows them with: `wp_fractional_scale_v1` for
//! those that know it, `wl_surface.preferred_buffer_scale` (rounded up) for
//! the others.

use coquille_model::{OutputId, Scale};
use smithay::desktop::{PopupManager, layer_map_for_output};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Transform;
use smithay::wayland::compositor::{
    TraversalAction, send_surface_state, with_surface_tree_downward,
};
use smithay::wayland::fractional_scale::{FractionalScaleHandler, with_fractional_scale};
use smithay::wayland::seat::WaylandFocus;

use crate::state::State;

/// Tells a surface, its subsurfaces and its menus the scale to draw at.
/// Each value only goes out when it changed.
pub fn tell_scale(surface: &WlSurface, scale: Scale) {
    tell_tree(surface, scale);
    for (popup, _) in PopupManager::popups_for_surface(surface) {
        tell_tree(popup.wl_surface(), scale);
    }
}

fn tell_tree(surface: &WlSurface, scale: Scale) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, ()| TraversalAction::DoChildren(()),
        |surface, states, ()| {
            send_surface_state(surface, states, scale.integer(), Transform::Normal);
            with_fractional_scale(states, |fractional| {
                fractional.set_preferred_scale(scale.get());
            });
        },
        |_, _, ()| true,
    );
}

impl State {
    /// The screen a surface shows on: its window's, its panel's, else the
    /// pointer's (where a new window opens).
    fn screen_of_surface(&self, surface: &WlSurface) -> Option<OutputId> {
        let window = self
            .shell
            .windows
            .iter()
            .find(|managed| managed.window.wl_surface().as_deref() == Some(surface));
        if let Some(managed) = window {
            return Some(managed.geometry.output);
        }
        let panel = self.outputs.iter().find(|screen| {
            layer_map_for_output(&screen.output)
                .layer_for_surface(surface, smithay::desktop::WindowSurfaceType::ALL)
                .is_some()
        });
        if let Some(screen) = panel {
            return Some(screen.id);
        }
        self.pointer_screen().map(|screen| screen.id)
    }

    /// Tells the panels of a screen its scale.
    pub fn tell_panels_scale(&self, output: OutputId) {
        let scale = self.shell.scale_of(output);
        let Some(wl_output) = self.output(output) else {
            return;
        };
        let map = layer_map_for_output(wl_output);
        for layer in map.layers() {
            tell_scale(layer.wl_surface(), scale);
        }
    }
}

impl FractionalScaleHandler for State {
    /// A client asks for the scale of a surface: the one of the screen it
    /// is on, or will open on.
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        let scale = self
            .screen_of_surface(&surface)
            .map_or(Scale::ONE, |id| self.shell.scale_of(id));
        tell_scale(&surface, scale);
    }
}
