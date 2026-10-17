use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::desktop::PopupKind;
use smithay::reexports::wayland_server::protocol::{wl_buffer, wl_surface::WlSurface};
use smithay::reexports::wayland_server::{Client, Resource};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    BufferAssignment, CompositorClientState, CompositorHandler, CompositorState, SurfaceAttributes,
    add_blocker, add_pre_commit_hook, get_parent, is_sync_subsurface, with_states,
};
use smithay::wayland::dmabuf::get_dmabuf;
use smithay::wayland::drm_syncobj::DrmSyncobjCachedState;
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::xwayland::XWaylandClientData;
use tracing::warn;

use crate::state::{ClientState, State};

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        // Every client is inserted with a `ClientState`, see `main`.
        static FALLBACK: std::sync::OnceLock<CompositorClientState> = std::sync::OnceLock::new();
        // XWayland is a client too, inserted by Smithay with its own data.
        if let Some(state) = client.get_data::<XWaylandClientData>() {
            return &state.compositor_state;
        }
        match client.get_data::<ClientState>() {
            Some(state) => &state.compositor_state,
            None => FALLBACK.get_or_init(CompositorClientState::default),
        }
    }

    /// A client using explicit sync says when its buffer will be ready: its
    /// commit is held until then, so that no half-drawn frame is shown.
    fn new_surface(&mut self, surface: &WlSurface) {
        add_pre_commit_hook::<Self, _>(surface, |state, _dh, surface| {
            let acquire_point = with_states(surface, |states| {
                let point = states
                    .cached_state
                    .get::<DrmSyncobjCachedState>()
                    .pending()
                    .acquire_point
                    .clone();
                let mut attributes = states.cached_state.get::<SurfaceAttributes>();
                let gpu_buffer = matches!(
                    attributes.pending().buffer.as_ref(),
                    Some(BufferAssignment::NewBuffer(buffer)) if get_dmabuf(buffer).is_ok()
                );
                point.filter(|_| gpu_buffer)
            });
            let (Some(acquire_point), Some(client)) = (acquire_point, surface.client()) else {
                return;
            };
            let Ok((blocker, source)) = acquire_point.generate_blocker() else {
                return;
            };
            let watched = state.loop_handle.insert_source(source, move |_, _, state| {
                let dh = state.display_handle.clone();
                let client_state = state.client_compositor_state(&client);
                client_state.blocker_cleared(state, &dh);
                Ok(())
            });
            if watched.is_ok() {
                add_blocker(surface, blocker);
            }
        });
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);

        let mut covers_anew = false;
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            if let Some(managed) = self.shell.by_surface_mut(&root) {
                managed.window.on_commit();
                let covered = managed.window.bbox().size;
                covers_anew = std::mem::replace(&mut managed.covered, covered) != covered;
                if covered.w > 0 && covered.h > 0 && managed.drawn_at.is_none() {
                    managed.drawn_at = Some(std::time::Instant::now());
                }
            }
        }

        // First commit of a toplevel: the model places it, it gets configured.
        self.map_window(surface);
        self.follow_client_size(surface);
        // What the window covers changed (its first buffer, most of the
        // time): so may have what is under the pointer.
        if covers_anew {
            self.refresh_pointer_focus();
        }

        self.commit_panel(surface);

        self.popups.commit(surface);
        if let Some(PopupKind::Xdg(popup)) = self.popups.find_popup(surface) {
            if !popup.is_initial_configure_sent() {
                if let Err(error) = popup.send_configure() {
                    warn!(%error, "cannot configure a popup");
                }
            }
        }

        self.queue_redraw();
    }
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}
