//! Input events: shortcuts, focus on click, title bar buttons.

use coquille_model::{
    Insets, Lifted, OutputId, Placement, Point as ModelPoint, Rect, Size, StackId, WindowId,
};
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    InputTime, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent, Switch, SwitchState, SwitchToggleEvent,
};
use smithay::desktop::{LayerSurface, WindowSurfaceType};
use smithay::input::keyboard::{FilterResult, Keycode, Keysym};
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, MotionEvent, RelativeMotionEvent,
};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use smithay::wayland::compositor::RegionAttributes;
use smithay::wayland::pointer_constraints::{PointerConstraint, with_pointer_constraint};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::wlr_layer::Layer;
use tracing::{debug, info, warn};

use crate::debug::Step;
use crate::decoration::capsules::CapsuleButton;
use crate::decoration::layout::{TitlebarButton, TitlebarLayout};
use crate::game::{StickyEdges, confine};
use crate::outputs::cross;
use crate::render::scene::Shape;
use crate::resize::Edges;
use crate::shell::{CapsuleHit, ManagedWindow};
use crate::shortcuts::{Action, KeyPress, action_for, settings_command};
use crate::state::State;
use crate::theme::Rgba;

/// Linux input codes of the mouse buttons.
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
/// How far one notch of a mouse wheel scrolls, as libinput reports it.
const WHEEL_NOTCH: f64 = 15.0;
/// Gap between a drop zone's outline and the slot it stands for.
const ZONE_INSET: i32 = 4;
/// How far from the gap between two slots their separation can be grabbed.
const SEPARATOR_REACH: i32 = 8;

/// How far the pointer must travel, button held, before a press on a
/// window or on a capsule becomes a drag.
const DRAG_THRESHOLD: f64 = 8.0;
/// How far a window in a slot (or maximized) must be dragged by its title
/// bar before it leaves its place: a click that slips is not a move.
const UNTIDY_THRESHOLD: f64 = 32.0;
/// The longest a second press on a title bar may come after the first for
/// both to make a double-click.
const DOUBLE_CLICK_MS: u32 = 400;

/// A press on the shell's title bar of a window, kept to recognize a
/// double-click.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarPress {
    pub window: WindowId,
    /// Input time, in milliseconds.
    pub time: u32,
    pub at: Point<f64, Logical>,
}

impl BarPress {
    /// `next` follows this press closely enough, on the same window and
    /// about the same place, to make a double-click with it.
    fn doubled_by(&self, next: &Self) -> bool {
        let travelled = next.at - self.at;
        let near = travelled.x.hypot(travelled.y) <= DRAG_THRESHOLD;
        let soon = next.time.wrapping_sub(self.time) <= DOUBLE_CLICK_MS;
        self.window == next.window && near && soon
    }
}

/// What lies under a point of the global space.
#[derive(Debug, Default)]
struct Hit {
    window: Option<WindowId>,
    /// The client surface there, with its position.
    surface: Option<(WlSurface, Point<f64, Logical>)>,
    /// The point is on the window's title bar.
    titlebar: bool,
    button: Option<TitlebarButton>,
    /// The capsule there, above a slot.
    capsule: Option<CapsuleHit>,
    /// The point is on these edges of a floating window.
    resize: Option<Edges>,
    /// The panel of the shell there (the dock, its drawer).
    panel: Option<LayerSurface>,
}

/// How a client holds the pointer where it is (pointer-constraints).
enum Hold {
    /// The pointer does not move: only the distances travelled are sent.
    Locked,
    /// The pointer stays on the surface, inside this region of it if any.
    Confined(Option<RegionAttributes>),
}

/// What a press on a window becomes once the pointer has travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    /// The window floats and follows the pointer.
    Move,
    /// Drop zones are shown, and releasing on one places the window.
    Place,
    /// Spatial mode: target screens are shown, and releasing on one moves the
    /// maximized window there.
    MoveScreen,
}

/// What the left mouse button is doing since it went down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    /// Down on a title bar, or anywhere on a window with the shell modifier,
    /// not moved yet: a click, or the start of `drag`.
    Pressed {
        window: WindowId,
        at: Point<f64, Logical>,
        button: Option<TitlebarButton>,
        drag: Drag,
    },
    /// Down on a capsule, not moved yet: a click on it or on the button of
    /// it that was under the pointer, or the start of a drag of its window.
    CapsulePressed {
        capsule: CapsuleHit,
        at: Point<f64, Logical>,
    },
    /// Dragging edges of a floating window. `from` is where the pointer went
    /// down and `frame` what the window was then, in the global space.
    Resizing {
        window: WindowId,
        edges: Edges,
        from: Point<f64, Logical>,
        frame: Rect,
    },
    /// Dragging separation `index` between the columns of a screen, with
    /// the shell modifier.
    Splitting { output: OutputId, index: usize },
    /// Dragging a window with the shell modifier, or by its capsule: drop
    /// zones are shown, and releasing on one places the window. `lifted` is
    /// where it stood before being moved by its title bar, if it was:
    /// `Escape` puts it back there.
    Placing {
        window: WindowId,
        lifted: Option<Lifted>,
    },
    /// Dragging a window by its title bar: it floats and follows the
    /// pointer. `grab` is where the pointer holds it, from its corner, and
    /// `width` how wide it was then; `lifted` is where it stood before.
    Moving {
        window: WindowId,
        grab: Point<f64, Logical>,
        width: i32,
        lifted: Option<Lifted>,
    },
    /// Spatial mode: dragging a maximized window by its title bar to drop it
    /// onto another screen.
    MovingScreen { window: WindowId },
}

impl Gesture {
    /// The window this gesture acts on, if any.
    pub fn window(&self) -> Option<WindowId> {
        match *self {
            Self::Pressed { window, .. }
            | Self::Resizing { window, .. }
            | Self::Placing { window, .. }
            | Self::Moving { window, .. }
            | Self::MovingScreen { window } => Some(window),
            Self::CapsulePressed { capsule, .. } => Some(capsule.window),
            Self::Splitting { .. } => None,
        }
    }
}

/// A key that acts on the window switcher while it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwitcherKey {
    Step(bool),
    Cancel,
    Finish,
}

/// What the pointer is doing during an interactive screenshot selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScreenshotMode {
    /// Waiting for the left button to go down.
    SelectingRegion,
    /// Dragging a selection rectangle from `start` to `current`.
    DraggingRegion {
        start: Point<f64, Logical>,
        current: Point<f64, Logical>,
    },
}

