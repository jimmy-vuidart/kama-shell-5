//! Glue between the placement model and Wayland windows.
//!
//! The model decides; this module applies: sizes go to clients as configures,
//! positions and stacking are kept for rendering and hit-testing.

use coquille_model::{
    AppKey, AppMemory, Desk, DropTarget, Metrics, OpenRequest, OutputId, OutputInfo,
    OutputRole, Placement, Point as ModelPoint, Rect, Size as ModelSize, WindowGeometry, WindowId,
};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::desktop::{Window, space::SpaceElement};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as ToplevelState;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, SERIAL_COUNTER, Size, Transform};
use smithay::wayland::compositor::with_states;
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData};
use smithay::xwayland::X11Surface;
use tracing::{debug, info, warn};

use crate::decoration::layout::TitlebarButton;
use crate::decoration::naming::{badge_color, identity};
use crate::decoration::paint::{Painter, TitlebarSpec};
use crate::focus::KeyFocus;
use crate::input::Gesture;
use crate::render::scene::Chrome;
use crate::state::State;
use crate::theme::Theme;

/// A screen and where it sits among the others.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenInfo {
    pub id: OutputId,
    pub role: OutputRole,
    /// In the global space shared by every screen.
    pub rect: Rect,
}

/// A window the model knows about.
pub struct ManagedWindow {
    pub id: WindowId,
    pub window: Window,
    pub app_id: String,
    pub title: String,
    /// The name of its application, from its desktop entry when it has one.
    pub name: String,
    /// The icon of its application, as its desktop entry names it.
    pub icon: Option<String>,
    /// Last geometry decided by the model, relative to its screen.
    pub geometry: WindowGeometry,
    /// Top-left corner of its screen in the global space.
    pub origin: ModelPoint,
    /// Floating: the size its client chose has been taken once already.
    pub sized: bool,
    /// Floating: its client wants more room than the screen has, so the
    /// compositor imposes the size instead of following it.
    pub oversized: bool,
    /// Floating: the user resized it, so the compositor imposes the size.
    pub user_sized: bool,
    /// How much room its surfaces took at their last commit.
    pub covered: Size<i32, Logical>,
    pub chrome: Chrome,
}

impl ManagedWindow {
    /// The whole decorated window, in the global space.
    pub fn frame(&self) -> Rect {
        self.geometry.frame.translated(self.origin)
    }

    pub fn titlebar(&self) -> Option<Rect> {
        Some(self.geometry.titlebar?.translated(self.origin))
    }

    pub fn client(&self) -> Rect {
        self.geometry.client.translated(self.origin)
    }

    /// Whether its title bar shows buttons: in a slot, they are on its
    /// capsule.
    pub fn buttons(&self) -> bool {
        self.geometry.floating
    }
}

/// A toplevel that has not made its first commit yet.
pub struct Unmapped {
    pub window: Window,
    /// It already asked to open fullscreen: on this screen, if it named one.
    pub fullscreen: Option<Option<OutputId>>,
}

/// A window about to be placed by the model, Wayland or X11.
pub struct NewWindow {
    pub window: Window,
    /// What identifies its application: `app_id`, or `WM_CLASS`.
    pub app_id: String,
    pub title: String,
    /// It follows the habits of its application. A dialog does not: it
    /// opens floating wherever the application lives.
    pub habits: bool,
    /// The screen to open on, when it is not the one under the pointer.
    pub screen: Option<OutputId>,
    /// The size its client asks for, without decoration, when it is known
    /// beforehand (X11).
    pub size: Option<ModelSize>,
    /// It opens fullscreen: on this screen, if it names one.
    pub fullscreen: Option<Option<OutputId>>,
    /// Its client draws its own title bar: the shell adds none.
    pub own_titlebar: bool,
}

