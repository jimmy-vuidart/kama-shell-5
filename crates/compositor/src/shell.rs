//! Glue between the placement model and Wayland windows.
//!
//! The model decides; this module applies: sizes go to clients as configures,
//! positions and stacking are kept for rendering and hit-testing.

use coquille_model::{
    AppKey, Desk, Metrics, OpenRequest, OutputId, OutputInfo, OutputRole, Size, WindowGeometry,
    WindowId,
};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::desktop::{Window, space::SpaceElement};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as ToplevelState;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{SERIAL_COUNTER, Transform};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData};
use tracing::{debug, warn};

use crate::decoration::layout::TitlebarButton;
use crate::decoration::naming::{badge_color, display_name};
use crate::decoration::paint::{Painter, TitlebarSpec};
use crate::render::scene::Chrome;
use crate::state::State;
use crate::theme::Theme;

/// The only output of the nested compositor.
const OUTPUT: OutputId = OutputId(0);

/// A window the model knows about.
pub struct ManagedWindow {
    pub id: WindowId,
    pub window: Window,
    pub app_id: String,
    pub title: String,
    /// Last geometry decided by the model.
    pub geometry: WindowGeometry,
    pub chrome: Chrome,
}

pub struct Shell {
    pub desk: Desk,
    pub output: OutputId,
    pub role: OutputRole,
    /// Bottom to top, as the model orders them.
    pub windows: Vec<ManagedWindow>,
    /// Toplevels that have not made their first commit yet.
    pub unmapped: Vec<Window>,
    next_id: u64,
}

impl Shell {
    pub fn new(role: OutputRole, size: Size) -> Self {
        let output = OutputInfo::new(OUTPUT, role, size);
        Self {
            desk: Desk::new(vec![output], Metrics::default()),
            output: OUTPUT,
            role,
            windows: Vec::new(),
            unmapped: Vec::new(),
            next_id: 1,
        }
    }

    /// The managed window whose toplevel surface is `surface`.
    pub fn by_surface(&self, surface: &WlSurface) -> Option<&ManagedWindow> {
        self.windows
            .iter()
            .find(|managed| is_toplevel_of(&managed.window, surface))
    }

    pub fn by_surface_mut(&mut self, surface: &WlSurface) -> Option<&mut ManagedWindow> {
        self.windows
            .iter_mut()
            .find(|managed| is_toplevel_of(&managed.window, surface))
    }

    pub fn by_id(&self, id: WindowId) -> Option<&ManagedWindow> {
        self.windows.iter().find(|managed| managed.id == id)
    }

    /// Takes the model's arrangement: geometry of each window, and stacking.
    fn sync(&mut self) {
        let arrangement = self.desk.arrangement();
        let rank = |id: WindowId| arrangement.iter().position(|geometry| geometry.id == id);
        self.windows.sort_by_key(|managed| rank(managed.id));
        for managed in &mut self.windows {
            if let Some(geometry) = arrangement.iter().find(|g| g.id == managed.id) {
                managed.geometry = *geometry;
            }
        }
    }

    /// Repaints the title bars whose look changed.
    pub fn refresh_chrome(
        &mut self,
        painter: &mut Painter,
        theme: &Theme,
        hover: Option<(WindowId, TitlebarButton)>,
    ) {
        let corner_radius = self.desk.metrics().corner_radius;
        for managed in &mut self.windows {
            let geometry = managed.geometry;
            let Some(bar) = geometry.titlebar.filter(|_| geometry.visible) else {
                continue;
            };
            let name = display_name(&managed.app_id);
            let subtitle = if managed.title == name {
                String::new()
            } else {
                managed.title.clone()
            };
            let spec = TitlebarSpec {
                size: bar.size(),
                title: name,
                subtitle,
                badge: badge_color(&managed.app_id),
                focused: geometry.focused,
                hovered: hover
                    .filter(|(id, _)| *id == managed.id)
                    .map(|(_, button)| button),
                show_swap: self.role.is_side(),
                theme: theme.kind,
                corner_radius,
            };
            if managed.chrome.spec.as_ref() == Some(&spec) {
                continue;
            }
            managed.chrome.titlebar = painter.titlebar(&spec).map(|pixmap| {
                // tiny-skia stores premultiplied RGBA bytes, which is ABGR8888
                // in DRM's little-endian naming.
                let size = (pixmap.width() as i32, pixmap.height() as i32);
                MemoryRenderBuffer::from_slice(
                    pixmap.data(),
                    Fourcc::Abgr8888,
                    size,
                    1,
                    Transform::Normal,
                    None,
                )
            });
            managed.chrome.spec = Some(spec);
        }
    }
}

fn is_toplevel_of(window: &Window, surface: &WlSurface) -> bool {
    window
        .toplevel()
        .is_some_and(|toplevel| toplevel.wl_surface() == surface)
}

/// `app_id` and title set by the client, empty when missing.
fn toplevel_names(toplevel: &ToplevelSurface) -> (String, String) {
    with_states(toplevel.wl_surface(), |states| {
        let attributes = states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|data| data.lock().ok());
        match attributes {
            Some(attributes) => (
                attributes.app_id.clone().unwrap_or_default(),
                attributes.title.clone().unwrap_or_default(),
            ),
            None => Default::default(),
        }
    })
}