impl State {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                self.key(event.key_code(), event.state(), Event::time(&event));
            }
            InputEvent::PointerMotion { event, .. } => {
                self.pointer_relative(event.delta(), event.delta_unaccel(), event.time());
            }
            // The nested window: a position inside the whole canvas.
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let canvas = self.shell.screens.iter().fold((0, 0), |size, screen| {
                    (
                        size.0.max(screen.rect.right()),
                        size.1.max(screen.rect.bottom()),
                    )
                });
                let location = event.position_transformed(canvas.into());
                self.pointer_absolute(location, event.time());
            }
            InputEvent::PointerButton { event, .. } => {
                self.pointer_button(event.button_code(), event.state(), event.time());
            }
            InputEvent::PointerAxis { event, .. } => self.on_axis::<I>(event),
            // Closing the lid is logind's business (it suspends the machine
            // when nobody holds it back, and nothing here does). Opened
            // again, every screen is drawn whole: one woken from sleep may
            // have lost its image.
            InputEvent::SwitchToggle { event } if event.switch() == Some(Switch::Lid) => {
                let closed = event.state() == SwitchState::On;
                info!(closed, "lid");
                if !closed {
                    self.full_redraw = true;
                    self.queue_redraw();
                }
            }
            _ => {}
        }
    }

    /// Puts the pointer in the middle of the main screen: the center one, or
    /// the first one.
    pub fn center_pointer(&mut self) {
        let screens = &self.shell.screens;
        let main = screens
            .iter()
            .find(|screen| screen.role == coquille_model::OutputRole::Center)
            .or(screens.first());
        if let Some(rect) = main.map(|screen| screen.rect) {
            let middle = (
                f64::from(rect.x) + f64::from(rect.w) / 2.0,
                f64::from(rect.y) + f64::from(rect.h) / 2.0,
            );
            self.pointer_moved(middle.into(), InputTime::now());
        }
    }

    /// Plays one step of a debug input script.
    pub fn debug_input(&mut self, step: Step) {
        let time = InputTime::now();
        match step {
            Step::Move { x, y } => self.pointer_absolute((x, y).into(), time),
            Step::Relative { dx, dy } => {
                self.pointer_relative((dx, dy).into(), (dx, dy).into(), time);
            }
            Step::Click | Step::RightClick | Step::MiddleClick => {
                let button = match step {
                    Step::RightClick => BTN_RIGHT,
                    Step::MiddleClick => BTN_MIDDLE,
                    _ => BTN_LEFT,
                };
                self.pointer_button(button, ButtonState::Pressed, time);
                self.pointer_button(button, ButtonState::Released, time);
            }
            Step::Wheel(notches) => {
                let Some(pointer) = self.seat.get_pointer() else {
                    return;
                };
                let frame = AxisFrame::new(time)
                    .source(AxisSource::Wheel)
                    .value(Axis::Vertical, f64::from(notches) * WHEEL_NOTCH)
                    .v120(Axis::Vertical, notches * 120);
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            Step::ButtonDown => self.pointer_button(BTN_LEFT, ButtonState::Pressed, time),
            Step::ButtonUp => self.pointer_button(BTN_LEFT, ButtonState::Released, time),
            Step::Press(keycode) => self.key(Keycode::new(keycode), KeyState::Pressed, time),
            Step::Release(keycode) => self.key(Keycode::new(keycode), KeyState::Released, time),
        }
    }

    fn key(&mut self, keycode: Keycode, key_state: KeyState, time: InputTime) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        let mod_key = self.mod_key;
        let mut tapped = false;
        let mut cancelled = false;
        let mut switch = None;

        let action = keyboard.input::<Option<Action>, _>(
            self,
            keycode,
            key_state,
            serial,
            time,
            |state, modifiers, handle| {
                if state.screenshot_mode.is_some() {
                    let is_escape = handle.raw_syms().contains(&Keysym::Escape);
                    if is_escape && key_state == KeyState::Pressed {
                        state.screenshot_mode = None;
                        state.screenshot_freeze = None;
                        state.full_redraw = true;
                        state.refresh_pointer_focus();
                        state.queue_redraw();
                    }
                    return FilterResult::Intercept(None);
                }
                let keycode = handle.raw_code().raw();
                let is_mod = mod_key.is_key(&handle.raw_syms());
                if key_state == KeyState::Released {
                    // Alt let go ends the switch; the client saw Alt go
                    // down, it sees it go up.
                    let alt = [Keysym::Alt_L, Keysym::Alt_R];
                    if state.switcher.is_some() && handle.raw_syms().iter().any(|s| alt.contains(s))
                    {
                        switch = Some(SwitcherKey::Finish);
                    }
                    tapped = state.mod_tap.key_released(is_mod);
                    // The client never saw the press: hide the release too.
                    let before = state.suppressed_keys.len();
                    state.suppressed_keys.retain(|key| *key != keycode);
                    return if state.suppressed_keys.len() != before {
                        FilterResult::Intercept(None)
                    } else {
                        FilterResult::Forward
                    };
                }
                state
                    .mod_tap
                    .key_pressed(is_mod, mod_key.is_alone(modifiers));
                if state.gesture.is_some() {
                    // Held to show the drop zones in the middle of a drag,
                    // the modifier is not a tap.
                    state.mod_tap.disturb();
                    if handle.raw_syms().contains(&Keysym::Escape) && state.can_cancel_gesture() {
                        state.suppressed_keys.push(keycode);
                        cancelled = true;
                        return FilterResult::Intercept(None);
                    }
                }
                if state.switcher.is_some() {
                    // While choosing, keys go to the switcher, not to the
                    // window that has the focus; modifiers and Tab go on.
                    let syms = handle.raw_syms();
                    let has = |sym: Keysym| syms.contains(&sym);
                    switch = if has(Keysym::Escape) {
                        Some(SwitcherKey::Cancel)
                    } else if has(Keysym::Right) || has(Keysym::Down) {
                        Some(SwitcherKey::Step(true))
                    } else if has(Keysym::Left) || has(Keysym::Up) {
                        Some(SwitcherKey::Step(false))
                    } else {
                        None
                    };
                    let passes = has(Keysym::Tab) || syms.iter().all(|sym| sym.is_modifier_key());
                    if switch.is_some() || !passes {
                        state.suppressed_keys.push(keycode);
                        return FilterResult::Intercept(None);
                    }
                }
                let press = KeyPress {
                    main: mod_key.is_held(modifiers),
                    shift: modifiers.shift,
                    ctrl: modifiers.ctrl,
                    alt: modifiers.alt,
                    syms: &handle.raw_syms(),
                    modified: handle.modified_sym(),
                };
                match action_for(press) {
                    Some(action) => {
                        state.suppressed_keys.push(keycode);
                        FilterResult::Intercept(Some(action))
                    }
                    None => FilterResult::Forward,
                }
            },
        );
        if let Some(Some(action)) = action {
            self.run(action);
        }
        if cancelled {
            self.cancel_gesture();
        }
        match switch {
            Some(SwitcherKey::Step(forward)) => self.switch_window(forward),
            Some(SwitcherKey::Cancel) => self.cancel_switch(),
            Some(SwitcherKey::Finish) => self.finish_switch(),
            None => {}
        }
        // The shell modifier alone, pressed and released: the drawer of the
        // dock, and with it the way out of a game that holds the pointer.
        if tapped {
            self.toggle_launcher();
        }
    }

    fn run(&mut self, action: Action) {
        debug!(?action, "shortcut");
        let screen = self.pointer_screen();
        let desk = &mut self.shell.desk;
        let result = match action {
            Action::Quit => {
                info!("quitting on request");
                self.loop_signal.stop();
                return;
            }
            Action::SpawnTerminal => {
                let terminal = self.terminal.clone();
                self.spawn(&terminal);
                return;
            }
            Action::OpenSettings => {
                let beside = std::env::current_exe().ok();
                let beside = beside.as_deref().and_then(std::path::Path::parent);
                let command = settings_command(beside, self.config_path.as_deref());
                self.spawn(&command);
                return;
            }
            Action::RestoreWindow => {
                if desk.restore_last().is_some() {
                    self.apply_arrangement();
                }
                return;
            }
            Action::LeaveFullscreen => {
                let game = screen.and_then(|screen| desk.game_to_release(screen.id));
                if let Some(game) = game {
                    self.leave_fullscreen(game);
                }
                return;
            }
            Action::SwitchVt(vt) => {
                self.backend.switch_vt(vt);
                return;
            }
            Action::Brightness { up } => {
                // The dock holds the brightness, and shows it.
                self.brightness_key(up);
                return;
            }
            Action::ScreenshotRegion => {
                self.start_region_screenshot();
                return;
            }
            Action::SwitchWindow { forward } => {
                self.switch_window(forward);
                return;
            }
            Action::CycleTab { forward } => {
                match desk.focused().and_then(|id| desk.placement(id)) {
                    Some(Placement::Slot(slot)) => desk.cycle_tab(slot, forward),
                    Some(Placement::Maximized(output)) => {
                        desk.cycle_tab(StackId::Maximized(output), forward)
                    }
                    _ => Ok(()),
                }
            }
        };
        if let Err(error) = result {
            warn!(%error, "shortcut refused");
        }
        self.apply_arrangement();
    }

    /// A mouse moved by `delta`: the pointer moves from where it is, stays
    /// on the screens, and obeys the client that holds it. This is the path
    /// of the hardware backend.
    pub fn pointer_relative(
        &mut self,
        delta: Point<f64, Logical>,
        delta_unaccel: Point<f64, Logical>,
        time: InputTime,
    ) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let from = pointer.current_location();
        let hit = self.hit_test(from);
        let hold = self.hold_at(&hit, from);
        // Games read the distances, whether the pointer moves or not.
        let event = RelativeMotionEvent {
            delta,
            delta_unaccel,
            time,
        };
        pointer.relative_motion(self, hit.surface.clone(), &event);

        let mut wanted = from + delta;
        match hold {
            Some(Hold::Locked) => {
                pointer.frame(self);
                return;
            }
            Some(Hold::Confined(region)) => {
                let inside = |point| self.is_inside(&hit, region.as_ref(), point);
                wanted = confine((from.x, from.y), (wanted.x, wanted.y), inside).into();
            }
            None => {}
        }
        let wanted = self.hold_at_sticky_edge((from.x, from.y), (wanted.x, wanted.y), time);
        let screens: Vec<_> = self.shell.screens.iter().map(|s| s.rect).collect();
        let location = cross((from.x, from.y), wanted, &screens);
        self.pointer_moved(location.into(), time);
        self.refresh_constraint();
    }

    /// Sticky edges: while a game is played, the pointer leaves its screen
    /// only after pushing against the edge for a while. They give way to a
    /// shell gesture, and to a pointer freed with the shell modifier.
    fn hold_at_sticky_edge(
        &mut self,
        from: (f64, f64),
        wanted: (f64, f64),
        time: InputTime,
    ) -> (f64, f64) {
        let game = self
            .shell
            .desk
            .game_in_play()
            .and_then(|output| self.shell.screen(output))
            .map(|screen| screen.rect);
        let free = self.pointer_freed.is_some()
            || self.gesture.is_some()
            || self.screenshot_mode.is_some();
        let Some(game) = game.filter(|_| !free) else {
            self.sticky_edges = StickyEdges::default();
            return wanted;
        };
        let was_pushing = self.sticky_edges.is_pushing();
        let now_ms = time.micros() / 1000;
        let allowed = self
            .sticky_edges
            .filter(from, wanted, &[game], now_ms, self.sticky_edges_ms);
        match (was_pushing, self.sticky_edges.is_pushing()) {
            (false, true) => debug!("pointer held at the edge of the game's screen"),
            (true, false) if allowed == wanted => debug!("pointer let out of the game's screen"),
            _ => {}
        }
        allowed
    }

    /// The pointer is asked to be at `location`: the nested window says
    /// where the host's pointer is. A client that holds the pointer keeps
    /// it; the distance travelled is still told, so that a game can be tried
    /// nested.
    pub fn pointer_absolute(&mut self, location: Point<f64, Logical>, time: InputTime) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let from = pointer.current_location();
        let hit = self.hit_test(from);
        let hold = self.hold_at(&hit, from);
        if let Some(last) = self.last_absolute.replace(location) {
            let delta = location - last;
            let event = RelativeMotionEvent {
                delta,
                delta_unaccel: delta,
                time,
            };
            pointer.relative_motion(self, hit.surface.clone(), &event);
        }

        let held = match hold {
            Some(Hold::Locked) => true,
            Some(Hold::Confined(region)) => {
                !self.is_inside(&hit, region.as_ref(), (location.x, location.y))
            }
            None => false,
        };
        if held {
            pointer.frame(self);
            return;
        }
        self.pointer_moved(location, time);
        self.refresh_constraint();
    }

    /// How the client under the pointer holds it, if it does. A shell
    /// gesture in progress is never held back.
    fn hold_at(&self, hit: &Hit, location: Point<f64, Logical>) -> Option<Hold> {
        if self.gesture.is_some() || self.screenshot_mode.is_some() {
            return None;
        }
        let pointer = self.seat.get_pointer()?;
        let (surface, origin) = hit.surface.as_ref()?;
        let local = (location - *origin).to_i32_round::<i32>();
        with_pointer_constraint(surface, &pointer, |constraint| {
            let constraint = constraint?;
            let applies = constraint.is_active()
                && constraint
                    .region()
                    .is_none_or(|region| region.contains(local));
            if !applies {
                return None;
            }
            Some(match &*constraint {
                PointerConstraint::Locked(_) => Hold::Locked,
                PointerConstraint::Confined(confined) => Hold::Confined(confined.region().cloned()),
            })
        })
    }

    /// `point` is still on the surface that confines the pointer, and in
    /// the region it confines it to.
    fn is_inside(&self, hit: &Hit, region: Option<&RegionAttributes>, point: (f64, f64)) -> bool {
        let Some((surface, origin)) = hit.surface.as_ref() else {
            return false;
        };
        let point = Point::<f64, Logical>::from(point);
        let same_surface = self
            .hit_test(point)
            .surface
            .is_some_and(|(under, _)| under == *surface);
        let local = (point - *origin).to_i32_round::<i32>();
        same_surface && region.is_none_or(|region| region.contains(local))
    }

    /// Lets the client under the pointer hold it, if it asked to: its
    /// window has the focus, the pointer is on its surface, and the user did
    /// not free the pointer from it.
    pub fn refresh_constraint(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        if self.pointer_freed.is_some() || self.gesture.is_some() {
            return;
        }
        let location = pointer.current_location();
        let hit = self.hit_test(location);
        let (Some(window), Some((surface, origin))) = (hit.window, hit.surface) else {
            return;
        };
        if self.shell.desk.focused() != Some(window)
            || pointer.current_focus().as_ref() != Some(&surface)
        {
            return;
        }
        let local = (location - origin).to_i32_round::<i32>();
        with_pointer_constraint(&surface, &pointer, |constraint| {
            let Some(constraint) = constraint else {
                return;
            };
            let in_region = constraint
                .region()
                .is_none_or(|region| region.contains(local));
            if !constraint.is_active() && in_region {
                debug!(?window, "pointer held by its window");
                constraint.activate();
            }
        });
    }

    /// The user tapped the shell modifier: the game under the pointer lets
    /// go of it, until the next click in that window.
    pub fn free_pointer(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let location = pointer.current_location();
        let hit = self.hit_test(location);
        let Some(window) = hit.window else {
            return;
        };
        // Where the client says its own cursor was, if it locked the pointer.
        let mut shown_at = None;
        let held = hit.surface.as_ref().is_some_and(|(surface, origin)| {
            with_pointer_constraint(surface, &pointer, |constraint| match constraint {
                Some(constraint) if constraint.is_active() => {
                    if let PointerConstraint::Locked(locked) = &*constraint {
                        shown_at = locked.cursor_position_hint().map(|hint| *origin + hint);
                    }
                    constraint.deactivate();
                    true
                }
                _ => false,
            })
        });
        let anchored = self
            .shell
            .by_id(window)
            .is_some_and(|managed| managed.geometry.anchored);
        if !(held || anchored) {
            return;
        }
        info!(?window, held, "pointer freed from its window");
        self.pointer_freed = Some(window);
        if let Some(location) = shown_at {
            self.pointer_moved(location, InputTime::now());
        }
        self.queue_redraw();
    }

    /// Where `surface` is in the global space, if the pointer is on it.
    pub fn surface_origin_under_pointer(&self, surface: &WlSurface) -> Option<Point<f64, Logical>> {
        let (under, origin) = self.hit_test(self.pointer_location()).surface?;
        (under == *surface).then_some(origin)
    }

    /// Looks again at what is under the pointer when a panel drew: it may
    /// have grown or shrunk under it. Nothing is said to clients unless the
    /// surface under the pointer is another one.
    pub fn refresh_pointer_focus_over_panels(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let under = self.hit_test(pointer.current_location()).surface;
        if pointer.current_focus() != under.map(|(surface, _)| surface) {
            self.refresh_pointer_focus();
        }
    }

    /// Moves the pointer to `location`, clamped inside the screens, after
    /// screens changed position or role.
    pub fn relocate_pointer(&mut self, location: Point<f64, Logical>) {
        let rects: Vec<Rect> = self.shell.screens.iter().map(|s| s.rect).collect();
        let (x, y) = crate::outputs::constrain((location.x, location.y), &rects);
        self.last_absolute = None;
        self.pointer_moved((x, y).into(), InputTime::now());
        self.refresh_constraint();
    }

    /// Looks again at what is under the pointer, after windows moved without
    /// it moving.
    pub fn refresh_pointer_focus(&mut self) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        if pointer.is_grabbed() || self.gesture.is_some() {
            return;
        }
        self.pointer_moved(pointer.current_location(), InputTime::now());
        self.refresh_constraint();
    }

    fn pointer_moved(&mut self, location: Point<f64, Logical>, time: InputTime) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        if let Some(ScreenshotMode::DraggingRegion { start, .. }) = self.screenshot_mode {
            self.screenshot_mode = Some(ScreenshotMode::DraggingRegion {
                start,
                current: location,
            });
            self.set_cursor(CursorImageStatus::Named(CursorIcon::Crosshair));
            pointer.motion(
                self,
                None,
                &MotionEvent {
                    location,
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                },
            );
            pointer.frame(self);
            self.queue_redraw();
            return;
        } else if self.screenshot_mode == Some(ScreenshotMode::SelectingRegion) {
            self.set_cursor(CursorImageStatus::Named(CursorIcon::Crosshair));
            pointer.motion(
                self,
                None,
                &MotionEvent {
                    location,
                    serial: SERIAL_COUNTER.next_serial(),
                    time,
                },
            );
            pointer.frame(self);
            self.queue_redraw();
            return;
        }
        self.advance_gesture(location);
        // While the shell drags a window, clients see no pointer at all.
        let dragging = matches!(
            self.gesture,
            Some(
                Gesture::Placing { .. }
                    | Gesture::Moving { .. }
                    | Gesture::MovingScreen { .. }
                    | Gesture::Resizing { .. }
                    | Gesture::Splitting { .. }
            )
        );
        let hit = if dragging {
            Hit::default()
        } else {
            self.hit_test(location)
        };

        self.hover = hit.window.zip(hit.button);
        self.capsule_hover = hit.capsule;
        // Freed from a window, the pointer is free until it leaves it.
        if self.pointer_freed.is_some() && self.pointer_freed != hit.window {
            self.pointer_freed = None;
        }
        // Over the shell's own chrome (or during a shell drag, even when a CSD
        // client holds an implicit button grab) no client sets the cursor: it
        // shows what a press would grab or the drag in progress.
        if hit.surface.is_none() && (!pointer.is_grabbed() || dragging) {
            let shape = match self.gesture {
                Some(Gesture::Resizing { edges, .. }) => edges.cursor(),
                Some(Gesture::Splitting { .. }) => CursorIcon::ColResize,
                Some(Gesture::MovingScreen { .. }) => CursorIcon::Grabbing,
                Some(_) => CursorIcon::Default,
                None => match (hit.resize, self.separator_at(location)) {
                    (Some(edges), _) => edges.cursor(),
                    (None, Some(_)) if self.mod_held() => CursorIcon::ColResize,
                    _ => CursorIcon::Default,
                },
            };
            self.set_cursor(CursorImageStatus::Named(shape));
        }

        pointer.motion(
            self,
            hit.surface,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);

        // Rule 7 sends new windows to the side screen used last.
        if let Some(screen) = self.pointer_screen() {
            self.shell.desk.note_output_used(screen.id);
        }
        // The pointer itself may have to be redrawn, and so may a hover.
        self.queue_redraw();
    }

    /// Lets the gesture in progress react to the pointer moving.
    fn advance_gesture(&mut self, location: Point<f64, Logical>) {
        let beyond = |at: Point<f64, Logical>, threshold: f64| {
            let travelled = (location - at).to_i32_round::<i32>();
            f64::from(travelled.x.pow(2) + travelled.y.pow(2)).sqrt() > threshold
        };
        let far = |at| beyond(at, DRAG_THRESHOLD);
        let floating = |window| {
            self.shell
                .by_id(window)
                .is_some_and(|managed| managed.geometry.floating)
        };
        match self.gesture {
            Some(Gesture::Pressed {
                window,
                at,
                button: None,
                drag: Drag::Move,
            }) if (!self.shell.desk.is_spatial() || floating(window))
                && beyond(
                    at,
                    if floating(window) {
                        DRAG_THRESHOLD
                    } else {
                        UNTIDY_THRESHOLD
                    },
                ) =>
            {
                self.start_moving(window, at, location)
            }
            Some(Gesture::Pressed {
                window,
                at,
                button: None,
                drag: Drag::MoveScreen,
            }) if beyond(at, UNTIDY_THRESHOLD) => {
                if !self.shell.desk.screen_drop_zones(window).is_empty() {
                    self.gesture = Some(Gesture::MovingScreen { window });
                }
            }
            Some(
                Gesture::Pressed {
                    window,
                    at,
                    button: None,
                    drag: Drag::Place,
                }
                | Gesture::CapsulePressed {
                    capsule:
                        CapsuleHit {
                            window,
                            button: None,
                            ..
                        },
                    at,
                },
            ) if far(at) => {
                self.gesture = Some(Gesture::Placing {
                    window,
                    lifted: None,
                })
            }
            // The modifier pressed while moving a window: it stops where it
            // is, and the drop zones show where it can go.
            Some(Gesture::Moving { window, lifted, .. }) if self.mod_held() => {
                self.gesture = Some(Gesture::Placing { window, lifted });
            }
            Some(Gesture::Moving {
                window,
                grab,
                width,
                ..
            }) => self.move_floating(window, location, grab, width),
            Some(Gesture::Resizing {
                window,
                edges,
                from,
                frame,
            }) => {
                let moved = (location - from).to_i32_round::<i32>();
                self.resize_floating(
                    window,
                    edges.apply(frame, moved.x, moved.y, self.min_size()),
                );
            }
            Some(Gesture::Splitting { output, index }) => {
                self.move_separation(output, index, location)
            }
            _ => {}
        }
    }

    /// The smallest a floating window may be dragged to.
    fn min_size(&self) -> Size {
        Size::new(160, self.shell.desk.metrics().titlebar_height + 60)
    }

    fn mod_held(&self) -> bool {
        self.seat
            .get_keyboard()
            .is_some_and(|keyboard| self.mod_key.is_held(&keyboard.modifier_state()))
    }

    /// Gives a floating window the frame `wanted` (global space), on its own
    /// screen. From then on the compositor imposes its size on the client.
    fn resize_floating(&mut self, window: WindowId, wanted: Rect) {
        let Some(managed) = self.shell.windows.iter_mut().find(|m| m.id == window) else {
            return;
        };
        managed.user_sized = true;
        managed.oversized = false;
        let output = managed.geometry.output;
        let local = wanted.translated(ModelPoint::new(-managed.origin.x, -managed.origin.y));
        if self.shell.desk.float_window(window, output, local).is_ok() {
            self.apply_arrangement();
        }
    }

    /// The screen and the separation between its columns close enough to
    /// `location` to be grabbed.
    fn separator_at(&self, location: Point<f64, Logical>) -> Option<(OutputId, usize)> {
        let point = model_point(location);
        let screen = self.shell.screen_at(point)?;
        let gaps = self.shell.desk.separators(screen.id);
        let index = gaps.iter().position(|gap| {
            let gap = gap.translated(screen.rect.origin());
            let reach = Rect::new(
                gap.x - SEPARATOR_REACH,
                gap.y - SEPARATOR_REACH,
                gap.w + 2 * SEPARATOR_REACH,
                gap.h + 2 * SEPARATOR_REACH,
            );
            reach.contains(point)
        })?;
        Some((screen.id, index))
    }

    /// Puts separation `index` of a screen under the pointer.
    fn move_separation(&mut self, output: OutputId, index: usize, location: Point<f64, Logical>) {
        let Some(origin) = self.shell.screen(output).map(|screen| screen.rect.origin()) else {
            return;
        };
        let point = model_point(location);
        let local = ModelPoint::new(point.x - origin.x, point.y - origin.y);
        if self.shell.desk.move_separator(output, index, local) == Ok(true) {
            self.apply_arrangement();
        }
    }

    /// `window` starts following the pointer, which took hold of it at
    /// `held` and is now at `pointer`.
    pub fn start_moving(
        &mut self,
        window: WindowId,
        held: Point<f64, Logical>,
        pointer: Point<f64, Logical>,
    ) {
        let Some(frame) = self.shell.by_id(window).map(ManagedWindow::frame) else {
            return;
        };
        let corner = Point::<f64, Logical>::from((f64::from(frame.x), f64::from(frame.y)));
        let grab = held - corner;
        self.gesture = Some(Gesture::Moving {
            window,
            grab,
            width: frame.w,
            lifted: self.shell.desk.lift(window),
        });
        self.move_floating(window, pointer, grab, frame.w);
    }

    /// Makes `window` float with the pointer holding it at `grab`, taken
    /// when it was `width` wide.
    fn move_floating(
        &mut self,
        window: WindowId,
        pointer: Point<f64, Logical>,
        grab: Point<f64, Logical>,
        width: i32,
    ) {
        let point = model_point(pointer);
        let (Some(screen), Some(managed)) = (
            self.shell.screen_at(point).copied(),
            self.shell.by_id(window),
        ) else {
            return;
        };
        let size = managed.frame().size();
        // Out of a slot, a window goes back to the size its client chooses:
        // the pointer keeps holding the same share of its title bar.
        let grab = Point::from((held_at(grab.x, width, size.w), grab.y));
        if let Some(Gesture::Moving {
            grab: held,
            width: was,
            ..
        }) = &mut self.gesture
        {
            *held = grab;
            *was = size.w;
        }
        let corner = model_point(pointer - grab);
        let rect = Rect::new(
            corner.x - screen.rect.x,
            corner.y - screen.rect.y,
            size.w,
            size.h,
        );
        // Refused on a screen held by a fullscreen window: the window then
        // simply stays where it was.
        if self
            .shell
            .desk
            .float_window(window, screen.id, rect)
            .is_ok()
        {
            self.apply_arrangement();
        }
    }

    /// Places a dragged window where it was released.
    fn drop_window(&mut self, window: WindowId, location: Point<f64, Logical>) {
        let point = model_point(location);
        let zones = self.shell.drop_zones(window);
        let Some((_, slot, _)) = zones.into_iter().find(|(rect, ..)| rect.contains(point)) else {
            return;
        };
        if let Err(error) = self.shell.desk.move_to_slot(window, slot) {
            warn!(%error, "cannot place the window there");
        }
        self.apply_arrangement();
    }

    /// Moves a spatial window to the screen under `location` when released on
    /// one of its target screens.
    fn drop_on_screen(&mut self, window: WindowId, location: Point<f64, Logical>) {
        let point = model_point(location);
        let Some(target) = self.shell.screen_at(point).map(|screen| screen.id) else {
            return;
        };
        let allowed = self
            .shell
            .desk
            .screen_drop_zones(window)
            .into_iter()
            .any(|zone| zone.output == target);
        if !allowed {
            return;
        }
        if let Err(error) = self.shell.desk.move_to_screen(window, target) {
            warn!(%error, "cannot move window to screen");
        }
        self.apply_arrangement();
    }

    /// `Escape` can end the gesture in progress: a window being moved,
    /// placed or resized.
    fn can_cancel_gesture(&self) -> bool {
        matches!(
            self.gesture,
            Some(
                Gesture::Moving { .. }
                    | Gesture::Placing { .. }
                    | Gesture::MovingScreen { .. }
                    | Gesture::Resizing { .. }
            )
        )
    }

    /// The user changed their mind (`Escape`): the window dragged goes back
    /// where it was, its habit with it, and releasing the button does
    /// nothing more.
    fn cancel_gesture(&mut self) {
        match self.gesture.take() {
            Some(
                Gesture::Moving {
                    lifted: Some(lifted),
                    ..
                }
                | Gesture::Placing {
                    lifted: Some(lifted),
                    ..
                },
            ) => {
                if let Err(error) = self.shell.desk.put_down(lifted) {
                    warn!(%error, "cannot put the window back where it was");
                }
                self.apply_arrangement();
            }
            Some(Gesture::Resizing { window, frame, .. }) => self.resize_floating(window, frame),
            Some(Gesture::MovingScreen { .. }) => {
                self.set_cursor(CursorImageStatus::Named(CursorIcon::Default));
            }
            _ => {}
        }
        // The release that ends the drag belongs to no client, unless a client
        // holds an implicit button grab (CSD move_request) that needs the
        // release to clear.
        if !self.seat.get_pointer().is_some_and(|p| p.is_grabbed()) {
            self.swallowed_press = true;
        }
        self.refresh_pointer_focus();
        self.queue_redraw();
    }

    /// The hints drawn above the windows: while a window is dragged to be
    /// placed, every place it can go, the one under the pointer standing
    /// out.
    pub fn overlay(&self) -> Vec<Shape> {
        if let Some(mode) = self.screenshot_mode {
            let dim_color = Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 0x50,
            };
            match mode {
                ScreenshotMode::SelectingRegion => {
                    let mut shapes = Vec::new();
                    for screen in &self.shell.screens {
                        shapes.push(Shape {
                            outline: screen.rect,
                            area: screen.rect,
                            color: dim_color,
                            radii: [0; 4],
                            border: 0,
                        });
                    }
                    return shapes;
                }
                ScreenshotMode::DraggingRegion { start, current } => {
                    let p1 = model_point(start);
                    let p2 = model_point(current);
                    let x = p1.x.min(p2.x);
                    let y = p1.y.min(p2.y);
                    let w = (p1.x - p2.x).abs();
                    let h = (p1.y - p2.y).abs();
                    let sel = Rect::new(x, y, w, h);

                    let mut shapes = Vec::new();
                    for screen in &self.shell.screens {
                        let s = screen.rect;
                        match s.intersection(sel) {
                            Some(inter) if inter.w > 0 && inter.h > 0 => {
                                if inter.y > s.y {
                                    let band = Rect::new(s.x, s.y, s.w, inter.y - s.y);
                                    shapes.push(Shape {
                                        outline: band,
                                        area: band,
                                        color: dim_color,
                                        radii: [0; 4],
                                        border: 0,
                                    });
                                }
                                if s.bottom() > inter.bottom() {
                                    let band = Rect::new(
                                        s.x,
                                        inter.bottom(),
                                        s.w,
                                        s.bottom() - inter.bottom(),
                                    );
                                    shapes.push(Shape {
                                        outline: band,
                                        area: band,
                                        color: dim_color,
                                        radii: [0; 4],
                                        border: 0,
                                    });
                                }
                                if inter.x > s.x {
                                    let band = Rect::new(s.x, inter.y, inter.x - s.x, inter.h);
                                    shapes.push(Shape {
                                        outline: band,
                                        area: band,
                                        color: dim_color,
                                        radii: [0; 4],
                                        border: 0,
                                    });
                                }
                                if s.right() > inter.right() {
                                    let band = Rect::new(
                                        inter.right(),
                                        inter.y,
                                        s.right() - inter.right(),
                                        inter.h,
                                    );
                                    shapes.push(Shape {
                                        outline: band,
                                        area: band,
                                        color: dim_color,
                                        radii: [0; 4],
                                        border: 0,
                                    });
                                }
                            }
                            _ => {
                                shapes.push(Shape {
                                    outline: s,
                                    area: s,
                                    color: dim_color,
                                    radii: [0; 4],
                                    border: 0,
                                });
                            }
                        }
                    }

                    if w > 0 && h > 0 {
                        let accent = self.theme.accent;
                        shapes.push(Shape {
                            outline: sel,
                            area: sel,
                            color: accent,
                            radii: [0; 4],
                            border: 2,
                        });
                        shapes.push(Shape {
                            outline: sel,
                            area: sel,
                            color: Rgba { a: 0x18, ..accent },
                            radii: [0; 4],
                            border: 0,
                        });
                    }

                    return shapes;
                }
            }
        }
        let point = model_point(self.pointer_location());
        let radius = self.shell.desk.metrics().corner_radius;
        let accent = self.theme.accent;
        let tint = |alpha: u8| Rgba { a: alpha, ..accent };

        let mut shapes = Vec::new();
        let mut push_zone = |outline: Rect, aimed: bool| {
            let zone = Shape {
                outline,
                area: outline,
                color: tint(if aimed { 0xff } else { 0x8c }),
                radii: [radius; 4],
                border: 2,
            };
            // Topmost first: the outline, then its rim, then its fill.
            shapes.push(zone);
            shapes.push(Shape {
                color: self.theme.rim,
                border: 1,
                ..zone
            });
            shapes.push(Shape {
                color: tint(if aimed { 0x4d } else { 0x1a }),
                border: 0,
                ..zone
            });
        };

        match self.gesture {
            Some(Gesture::Placing { window, .. }) => {
                for (rect, ..) in self.shell.drop_zones(window) {
                    let aimed = rect.contains(point);
                    push_zone(rect.inset(Insets::uniform(ZONE_INSET)), aimed);
                }
            }
            Some(Gesture::MovingScreen { window }) => {
                let inset = self.shell.desk.metrics().gap;
                let pointer_output = self.shell.screen_at(point).map(|screen| screen.id);
                for (rect, output) in self.shell.screen_drop_zones(window) {
                    let aimed = pointer_output == Some(output);
                    push_zone(rect.inset(Insets::uniform(inset)), aimed);
                }
            }
            _ => {}
        }
        shapes
    }

    fn pointer_button(&mut self, button: u32, state: ButtonState, time: InputTime) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let location = pointer.current_location();
        if let Some(mode) = self.screenshot_mode {
            match (state, button) {
                (ButtonState::Pressed, BTN_RIGHT) => {
                    self.screenshot_mode = None;
                    self.screenshot_freeze = None;
                    self.full_redraw = true;
                    self.refresh_pointer_focus();
                    self.queue_redraw();
                }
                (ButtonState::Pressed, BTN_LEFT) => {
                    self.screenshot_mode = Some(ScreenshotMode::DraggingRegion {
                        start: location,
                        current: location,
                    });
                    self.queue_redraw();
                }
                (ButtonState::Released, BTN_LEFT) => {
                    if let ScreenshotMode::DraggingRegion { start, .. } = mode {
                        let p1 = model_point(start);
                        let p2 = model_point(location);
                        let x = p1.x.min(p2.x);
                        let y = p1.y.min(p2.y);
                        let w = (p1.x - p2.x).abs();
                        let h = (p1.y - p2.y).abs();
                        if w >= 4 && h >= 4 {
                            self.screenshot_mode = None;
                            let freeze = self.screenshot_freeze.take();
                            self.full_redraw = true;
                            self.refresh_pointer_focus();
                            self.queue_redraw();
                            let rect = Rect::new(x, y, w, h);
                            if let Err(error) =
                                crate::screenshot::save_frozen_region(self, freeze.as_ref(), rect)
                            {
                                warn!(%error, "cannot capture region screenshot");
                            }
                        } else {
                            self.screenshot_mode = Some(ScreenshotMode::SelectingRegion);
                            self.queue_redraw();
                        }
                    }
                }
                _ => {}
            }
            return;
        }
        let serial = SERIAL_COUNTER.next_serial();
        let mut for_clients = true;
        let mut ended_screen_drag = false;
        self.mod_tap.disturb();
        if state == ButtonState::Pressed {
            // A click gives the pointer back to the game it was freed from.
            self.pointer_freed = None;
        }

        match state {
            // During a grab (a popup menu, a drag) clicks belong to the grab.
            ButtonState::Pressed if !pointer.is_grabbed() => {
                let hit = self.hit_test(location);
                if let Some(window) = hit.window {
                    self.focus_window(window);
                }
                // A click on a panel that accepts the keyboard gives it to
                // that panel; a click on a window or on a capsule takes it
                // back.
                if hit.panel.is_some() || hit.window.is_some() || hit.capsule.is_some() {
                    self.panel_focus = hit.panel.clone();
                    self.sync_keyboard_focus();
                }
                if button == BTN_LEFT {
                    for_clients = !self.start_gesture(&hit, location, time);
                    self.swallowed_press = !for_clients;
                }
            }
            ButtonState::Released if button == BTN_LEFT => {
                match self.gesture.take() {
                    // A click counts when it ends on the button it began on.
                    Some(Gesture::Pressed {
                        window,
                        button: Some(pressed),
                        ..
                    }) => {
                        let hit = self.hit_test(location);
                        if hit.window.zip(hit.button) == Some((window, pressed)) {
                            self.click_titlebar_button(window, pressed);
                        }
                    }
                    Some(Gesture::CapsulePressed { capsule, .. }) => {
                        let released = self.hit_test(location).capsule;
                        self.click_capsule(capsule, released);
                    }
                    Some(Gesture::Placing { window, .. }) => self.drop_window(window, location),
                    Some(Gesture::MovingScreen { window }) => {
                        ended_screen_drag = true;
                        self.set_cursor(CursorImageStatus::Named(CursorIcon::Default));
                        self.drop_on_screen(window, location);
                    }
                    // The client drew during the drag without the frame
                    // following: settle on what it has now.
                    Some(Gesture::Resizing { window, .. }) => {
                        let surface = self
                            .shell
                            .by_id(window)
                            .and_then(|managed| managed.window.wl_surface())
                            .map(|surface| surface.into_owned());
                        if let Some(surface) = surface {
                            self.follow_client_size(&surface);
                        }
                    }
                    // Its widths are kept once the separation is let go.
                    Some(Gesture::Splitting { .. }) => self.save_proportions(),
                    Some(Gesture::Pressed { .. } | Gesture::Moving { .. }) | None => {}
                }
                for_clients = !std::mem::take(&mut self.swallowed_press);
                self.queue_redraw();
            }
            _ => {}
        }

        if for_clients {
            pointer.button(
                self,
                &ButtonEvent {
                    button,
                    state,
                    serial,
                    time,
                },
            );
            pointer.frame(self);
        }
        if state == ButtonState::Pressed {
            self.refresh_constraint();
        } else if ended_screen_drag {
            self.refresh_pointer_focus();
        } else {
            self.refresh_pointer_focus_over_panels();
        }
    }

    /// Decides what a left press starts. Returns whether the shell keeps the
    /// press for itself.
    fn start_gesture(&mut self, hit: &Hit, location: Point<f64, Logical>, time: InputTime) -> bool {
        let mod_held = self.mod_held();
        // With the modifier, the separation wins over the windows next to it.
        if let Some((output, index)) = self.separator_at(location).filter(|_| mod_held) {
            self.gesture = Some(Gesture::Splitting { output, index });
            return true;
        }
        if let Some(capsule) = hit.capsule {
            self.gesture = Some(Gesture::CapsulePressed {
                capsule,
                at: location,
            });
            return true;
        }
        let Some(managed) = hit.window.and_then(|window| self.shell.by_id(window)) else {
            return false;
        };
        let window = managed.id;
        let anchored = managed.geometry.anchored;
        let floating = managed.geometry.floating;
        let spatial = self.shell.desk.is_spatial();

        if let Some(edges) = hit.resize.filter(|_| !mod_held && (floating || !spatial)) {
            self.gesture = Some(Gesture::Resizing {
                window,
                edges,
                from: location,
                frame: managed.frame(),
            });
            return true;
        }
        if mod_held && !anchored && !spatial {
            // Anywhere on the window: dragged from there, it shows where it
            // can be placed.
            self.gesture = Some(Gesture::Pressed {
                window,
                at: location,
                button: None,
                drag: Drag::Place,
            });
            return true;
        }
        if hit.titlebar && hit.button.is_none() && !spatial {
            // Twice on its title bar: maximized, or back in its place.
            let press = BarPress {
                window,
                time: time.millis(),
                at: location,
            };
            let previous = self.last_bar_press.replace(press);
            if previous.is_some_and(|previous| previous.doubled_by(&press)) {
                self.last_bar_press = None;
                self.toggle_maximized(window);
                return true;
            }
        }
        if hit.titlebar {
            // Classic mode (or a floating dialog): dragged by its title bar, it
            // floats and follows the pointer. Spatial maximized windows drag to
            // move onto another screen.
            let drag = if spatial && !floating {
                Drag::MoveScreen
            } else {
                Drag::Move
            };
            self.gesture = Some(Gesture::Pressed {
                window,
                at: location,
                button: hit.button,
                drag,
            });
        }
        false
    }

    fn on_axis<I: InputBackend>(&mut self, event: I::PointerAxisEvent) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let source = event.source();
        let mut frame = AxisFrame::new(event.time()).source(source);
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let discrete = event.amount_v120(axis);
            let amount = event
                .amount(axis)
                .unwrap_or_else(|| discrete.unwrap_or(0.0) * 15.0 / 120.0);
            if amount != 0.0 {
                frame = frame.value(axis, amount);
                if let Some(discrete) = discrete {
                    frame = frame.v120(axis, discrete as i32);
                }
            }
            if source == AxisSource::Finger && event.amount(axis) == Some(0.0) {
                frame = frame.stop(axis);
            }
        }
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    fn focus_window(&mut self, window: WindowId) {
        if self.shell.desk.focused() == Some(window) {
            return;
        }
        if let Err(error) = self.shell.desk.focus(window) {
            warn!(%error, "cannot focus a window");
        }
        self.apply_arrangement();
    }

    /// A press on a capsule ended without becoming a drag: on a button, it
    /// counts when it ends on that same button; elsewhere, it shows the
    /// window of the capsule, which also brings it back when minimized.
    fn click_capsule(&mut self, pressed: CapsuleHit, released: Option<CapsuleHit>) {
        match pressed.button {
            Some(_) if released != Some(pressed) => {}
            Some(CapsuleButton::Close) => self.close_window(pressed.window),
            Some(CapsuleButton::Minimize) => self.minimize_window(pressed.window),
            Some(CapsuleButton::Maximize) => self.toggle_maximized(pressed.window),
            None => {
                let desk = &mut self.shell.desk;
                if let Err(error) = desk.activate_tab(pressed.stack, pressed.index) {
                    warn!(%error, "cannot show the window of this capsule");
                }
                self.apply_arrangement();
            }
        }
    }

    fn click_titlebar_button(&mut self, window: WindowId, button: TitlebarButton) {
        match button {
            TitlebarButton::Close => self.close_window(window),
            TitlebarButton::Minimize => self.minimize_window(window),
            TitlebarButton::Maximize => self.toggle_maximized(window),
        }
    }

    /// Hides a window until it is restored (its tab, or the shortcut).
    pub fn minimize_window(&mut self, window: WindowId) {
        if let Err(error) = self.shell.desk.minimize(window) {
            warn!(%error, "cannot minimize this window");
        }
        self.apply_arrangement();
    }

    /// Finds what is under `location`, in the order things are drawn: the
    /// panels above everything, menus, the panels above
    /// the windows, the windows from the top one down, the capsules, then
    /// the panels under them.
    fn hit_test(&self, location: Point<f64, Logical>) -> Hit {
        let panels = self.panels();
        let panel_at = |layers: &[Layer]| {
            let mut on_layers = panels.iter().filter(|panel| layers.contains(&panel.layer));
            on_layers.find_map(|panel| {
                let origin = panel.origin.to_f64();
                let kind = WindowSurfaceType::ALL;
                let (surface, offset) = panel.surface.surface_under(location - origin, kind)?;
                Some(Hit {
                    surface: Some((surface, offset.to_f64() + origin)),
                    panel: Some(panel.surface.clone()),
                    ..Hit::default()
                })
            })
        };
        if let Some(hit) = panel_at(&[Layer::Overlay]) {
            return hit;
        }

        let visible = || {
            self.shell
                .windows
                .iter()
                .rev()
                .filter(|managed| managed.geometry.visible)
        };
        // Where the window's surface tree starts.
        let origin = |managed: &ManagedWindow| {
            let client = managed.client();
            let geometry = managed.window.geometry();
            Point::<i32, Logical>::from((client.x, client.y)) - geometry.loc
        };
        let surface_at = |managed: &ManagedWindow, kind: WindowSurfaceType| {
            let origin = origin(managed).to_f64();
            managed
                .window
                .surface_under(location - origin, kind)
                .map(|(surface, offset)| (surface, offset.to_f64() + origin))
        };

        let point = model_point(location);
        // X11 menus, tooltips and dropdowns sit above every window.
        for window in self.shell.visible_overrides().rev() {
            let origin = window.last_configure().loc;
            let under =
                window.surface_under(location - origin.to_f64(), (0, 0), WindowSurfaceType::ALL);
            if let Some((surface, offset)) = under {
                return Hit {
                    surface: Some((surface, (offset + origin).to_f64())),
                    ..Hit::default()
                };
            }
        }

        for managed in visible() {
            if let Some(surface) = surface_at(managed, WindowSurfaceType::POPUP) {
                return Hit {
                    window: Some(managed.id),
                    surface: Some(surface),
                    ..Hit::default()
                };
            }
        }

        if let Some(hit) = panel_at(&[Layer::Top]) {
            return hit;
        }

        for managed in visible() {
            let resizable = managed.geometry.floating && !managed.geometry.anchored;
            if let Some(edges) = Edges::at(managed.frame(), point).filter(|_| resizable) {
                return Hit {
                    window: Some(managed.id),
                    resize: Some(edges),
                    ..Hit::default()
                };
            }
            if let Some(bar) = managed.titlebar().filter(|bar| bar.contains(point)) {
                let layout =
                    TitlebarLayout::new(bar.size(), managed.buttons(self.shell.desk.is_spatial()));
                let local = ModelPoint::new(point.x - bar.x, point.y - bar.y);
                let button = layout.button_at(local);
                return Hit {
                    window: Some(managed.id),
                    surface: None,
                    titlebar: true,
                    button,
                    ..Hit::default()
                };
            }
            if managed.client().contains(point) {
                let kind = WindowSurfaceType::TOPLEVEL | WindowSurfaceType::SUBSURFACE;
                return Hit {
                    window: Some(managed.id),
                    surface: surface_at(managed, kind),
                    ..Hit::default()
                };
            }
        }
        // The capsules never cover a tidied window, and floating ones
        // cover them.
        if let Some(capsule) = self.shell.capsule_at(point) {
            return Hit {
                capsule: Some(capsule),
                ..Hit::default()
            };
        }
        panel_at(&[Layer::Bottom, Layer::Background]).unwrap_or_default()
    }
}

