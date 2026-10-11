//! Glue between the placement model and Wayland windows.
//!
//! The model decides; this module applies: sizes go to clients as configures,
//! positions and stacking are kept for rendering and hit-testing.

use coquille_model::{
    AppKey, AppMemory, Desk, DropTarget, LayoutKind, Metrics, OpenRequest, OutputId, OutputInfo,
    OutputRole, Placement, Point as ModelPoint, Rect, SlotId, WindowGeometry, WindowId,
};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::desktop::{Window, space::SpaceElement};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State as ToplevelState;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, SERIAL_COUNTER, Size, Transform};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData};
use tracing::{debug, info, warn};

use crate::decoration::layout::{Buttons, TitlebarButton};
use crate::decoration::naming::{badge_color, display_name};
use crate::decoration::paint::{Painter, TabSpec, TitlebarSpec};
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
    /// Last geometry decided by the model, relative to its screen.
    pub geometry: WindowGeometry,
    /// Top-left corner of its screen in the global space.
    pub origin: ModelPoint,
    /// The "swap with the center" button applies to this window.
    pub show_swap: bool,
    /// The windows stacked in its slot when they show as tabs in its title
    /// bar, in order; empty otherwise.
    pub tabs: Vec<WindowId>,
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

    /// The buttons its title bar shows.
    pub fn buttons(&self) -> Buttons {
        Buttons {
            swap: self.show_swap,
            add_tab: !self.geometry.floating,
        }
    }
}

/// A toplevel that has not made its first commit yet.
pub struct Unmapped {
    pub window: Window,
    /// It already asked to open fullscreen: on this screen, if it named one.
    pub fullscreen: Option<Option<OutputId>>,
}

pub struct Shell {
    pub desk: Desk,
    pub screens: Vec<ScreenInfo>,
    /// Bottom to top, as the model orders them.
    pub windows: Vec<ManagedWindow>,
    pub unmapped: Vec<Unmapped>,
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