pub struct Shell {
    pub desk: Desk,
    pub screens: Vec<ScreenInfo>,
    /// Bottom to top, as the model orders them.
    pub windows: Vec<ManagedWindow>,
    pub unmapped: Vec<Unmapped>,
    /// X11 windows that manage themselves (menus, tooltips, dropdowns),
    /// bottom to top. They are shown where they say, above everything.
    pub overrides: Vec<X11Surface>,
    next_id: u64,
}

impl Shell {
    /// `memory` holds the habits saved by earlier sessions.
    pub fn new(screens: Vec<ScreenInfo>, memory: AppMemory) -> Self {
        let outputs = screens
            .iter()
            .map(|screen| OutputInfo::new(screen.id, screen.role, screen.rect.size()))
            .collect();
        Self {
            desk: Desk::new(outputs, Metrics::default()).with_memory(memory),
            screens,
            windows: Vec::new(),
            unmapped: Vec::new(),
            overrides: Vec::new(),
            next_id: 1,
        }
    }

    pub fn screen(&self, id: OutputId) -> Option<&ScreenInfo> {
        self.screens.iter().find(|screen| screen.id == id)
    }

    pub fn screen_at(&self, point: ModelPoint) -> Option<&ScreenInfo> {
        self.screens
            .iter()
            .find(|screen| screen.rect.contains(point))
    }

    /// The screens changed size or place (the nested window was resized).
    pub fn move_screens(&mut self, screens: Vec<ScreenInfo>) {
        for screen in &screens {
            if let Err(error) = self.desk.set_output_size(screen.id, screen.rect.size()) {
                warn!(%error, "cannot resize a screen");
            }
        }
        self.screens = screens;
    }

    /// The managed window whose main surface is `surface`.
    pub fn by_surface(&self, surface: &WlSurface) -> Option<&ManagedWindow> {
        self.windows
            .iter()
            .find(|managed| is_surface_of(&managed.window, surface))
    }

    pub fn by_surface_mut(&mut self, surface: &WlSurface) -> Option<&mut ManagedWindow> {
        self.windows
            .iter_mut()
            .find(|managed| is_surface_of(&managed.window, surface))
    }

    /// The managed window that is this X11 window.
    pub fn by_x11(&self, window: &X11Surface) -> Option<&ManagedWindow> {
        self.windows
            .iter()
            .find(|managed| managed.window.x11_surface() == Some(window))
    }

    /// The toplevel not placed yet whose surface is `surface`.
    pub fn unmapped_mut(&mut self, surface: &WlSurface) -> Option<&mut Unmapped> {
        self.unmapped
            .iter_mut()
            .find(|unmapped| is_surface_of(&unmapped.window, surface))
    }

    pub fn by_id(&self, id: WindowId) -> Option<&ManagedWindow> {
        self.windows.iter().find(|managed| managed.id == id)
    }

    /// Where a dragged window can be released, in the global space.
    pub fn drop_zones(&self, dragged: WindowId) -> Vec<(Rect, DropTarget, bool)> {
        let zones = self.desk.drop_zones(dragged).into_iter();
        zones
            .filter_map(|zone| {
                let output = match zone.target {
                    DropTarget::Slot(slot) => slot.output,
                    DropTarget::Float(output) => output,
                };
                let origin = self.screen(output)?.rect.origin();
                Some((zone.rect.translated(origin), zone.target, zone.occupied))
            })
            .collect()
    }

    /// Top-left corner, in the global space, of the screen a window is on.
    fn origin_of(&self, geometry: &WindowGeometry) -> ModelPoint {
        self.screen(geometry.output)
            .map_or_else(ModelPoint::default, |screen| screen.rect.origin())
    }