/// Where the pointer holds a window that went from `was` to `now` wide,
/// when it held it at `grab` from its left edge: at the same share of its
/// width.
fn held_at(grab: f64, was: i32, now: i32) -> f64 {
    if was == now || was <= 0 {
        return grab;
    }
    grab * f64::from(now) / f64::from(was)
}

/// The pixel a position falls in.
fn model_point(location: Point<f64, Logical>) -> ModelPoint {
    ModelPoint::new(location.x.floor() as i32, location.y.floor() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_that_keeps_its_width_is_held_where_it_was_taken() {
        assert_eq!(held_at(300.0, 800, 800), 300.0);
    }

    #[test]
    fn a_window_that_shrinks_stays_under_the_pointer() {
        // Taken near the right end of a wide title bar: still near the
        // right end of the narrow one, not past it.
        assert_eq!(held_at(1500.0, 2000, 800), 600.0);
    }

    #[test]
    fn a_window_that_grows_is_held_at_the_same_share() {
        assert_eq!(held_at(200.0, 800, 1600), 400.0);
    }

    #[test]
    fn led_state_conversion_sets_num_lock_flag() {
        use smithay::input::keyboard::LedState;
        use smithay::reexports::input::Led;

        let state = LedState {
            num: Some(true),
            caps: Some(false),
            scroll: None,
        };
        let leds: Led = state.into();
        assert!(leds.contains(Led::NUMLOCK));
        assert!(!leds.contains(Led::CAPSLOCK));
        assert!(!leds.contains(Led::SCROLLLOCK));

        let state_off = LedState {
            num: Some(false),
            caps: Some(true),
            scroll: Some(false),
        };
        let leds_off: Led = state_off.into();
        assert!(!leds_off.contains(Led::NUMLOCK));
        assert!(leds_off.contains(Led::CAPSLOCK));
    }

    #[test]
    fn xkb_num_lock_key_toggles_modifier() {
        use smithay::input::keyboard::xkb;

        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "",
            "",
            "fr",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
        .unwrap();
        let mut state = xkb::State::new(&keymap);

        let num_mod = keymap.mod_get_index(xkb::MOD_NAME_NUM);
        assert_ne!(num_mod, xkb::MOD_INVALID);

        assert!(!state.mod_index_is_active(num_mod, xkb::STATE_MODS_LOCKED));

        // Keycode 77 is KEY_NUMLOCK in XKB (evdev 69 + 8)
        let num_lock_kc = xkb::Keycode::new(77);
        state.update_key(num_lock_kc, xkb::KeyDirection::Down);
        state.update_key(num_lock_kc, xkb::KeyDirection::Up);

        assert!(state.mod_index_is_active(num_mod, xkb::STATE_MODS_LOCKED));

        state.update_key(num_lock_kc, xkb::KeyDirection::Down);
        state.update_key(num_lock_kc, xkb::KeyDirection::Up);

        assert!(!state.mod_index_is_active(num_mod, xkb::STATE_MODS_LOCKED));
    }

    fn bar_press(window: u64, time: u32, x: f64) -> BarPress {
        BarPress {
            window: WindowId(window),
            time,
            at: Point::from((x, 20.0)),
        }
    }

    #[test]
    fn two_quick_presses_at_the_same_place_make_a_double_click() {
        let first = bar_press(1, 1000, 300.0);

        assert!(first.doubled_by(&bar_press(1, 1350, 304.0)));
    }

    #[test]
    fn a_slow_second_press_is_a_new_click() {
        let first = bar_press(1, 1000, 300.0);

        assert!(!first.doubled_by(&bar_press(1, 1401, 300.0)));
    }

    #[test]
    fn a_second_press_elsewhere_or_on_another_window_is_a_new_click() {
        let first = bar_press(1, 1000, 300.0);

        assert!(!first.doubled_by(&bar_press(1, 1100, 320.0)));
        assert!(!first.doubled_by(&bar_press(2, 1100, 300.0)));
    }
}
