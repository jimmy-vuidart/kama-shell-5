use smithay::desktop::{LayerSurface, layer_map_for_output};
use smithay::output::Output;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::wayland::shell::wlr_layer::{
    Layer, LayerSurface as WlrLayerSurface, WlrLayerShellHandler, WlrLayerShellState,
};
use smithay::wayland::shell::xdg::PopupSurface;
use tracing::{debug, warn};

use crate::state::State;

/// Panels of the shell: the dock, its drawer.
impl WlrLayerShellHandler for State {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    /// A panel goes on the screen its client names, else on the one the
    /// pointer is on. It is placed on its first commit.
    fn new_layer_surface(
        &mut self,
        surface: WlrLayerSurface,
        output: Option<WlOutput>,
        layer: Layer,
        namespace: String,
    ) {
        let named = output.as_ref().and_then(Output::from_resource);
        let named = named.and_then(|output| {
            let known = self.outputs.iter().find(|screen| screen.output == output);
            known.map(|screen| screen.id)
        });
        let pointer = || self.pointer_screen().map(|screen| screen.id);
        let first = || self.outputs.first().map(|screen| screen.id);
        let Some(id) = named.or_else(pointer).or_else(first) else {
            warn!(namespace, "no screen to show a panel on");
            return;
        };
        let Some(output) = self.output(id) else {
            return;
        };
        debug!(namespace, ?layer, screen = output.name(), "new panel");
        let panel = LayerSurface::new(surface, namespace);
        if let Err(error) = layer_map_for_output(output).map_layer(&panel) {
            warn!(%error, "cannot show a panel");
        }
    }

    /// A menu of a panel: it stays on the screen of the panel.
    fn new_popup(&mut self, _parent: WlrLayerSurface, popup: PopupSurface) {
        self.unconstrain_popup(&popup);
    }

    fn layer_destroyed(&mut self, surface: WlrLayerSurface) {
        let found = self.outputs.iter().find_map(|screen| {
            let map = layer_map_for_output(&screen.output);
            let panel = map.layers().find(|panel| *panel.layer_surface() == surface);
            panel.cloned().map(|panel| (screen.id, panel))
        });
        let Some((id, panel)) = found else {
            return;
        };
        debug!(namespace = panel.namespace(), "panel gone");
        if let Some(output) = self.output(id) {
            layer_map_for_output(output).unmap_layer(&panel);
        }
        if self.panel_focus.as_ref() == Some(&panel) {
            self.panel_focus = None;
        }
        // Windows take back what the panel reserved; the keyboard returns
        // to the window that had it.
        self.arrange_panels(id);
        self.refresh_pointer_focus();
    }
}
