//! Panels of the shell: surfaces that clients pin to a screen with the
//! layer-shell protocol (the dock, its drawer). A panel is not a window: no
//! title bar, no slot, no habit. It may keep an edge of its screen for
//! itself, and take the keyboard while it needs it.

use coquille_ipc::GlassShape;
use coquille_model::{Insets, OutputId, OutputRole, Rect, Size};
use smithay::desktop::{LayerSurface, WindowSurfaceType, layer_map_for_output};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point};
use smithay::wayland::compositor::get_parent;
use smithay::wayland::shell::wlr_layer::{KeyboardInteractivity, Layer};
use tracing::{debug, warn};

use crate::state::State;

/// A panel and where it is.
#[derive(Debug, Clone)]
pub struct Panel {
    pub surface: LayerSurface,
    pub layer: Layer,
    pub output: OutputId,
    /// Its top-left corner in the global space.
    pub origin: Point<i32, Logical>,
    /// The liquid glass its program asked for (see `Command::SetGlass`).
    pub liquid: Vec<GlassShape>,
}

/// How a panel asks for the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// It never wants it (the dock).
    Never,
    /// It takes it when the user clicks on it.
    OnDemand,
    /// It has it for as long as it is shown (the drawer and its search).
    Exclusive,
}

/// Which panel has the keyboard, among those shown, topmost first: the first
/// that wants it for itself, else the one the user clicked if it accepts.
/// `None` leaves the keyboard to the focused window.
pub fn keyboard_holder(asks: &[Ask], clicked: Option<usize>) -> Option<usize> {
    let exclusive = asks.iter().position(|ask| *ask == Ask::Exclusive);
    exclusive.or(clicked.filter(|index| asks.get(*index) == Some(&Ask::OnDemand)))
}

/// What a screen keeps for its panels, given the room they leave free.
pub fn reserved(screen: Size, free: Rect) -> Insets {
    Insets {
        left: free.x.max(0),
        top: free.y.max(0),
        right: (screen.w - free.right()).max(0),
        bottom: (screen.h - free.bottom()).max(0),
    }
}

/// The launcher of the dock is open on this screen.
fn has_drawer(output: &smithay::output::Output) -> bool {
    let map = layer_map_for_output(output);
    map.layers()
        .any(|layer| layer.namespace() == DRAWER_NAMESPACE)
}

/// What the dock calls its launcher.
const DRAWER_NAMESPACE: &str = "coquille-drawer";

impl State {
    /// The panels shown, topmost first. A screen held by a game shows none,
    /// unless games hold every screen (a laptop) and the drawer is open on
    /// it: with nowhere else to go, the dock comes over the game.
    pub fn panels(&self) -> Vec<Panel> {
        let elsewhere = self
            .outputs
            .iter()
            .any(|screen| !self.is_anchored(screen.id));
        let mut panels = Vec::new();
        for layer in [Layer::Overlay, Layer::Top, Layer::Bottom, Layer::Background] {
            for screen in &self.outputs {
                let Some(info) = self.shell.screen(screen.id) else {
                    continue;
                };
                let over_game = !elsewhere && has_drawer(&screen.output);
                if self.is_anchored(screen.id) && !over_game {
                    continue;
                }
                let corner = Point::from((info.rect.x, info.rect.y));
                let map = layer_map_for_output(&screen.output);
                for surface in map.layers_on(layer).rev() {
                    if let Some(geometry) = map.layer_geometry(surface) {
                        panels.push(Panel {
                            surface: surface.clone(),
                            layer,
                            output: screen.id,
                            origin: corner + geometry.loc,
                            liquid: self.liquid_of(surface, info.role),
                        });
                    }
                }
            }
        }
        panels
    }

    /// The liquid glass of a panel of the shell on the screen `role`.
    fn liquid_of(&self, surface: &LayerSurface, role: OutputRole) -> Vec<GlassShape> {
        self.liquid
            .get(&(surface.namespace().to_owned(), role))
            .cloned()
            .unwrap_or_default()
    }

    /// A fullscreen window holds this screen (game mode).
    pub fn is_anchored(&self, output: OutputId) -> bool {
        let screen = self.shell.desk.screen(output);
        screen.is_some_and(|screen| screen.anchored().is_some())
    }

    /// The panel `surface` belongs to: its own surface, a subsurface of it.
    pub fn panel_of(&self, surface: &WlSurface) -> Option<Panel> {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        self.outputs.iter().find_map(|screen| {
            let info = self.shell.screen(screen.id)?;
            let map = layer_map_for_output(&screen.output);
            let surface = map.layer_for_surface(&root, WindowSurfaceType::TOPLEVEL)?;
            let geometry = map.layer_geometry(surface)?;
            Some(Panel {
                surface: surface.clone(),
                layer: surface.layer(),
                output: screen.id,
                origin: Point::from((info.rect.x, info.rect.y)) + geometry.loc,
                liquid: self.liquid_of(surface, info.role),
            })
        })
    }