impl State {
    /// A toplevel made its first commit: the model places it, and it gets its
    /// initial configure.
    pub fn map_window(&mut self, surface: &WlSurface) {
        let Some(index) = self
            .shell
            .unmapped
            .iter()
            .position(|window| is_toplevel_of(window, surface))
        else {
            return;
        };
        let window = self.shell.unmapped.swap_remove(index);
        let Some(toplevel) = window.toplevel() else {
            return;
        };
        let (app_id, title) = toplevel_names(toplevel);

        let id = WindowId(self.shell.next_id);
        self.shell.next_id += 1;
        let app = (!app_id.is_empty()).then(|| AppKey::new(app_id.clone()));
        let request = OpenRequest::at(self.shell.output);
        if let Err(error) = self.shell.desk.open_window(id, app, request) {
            warn!(%error, "the model refused a new window");
            return;
        }
        // Until windows can be dragged to slots (phase 2), tile right away.
        if let Err(error) = self.shell.desk.tile_automatically(id) {
            warn!(%error, "cannot tile a new window");
        }
        if let Err(error) = self.shell.desk.set_title(id, title.clone()) {
            warn!(%error, "cannot title a new window");
        }
        let Some(geometry) = self
            .shell
            .desk
            .arrangement()
            .into_iter()
            .find(|geometry| geometry.id == id)
        else {
            return;
        };
        debug!(?id, app_id, title, "window mapped");

        self.shell.windows.push(ManagedWindow {
            id,
            window,
            app_id,
            title,
            geometry,
            chrome: Chrome::default(),
        });
        self.apply_arrangement();
    }

    /// A toplevel is gone, mapped or not.
    pub fn unmap_window(&mut self, surface: &WlSurface) {
        self.shell
            .unmapped
            .retain(|window| !is_toplevel_of(window, surface));
        let Some(id) = self.shell.by_surface(surface).map(|managed| managed.id) else {
            return;
        };
        self.shell.windows.retain(|managed| managed.id != id);
        if let Err(error) = self.shell.desk.close_window(id) {
            warn!(%error, "the model did not know a closed window");
        }
        debug!(?id, "window unmapped");
        self.apply_arrangement();
    }

    /// The client changed its title or `app_id`.
    pub fn refresh_names(&mut self, toplevel: &ToplevelSurface) {
        let (app_id, title) = toplevel_names(toplevel);
        let Some(managed) = self.shell.by_surface_mut(toplevel.wl_surface()) else {
            return;
        };
        managed.app_id = app_id;
        managed.title = title.clone();
        let id = managed.id;
        if let Err(error) = self.shell.desk.set_title(id, title) {
            warn!(%error, "cannot retitle a window");
        }
        self.queue_redraw();
    }

    /// Makes the Wayland side match the model: sizes and states go to
    /// clients, the keyboard focus follows the focused window.
    pub fn apply_arrangement(&mut self) {
        self.shell.sync();

        for managed in &self.shell.windows {
            let geometry = managed.geometry;
            let Some(toplevel) = managed.window.toplevel() else {
                continue;
            };
            toplevel.with_pending_state(|state| {
                state.size = Some((geometry.client.w, geometry.client.h).into());
                state.decoration_mode = Some(DecorationMode::ServerSide);

                let mut set = |flag: ToplevelState, on: bool| {
                    if on {
                        state.states.set(flag);
                    } else {
                        state.states.unset(flag);
                    }
                };
                set(ToplevelState::Activated, geometry.focused);
                set(ToplevelState::Fullscreen, geometry.anchored);
                // Tiled clients drop their own shadows and rounded corners.
                let tiled = !geometry.floating && !geometry.anchored;
                for edge in [
                    ToplevelState::TiledLeft,
                    ToplevelState::TiledRight,
                    ToplevelState::TiledTop,
                    ToplevelState::TiledBottom,
                ] {
                    set(edge, tiled);
                }
            });
            if toplevel.is_initial_configure_sent() {
                toplevel.send_pending_configure();
            } else {
                toplevel.send_configure();
            }

            // Tells the client which output it is on, hence its scale.
            if geometry.visible {
                managed
                    .window
                    .output_enter(&self.output, managed.window.bbox());
            } else {
                managed.window.output_leave(&self.output);
            }
        }

        self.sync_keyboard_focus();
        self.backend.full_redraw = true;
        self.queue_redraw();
    }

    fn sync_keyboard_focus(&mut self) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        // A popup grab owns the keyboard until it ends.
        if keyboard.is_grabbed() {
            return;
        }
        let wanted = self
            .shell
            .desk
            .focused()
            .and_then(|id| self.shell.by_id(id))
            .and_then(|managed| managed.window.toplevel())
            .map(|toplevel| toplevel.wl_surface().clone());
        if keyboard.current_focus() != wanted {
            keyboard.set_focus(self, wanted, SERIAL_COUNTER.next_serial());
        }
    }
}