    /// Takes the model's arrangement: geometry of each window, and stacking.
    fn sync(&mut self) {
        let arrangement = self.desk.arrangement();
        let rank = |id: WindowId| arrangement.iter().position(|geometry| geometry.id == id);
        self.windows.sort_by_key(|managed| rank(managed.id));
        for index in 0..self.windows.len() {
            let id = self.windows[index].id;
            if let Some(geometry) = arrangement.iter().find(|geometry| geometry.id == id) {
                let origin = self.origin_of(geometry);
                let managed = &mut self.windows[index];
                managed.geometry = *geometry;
                managed.origin = origin;
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
            let name = managed.name.clone();
            let subtitle = if managed.title == name {
                String::new()
            } else {
                managed.title.clone()
            };
            let spec = TitlebarSpec {
                size: bar.size(),
                title: name,
                subtitle,
                icon: managed.icon.clone(),
                badge: badge_color(&managed.app_id),
                focused: geometry.focused,
                hovered: hover
                    .filter(|(id, _)| *id == managed.id)
                    .map(|(_, button)| button),
                buttons: managed.buttons(),
                theme: theme.kind,
                accent: theme.accent,
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

/// `surface` is the main surface of `window`. An X11 window has none until
/// XWayland pairs them.
fn is_surface_of(window: &Window, surface: &WlSurface) -> bool {
    window.wl_surface().is_some_and(|own| *own == *surface)
}

/// The rectangle an X11 window is given for its content to fill `client`:
/// the window may draw shadows of its own around it, which stay outside.
pub fn x11_rect(client: Rect, shadows: (i32, i32, i32, i32)) -> Rect {
    let (left, right, top, bottom) = shadows;
    Rect::new(
        client.x - left,
        client.y - top,
        client.w + left + right,
        client.h + top + bottom,
    )
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
            .position(|unmapped| is_surface_of(&unmapped.window, surface))
        else {
            return;
        };
        let Unmapped { window, fullscreen } = self.shell.unmapped.swap_remove(index);
        let Some(toplevel) = window.toplevel() else {
            return;
        };
        let (app_id, title) = toplevel_names(toplevel);
        // A client that never asked the compositor for decorations draws
        // them itself.
        let own_titlebar = !crate::handlers::asked_for_decorations(surface);
        self.manage(NewWindow {
            window,
            app_id,
            title,
            habits: true,
            screen: None,
            size: None,
            fullscreen,
            own_titlebar,
        });
    }

    /// A new window, Wayland or X11: the model places it, and its client is
    /// told where it stands.
    pub fn manage(&mut self, new: NewWindow) -> Option<WindowId> {
        let NewWindow {
            window,
            app_id,
            title,
            habits,
            screen,
            size,
            fullscreen,
            own_titlebar,
        } = new;
        let known = screen.filter(|id| self.shell.screen(*id).is_some());
        let pointer = || self.pointer_screen().map(|screen| screen.id);
        let first = || self.shell.screens.first().map(|screen| screen.id);
        let Some(output) = known.or_else(pointer).or_else(first) else {
            warn!("no screen to open a window on");
            return None;
        };

        let id = WindowId(self.shell.next_id);
        self.shell.next_id += 1;
        let app = (habits && !app_id.is_empty()).then(|| AppKey::new(app_id.clone()));
        let bar = if own_titlebar {
            0
        } else {
            self.shell.desk.metrics().titlebar_height
        };
        let mut request = OpenRequest::at(output);
        if let Some(size) = size.filter(|size| size.w > 0 && size.h > 0) {
            request = request.with_size(ModelSize::new(size.w, size.h + bar));
        }
        if let Err(error) = self.shell.desk.open_window(id, app, request) {
            warn!(%error, "the model refused a new window");
            return None;
        }
        if let Err(error) = self.shell.desk.set_title(id, title.clone()) {
            warn!(%error, "cannot title a new window");
        }
        if let Err(error) = self.shell.desk.set_own_titlebar(id, own_titlebar) {
            warn!(%error, "cannot record who draws the title bar");
        }
        let geometry = self
            .shell
            .desk
            .arrangement()
            .into_iter()
            .find(|geometry| geometry.id == id)?;
        let x11 = window.x11_surface().is_some();
        debug!(?id, app_id, title, x11, "window mapped");

        let origin = self.shell.origin_of(&geometry);
        let (name, icon) = identity(&self.catalog, &app_id);
        self.shell.windows.push(ManagedWindow {
            id,
            window,
            app_id,
            title,
            name,
            icon,
            geometry,
            origin,
            // The size of an X11 window is known and imposed from the start.
            sized: x11,
            oversized: false,
            user_sized: false,
            covered: Size::default(),
            chrome: Chrome::default(),
        });
        // It opens fullscreen: its screen is anchored from the start.
        if let Some(output) = fullscreen {
            match self.shell.desk.set_fullscreen(id, output) {
                Ok(()) => info!(?id, "a window opened fullscreen: screen anchored"),
                Err(error) => warn!(%error, "cannot open the window fullscreen"),
            }
            self.shell.sync();
        }
        // Rule 1: a window that opens floating comes with the offer to tidy it.
        let offered = self
            .shell
            .by_id(id)
            .is_some_and(|m| m.geometry.floating && !m.geometry.anchored);
        self.placement_offer = offered.then_some(id);
        self.apply_arrangement();
        self.refresh_pointer_focus();
        Some(id)
    }

    /// A window asks to cover a screen, its own when it names none: that
    /// screen is anchored (game mode).
    pub fn enter_fullscreen(&mut self, id: WindowId, output: Option<OutputId>) {
        match self.shell.desk.set_fullscreen(id, output) {
            Ok(()) => info!(?id, ?output, "fullscreen: screen anchored"),
            Err(error) => warn!(%error, "cannot make the window fullscreen"),
        }
        self.apply_arrangement();
        self.refresh_pointer_focus();
    }

    /// A window leaves fullscreen, by itself or because the user asked: its
    /// screen is released.
    pub fn leave_fullscreen(&mut self, id: WindowId) {
        let was_anchored = self.shell.by_id(id).is_some_and(|m| m.geometry.anchored);
        if let Err(error) = self.shell.desk.set_anchored(id, false) {
            warn!(%error, "cannot take the window out of fullscreen");
        }
        if was_anchored {
            info!(?id, "fullscreen left: screen released");
        }
        self.apply_arrangement();
        self.refresh_pointer_focus();
    }

    /// A toplevel is gone, mapped or not.
    pub fn unmap_window(&mut self, surface: &WlSurface) {
        self.shell
            .unmapped
            .retain(|unmapped| !is_surface_of(&unmapped.window, surface));
        if let Some(id) = self.shell.by_surface(surface).map(|managed| managed.id) {
            self.remove_window(id);
        }
    }

    /// A managed window is gone: its place is given back.
    pub fn remove_window(&mut self, id: WindowId) {
        self.shell.windows.retain(|managed| managed.id != id);
        if let Err(error) = self.shell.desk.close_window(id) {
            warn!(%error, "the model did not know a closed window");
        }
        debug!(?id, "window unmapped");
        self.apply_arrangement();
        self.refresh_pointer_focus();
    }

    /// Asks the client of a window to close it.
    pub fn close_window(&self, id: WindowId) {
        let Some(managed) = self.shell.by_id(id) else {
            return;
        };
        if let Some(toplevel) = managed.window.toplevel() {
            toplevel.send_close();
        } else if let Some(window) = managed.window.x11_surface() {
            if let Err(error) = window.close() {
                warn!(%error, "cannot close an X11 window");
            }
        }
    }

    /// The client changed its title or `app_id`.
    pub fn refresh_names(&mut self, toplevel: &ToplevelSurface) {
        let (app_id, title) = toplevel_names(toplevel);
        if let Some(id) = self.shell.by_surface(toplevel.wl_surface()).map(|m| m.id) {
            self.rename_window(id, app_id, title);
        }
    }

    /// A window is called something else from now on.
    pub fn rename_window(&mut self, id: WindowId, app_id: String, title: String) {
        let Some(managed) = self.shell.windows.iter_mut().find(|m| m.id == id) else {
            return;
        };
        // An application that names itself late gets its name and icon then.
        if managed.app_id != app_id {
            (managed.name, managed.icon) = identity(&self.catalog, &app_id);
        }
        managed.app_id = app_id;
        managed.title = title.clone();
        if let Err(error) = self.shell.desk.set_title(id, title) {
            warn!(%error, "cannot retitle a window");
        }
        self.shell_changed();
        self.queue_redraw();
    }

    /// Makes the Wayland side match the model: sizes and states go to
    /// clients, the keyboard focus follows the focused window.
    pub fn apply_arrangement(&mut self) {
        self.shell.sync();

        for managed in &self.shell.windows {
            let geometry = managed.geometry;
            // Tells the client which output it is on, hence its scale.
            for screen in &self.outputs {
                if screen.id == geometry.output && geometry.visible {
                    managed
                        .window
                        .output_enter(&screen.output, managed.window.bbox());
                } else {
                    managed.window.output_leave(&screen.output);
                }
            }
            if let Some(window) = managed.window.x11_surface() {
                configure_x11(window, managed);
            }
            let Some(toplevel) = managed.window.toplevel() else {
                continue;
            };
            // A floating window has the size its client chooses, unless that
            // is more than the screen offers.
            let imposed = managed.oversized || managed.user_sized;
            let free_size = geometry.floating && !geometry.anchored && !imposed;
            toplevel.with_pending_state(|state| {
                state.size = (!free_size).then(|| (geometry.client.w, geometry.client.h).into());
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
        }

        self.sync_keyboard_focus();
        self.save_habits();
        self.shell_changed();
        self.full_redraw = true;
        self.queue_redraw();
    }

    /// Writes the habits to disk when tidying a window changed them.
    fn save_habits(&mut self) {
        let Some(directory) = &self.state_dir else {
            return;
        };
        let memory = self.shell.desk.memory();
        if *memory == self.saved_habits {
            return;
        }
        match crate::persist::save(directory, memory) {
            Ok(()) => self.saved_habits = memory.clone(),
            Err(error) => warn!(%error, "cannot save habits"),
        }
    }

    /// A client drew: if its window floats, the frame takes the size the
    /// client chose. The first time, the window stays centered where it
    /// opened; later it keeps its top-left corner.
    /// A client turned out to want its decorations from the compositor after
    /// all (it asked once its window was already open).
    pub fn decorate(&mut self, surface: &WlSurface) {
        let Some(id) = self.shell.by_surface(surface).map(|managed| managed.id) else {
            return;
        };
        if self.shell.desk.set_own_titlebar(id, false).is_ok() {
            self.apply_arrangement();
        }
    }

    /// A client asks to be moved, because the user drags its own title bar:
    /// that is the same gesture as dragging the shell's.
    pub fn drag_from_client(&mut self, window: WindowId) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let movable = self
            .shell
            .by_id(window)
            .is_some_and(|managed| !managed.geometry.anchored);
        // Only while a button is held on the client, which is when its
        // implicit grab is active.
        if !movable || self.gesture.is_some() || !pointer.is_grabbed() {
            return;
        }
        // The client keeps its implicit grab until the button is released;
        // what it does with the motion in the meantime does not matter.
        self.gesture = Some(Gesture::Placing { window });
        self.queue_redraw();
    }

    pub fn follow_client_size(&mut self, surface: &WlSurface) {
        let Some(managed) = self.shell.by_surface_mut(surface) else {
            return;
        };
        // What the shell adds above the client: its title bar, if any.
        let bar = managed.geometry.titlebar.map_or(0, |bar| bar.h);
        // An X11 window has the size it is given; it asks for another one
        // with a configure request.
        if managed.window.x11_surface().is_some() {
            return;
        }
        let geometry = managed.geometry;
        let drawn = managed.window.geometry().size;
        let (frame, client) = (geometry.frame, geometry.client);
        if !geometry.floating || geometry.anchored || drawn.w <= 0 || drawn.h <= 0 {
            return;
        }
        // While the user drags an edge, the frame follows the pointer; the
        // client catches up afterwards.
        if matches!(self.gesture, Some(Gesture::Resizing { window, .. }) if window == managed.id) {
            return;
        }
        let first_time = !std::mem::replace(&mut managed.sized, true);
        if (drawn.w, drawn.h) == (client.w, client.h) {
            return;
        }

        let (w, h) = (drawn.w, drawn.h + bar);
        let wanted = if first_time {
            Rect::new(
                frame.x + (frame.w - w) / 2,
                frame.y + (frame.h - h) / 2,
                w,
                h,
            )
        } else {
            Rect::new(frame.x, frame.y, w, h)
        };
        let id = managed.id;
        let was_oversized = managed.oversized;
        if let Err(error) = self.shell.desk.set_floating_rect(id, wanted) {
            warn!(%error, "cannot resize a floating window");
            return;
        }
        let Some(Placement::Floating { rect, .. }) = self.shell.desk.placement(id) else {
            return;
        };
        let oversized = rect.w < w || rect.h < h;
        if let Some(managed) = self.shell.windows.iter_mut().find(|m| m.id == id) {
            managed.oversized = oversized;
        }
        if rect != frame || oversized != was_oversized {
            self.apply_arrangement();
        }
    }

    /// Gives the keyboard to who it belongs to: the panel that holds it if
    /// there is one, else the focused window.
    pub fn sync_keyboard_focus(&mut self) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        // A popup grab owns the keyboard until it ends.
        if keyboard.is_grabbed() {
            return;
        }
        let window = || {
            let focused = self.shell.desk.focused()?;
            KeyFocus::of(&self.shell.by_id(focused)?.window)
        };
        let panel = self.keyboard_panel();
        let panel = panel.map(|panel| KeyFocus::from(panel.wl_surface().clone()));
        let wanted = panel.or_else(window);
        if keyboard.current_focus() == wanted {
            return;
        }
        // X11 has a stacking order of its own, which must agree on who is in
        // front.
        if let (Some(window), Some(xwm)) = (wanted.as_ref().and_then(KeyFocus::x11), &mut self.xwm)
        {
            if let Err(error) = xwm.raise_window(window) {
                debug!(%error, "cannot raise an X11 window");
            }
        }
        keyboard.set_focus(self, wanted, SERIAL_COUNTER.next_serial());
    }
}

/// Tells an X11 window where it is and in what state. X11 clients place
/// their menus from where they believe their window is, so the position is
/// the real one, in the global space.
fn configure_x11(window: &X11Surface, managed: &ManagedWindow) {
    if !window.alive() {
        return;
    }
    let geometry = managed.geometry;
    let mut done = Ok(());
    if window.is_fullscreen() != geometry.anchored {
        done = done.and(window.set_fullscreen(geometry.anchored));
    }
    if window.is_activated() != geometry.focused {
        done = done.and(window.set_activated(geometry.focused));
    }
    if window.is_hidden() != geometry.minimized {
        done = done.and(window.set_hidden(geometry.minimized));
    }
    if let Err(error) = done {
        debug!(%error, "cannot update the state of an X11 window");
    }

    let shadows = window.frame_extents();
    let rect = x11_rect(
        managed.client(),
        (shadows.left, shadows.right, shadows.top, shadows.bottom),
    );
    let rect = crate::render::scene::rectangle(rect);
    if window.last_configure() != rect {
        if let Err(error) = window.configure(rect) {
            debug!(%error, "cannot configure an X11 window");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_x11_window_without_shadows_gets_the_client_area() {
        let client = Rect::new(1928, 46, 800, 600);

        assert_eq!(x11_rect(client, (0, 0, 0, 0)), client);
    }

    #[test]
    fn the_shadows_of_an_x11_window_stay_outside_the_client_area() {
        let client = Rect::new(1928, 46, 800, 600);

        assert_eq!(
            x11_rect(client, (20, 22, 10, 30)),
            Rect::new(1908, 36, 842, 640)
        );
    }
}