    /// The panel that has the keyboard, when one does.
    pub fn keyboard_panel(&self) -> Option<LayerSurface> {
        let panels = self.panels();
        let asks: Vec<Ask> = panels.iter().map(ask).collect();
        let clicked = self
            .panel_focus
            .as_ref()
            .and_then(|clicked| panels.iter().position(|panel| panel.surface == *clicked));
        keyboard_holder(&asks, clicked).map(|index| panels[index].surface.clone())
    }

    /// Panels came, went or changed on a screen: they get the place they ask
    /// for, and windows keep out of what they reserve.
    pub fn arrange_panels(&mut self, output: OutputId) {
        let Some(wl_output) = self.output(output).cloned() else {
            return;
        };
        let free = {
            let mut map = layer_map_for_output(&wl_output);
            map.arrange();
            map.non_exclusive_zone()
        };
        self.tell_panels_scale(output);
        let Some(info) = self.shell.desk.screen(output).map(|screen| *screen.info()) else {
            return;
        };
        let free = Rect::new(free.loc.x, free.loc.y, free.size.w, free.size.h);
        let wanted = reserved(info.size, free);
        if wanted != info.reserved {
            debug!(?output, ?wanted, "room reserved for the panels of a screen");
            if let Err(error) = self.shell.desk.set_reserved(output, wanted) {
                warn!(%error, "cannot reserve room for a panel");
            }
            self.apply_arrangement();
        } else {
            self.sync_keyboard_focus();
            self.queue_redraw();
        }
    }

    /// A client drew on a panel, or changed what it asks for it.
    pub fn commit_panel(&mut self, surface: &WlSurface) {
        let Some(panel) = self.panel_of(surface) else {
            return;
        };
        self.arrange_panels(panel.output);
        // The first configure answers the first commit, as the protocol
        // wants; later ones are sent by the arrangement itself.
        panel.surface.layer_surface().send_pending_configure();
        self.refresh_pointer_focus_over_panels();
    }
}

fn ask(panel: &Panel) -> Ask {
    let wanted = panel.surface.cached_state().keyboard_interactivity;
    // Only what is above the windows may keep the keyboard from them.
    let above = matches!(panel.layer, Layer::Overlay | Layer::Top);
    match wanted {
        KeyboardInteractivity::Exclusive if above => Ask::Exclusive,
        KeyboardInteractivity::Exclusive | KeyboardInteractivity::OnDemand => Ask::OnDemand,
        KeyboardInteractivity::None => Ask::Never,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Size = Size::new(1920, 1080);

    #[test]
    fn without_panel_the_keyboard_stays_with_the_windows() {
        assert_eq!(keyboard_holder(&[], None), None);
    }

    #[test]
    fn the_dock_never_takes_the_keyboard_even_clicked() {
        assert_eq!(keyboard_holder(&[Ask::Never], Some(0)), None);
    }

    #[test]
    fn a_panel_on_demand_takes_the_keyboard_once_clicked() {
        let asks = [Ask::Never, Ask::OnDemand];

        assert_eq!(keyboard_holder(&asks, None), None);
        assert_eq!(keyboard_holder(&asks, Some(1)), Some(1));
    }

    #[test]
    fn an_exclusive_panel_has_the_keyboard_whatever_was_clicked() {
        let asks = [Ask::OnDemand, Ask::Never, Ask::Exclusive];

        assert_eq!(keyboard_holder(&asks, Some(0)), Some(2));
    }

    #[test]
    fn the_topmost_exclusive_panel_wins() {
        let asks = [Ask::Exclusive, Ask::Exclusive];

        assert_eq!(keyboard_holder(&asks, None), Some(0));
    }

    #[test]
    fn a_click_on_a_panel_that_is_gone_gives_nothing() {
        assert_eq!(keyboard_holder(&[Ask::OnDemand], Some(3)), None);
    }

    #[test]
    fn a_screen_without_panel_reserves_nothing() {
        assert_eq!(reserved(SCREEN, Rect::from_size(SCREEN)), Insets::ZERO);
    }

    #[test]
    fn a_dock_on_the_left_reserves_its_width() {
        let insets = reserved(SCREEN, Rect::new(60, 0, 1860, 1080));

        assert_eq!(
            insets,
            Insets {
                left: 60,
                ..Insets::ZERO
            }
        );
    }

    #[test]
    fn panels_on_several_edges_each_reserve_their_own() {
        let insets = reserved(SCREEN, Rect::new(0, 30, 1880, 1010));

        assert_eq!(
            insets,
            Insets {
                left: 0,
                top: 30,
                right: 40,
                bottom: 40,
            }
        );
    }

    #[test]
    fn free_room_larger_than_the_screen_reserves_nothing() {
        let insets = reserved(SCREEN, Rect::new(-10, -10, 2000, 1200));

        assert_eq!(insets, Insets::ZERO);
    }
}
