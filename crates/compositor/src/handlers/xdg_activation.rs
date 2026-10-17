//! xdg-activation: an application asks for one of its windows to come
//! forward (its tray icon was clicked, it was launched again, a notification
//! of its was clicked). Without it, a minimized Wayland window could never
//! be brought back by its application.

use std::time::Duration;

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};
use tracing::{debug, info, warn};

use crate::state::State;

/// A token older than this was not asked for by what the user just did.
const FRESH: Duration = Duration::from_secs(10);

impl XdgActivationHandler for State {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    /// The window shows, minimized or not, and takes the focus, unless a
    /// game is being played (the model decides).
    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        // A token is used once.
        self.xdg_activation_state.remove_token(&token);
        if token_data.timestamp.elapsed() > FRESH {
            debug!("activation asked with a stale token: ignored");
            return;
        }
        let Some(id) = self.shell.by_surface(&surface).map(|managed| managed.id) else {
            debug!("activation asked for a window not placed yet: ignored");
            return;
        };
        match self.shell.desk.activate(id) {
            Ok(()) => info!(?id, "a window asked to come forward"),
            Err(error) => warn!(%error, "cannot bring the window forward"),
        }
        self.apply_arrangement();
        self.refresh_pointer_focus();
    }
}