    /// The toplevel not placed yet whose surface is `surface`.
    pub fn unmapped_mut(&mut self, surface: &WlSurface) -> Option<&mut Unmapped> {
        self.unmapped
            .iter_mut()
            .find(|unmapped| is_toplevel_of(&unmapped.window, surface))
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

    /// What a window needs besides the model's geometry: where its screen is,
    /// and whether it can be swapped with the center.
    fn placement_of(&self, geometry: &WindowGeometry) -> (ModelPoint, bool) {
        match self.screen(geometry.output) {
            Some(screen) => (
                screen.rect.origin(),
                self.desk.can_swap_with_center(geometry.id),
            ),
            None => (ModelPoint::default(), false),
        }
    }

    /// The stack a window belongs to, when it must show as tabs: as soon as
    /// it holds two windows, and always in the `Tabs` layout.
    fn tabs_of(&self, id: WindowId) -> Vec<WindowId> {
        let Some(Placement::Slot(slot)) = self.desk.placement(id) else {
            return Vec::new();
        };
        let Some(screen) = self.desk.screen(slot.output) else {
            return Vec::new();
        };
        let stack = &screen.slots()[slot.index];
        if stack.len() >= 2 || screen.layout() == LayoutKind::Tabs {
            stack.windows().to_vec()
        } else {
            Vec::new()
        }
    }

    /// Takes the model's arrangement: geometry of each window, and stacking.
    fn sync(&mut self) {
        let arrangement = self.desk.arrangement();
        let rank = |id: WindowId| arrangement.iter().position(|geometry| geometry.id == id);
        self.windows.sort_by_key(|managed| rank(managed.id));
        for index in 0..self.windows.len() {
            let id = self.windows[index].id;
            if let Some(geometry) = arrangement.iter().find(|geometry| geometry.id == id) {
                let (origin, show_swap) = self.placement_of(geometry);
                let tabs = self.tabs_of(id);
                let managed = &mut self.windows[index];
                managed.geometry = *geometry;
                managed.origin = origin;
                managed.show_swap = show_swap;
                managed.tabs = tabs;
            }
        }
    }

    /// Repaints the title bars whose look changed.
    pub fn refresh_chrome(
        &mut self,
        painter: &mut Painter,
        theme: &Theme,
        hover: Option<(WindowId, TitlebarButton)>,
        armed: Option<SlotId>,
    ) {
        let corner_radius = self.desk.metrics().corner_radius;
        // The window whose add-tab button shows as armed.
        let armed = armed.and_then(|slot| {
            let screen = self.desk.screen(slot.output)?;
            screen.slots().get(slot.index)?.active()
        });
        // What each window is called, for the tabs of its neighbours.
        let labels: Vec<(WindowId, String, [u8; 3], bool)> = self
            .windows
            .iter()
            .map(|managed| {
                let label = if managed.title.is_empty() {
                    display_name(&managed.app_id)
                } else {
                    managed.title.clone()
                };
                let minimized = managed.geometry.minimized;
                (managed.id, label, badge_color(&managed.app_id), minimized)
            })
            .collect();

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
            let tabs = managed
                .tabs
                .iter()
                .filter_map(|id| labels.iter().find(|(known, ..)| known == id))
                .map(|(id, label, badge, minimized)| TabSpec {
                    label: label.clone(),
                    badge: *badge,
                    active: *id == managed.id,
                    minimized: *minimized,
                })
                .collect();
            let spec = TitlebarSpec {
                size: bar.size(),
                title: name,
                subtitle,
                badge: badge_color(&managed.app_id),
                focused: geometry.focused,
                hovered: hover
                    .filter(|(id, _)| *id == managed.id)
                    .map(|(_, button)| button),
                buttons: managed.buttons(),
                armed: armed == Some(managed.id),
                theme: theme.kind,
                corner_radius,
                tabs,
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
            .position(|unmapped| is_toplevel_of(&unmapped.window, surface))
        else {
            return;
        };
        let Unmapped { window, fullscreen } = self.shell.unmapped.swap_remove(index);
        let Some(toplevel) = window.toplevel() else {
            return;
        };
        let (app_id, title) = toplevel_names(toplevel);
        let Some(cursor_screen) = self
            .pointer_screen()
            .or(self.shell.screens.first().copied())
        else {
            warn!("no screen to open a window on");
            return;
        };

        let id = WindowId(self.shell.next_id);
        self.shell.next_id += 1;
        let app = (!app_id.is_empty()).then(|| AppKey::new(app_id.clone()));
        let request = OpenRequest::at(cursor_screen.id);
        if let Err(error) = self.shell.desk.open_window(id, app, request) {
            warn!(%error, "the model refused a new window");
            return;
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

        let (origin, show_swap) = self.shell.placement_of(&geometry);
        self.shell.windows.push(ManagedWindow {
            id,
            window,
            app_id,
            title,
            geometry,
            origin,
            show_swap,
            tabs: Vec::new(),
            sized: false,
            oversized: false,
            user_sized: false,
            covered: Size::default(),
            chrome: Chrome::default(),
        });
        // An armed "add a tab" button takes the next window, whatever it is.
        if let Some(slot) = self.armed_slot.take() {
            match self.shell.desk.move_to_slot(id, slot) {
                Ok(()) => self.shell.sync(),
                Err(error) => warn!(%error, "cannot add the window as a tab"),
            }
        }
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
            .retain(|unmapped| !is_toplevel_of(&unmapped.window, surface));
        let Some(id) = self.shell.by_surface(surface).map(|managed| managed.id) else {
            return;
        };
        self.shell.windows.retain(|managed| managed.id != id);
        if let Err(error) = self.shell.desk.close_window(id) {
            warn!(%error, "the model did not know a closed window");
        }
        debug!(?id, "window unmapped");
        self.apply_arrangement();
        self.refresh_pointer_focus();
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
        }

        self.sync_keyboard_focus();
        self.save_habits();
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
    pub fn follow_client_size(&mut self, surface: &WlSurface) {
        let bar = self.shell.desk.metrics().titlebar_height;
        let Some(managed) = self.shell.by_surface_mut(surface) else {
            return;
        };
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
