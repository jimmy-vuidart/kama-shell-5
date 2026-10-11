//! Input events: shortcuts, focus on click, title bar buttons.

use coquille_model::{DropTarget, Insets, Placement, Point as ModelPoint, Rect, WindowId};
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    InputTime, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    PointerMotionEvent,
};
use smithay::desktop::WindowSurfaceType;
use smithay::input::keyboard::{FilterResult, Keycode};
use smithay::input::pointer::{AxisFrame, ButtonEvent, CursorImageStatus, MotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use tracing::{debug, info, warn};

use crate::debug::Step;
use crate::decoration::layout::{TitlebarButton, TitlebarLayout};
use crate::outputs::constrain;
use crate::render::scene::Shape;
use crate::shell::ManagedWindow;
use crate::shortcuts::{Action, KeyPress, action_for};
use crate::state::State;
use crate::theme::Rgba;

/// Linux input code of the left mouse button.
const BTN_LEFT: u32 = 0x110;
/// Gap between a drop zone's outline and the slot it stands for.
const ZONE_INSET: i32 = 4;

/// How far the pointer must travel, button held, before a press on a title
/// bar becomes a drag.
const DRAG_THRESHOLD: f64 = 8.0;

/// What lies under a point of the global space.
#[derive(Debug, Default)]
struct Hit {
    window: Option<WindowId>,
    /// The client surface there, with its position.
    surface: Option<(WlSurface, Point<f64, Logical>)>,
    /// The point is on the window's title bar.
    titlebar: bool,
    button: Option<TitlebarButton>,
}

/// What the left mouse button is doing since it went down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    /// Down on a title bar, not moved yet: a click, or the start of a drag.
    Pressed {
        window: WindowId,
        at: Point<f64, Logical>,
        button: Option<TitlebarButton>,
    },
    /// Dragging a window by its title bar: drop zones are shown, and
    /// releasing on one places the window.
    Placing { window: WindowId },
    /// Dragging with the shell modifier: the window floats and follows the
    /// pointer. `grab` is where the pointer holds it, from its corner.
    Moving {
        window: WindowId,
        grab: Point<f64, Logical>,
    },
}

impl State {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                self.key(event.key_code(), event.state(), Event::time(&event));
            }
            // A mouse: moves from where the pointer is, and stays on screens.
            InputEvent::PointerMotion { event, .. } => {
                let wanted = self.pointer_location() + event.delta();
                let screens: Vec<_> = self.shell.screens.iter().map(|s| s.rect).collect();
                let location = constrain((wanted.x, wanted.y), &screens);
                self.pointer_moved(location.into(), event.time());
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
                self.pointer_moved(location, event.time());
            }
            InputEvent::PointerButton { event, .. } => {
                self.pointer_button(event.button_code(), event.state(), event.time());
            }
            InputEvent::PointerAxis { event, .. } => self.on_axis::<I>(event),
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
            Step::Move { x, y } => self.pointer_moved((x, y).into(), time),
            Step::Click => {
                self.pointer_button(BTN_LEFT, ButtonState::Pressed, time);
                self.pointer_button(BTN_LEFT, ButtonState::Released, time);
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

        let action = keyboard.input::<Option<Action>, _>(
            self,
            keycode,
            key_state,
            serial,
            time,
            |state, modifiers, handle| {
                let keycode = handle.raw_code().raw();
                if key_state == KeyState::Released {
                    // The client never saw the press: hide the release too.
                    let before = state.suppressed_keys.len();
                    state.suppressed_keys.retain(|key| *key != keycode);
                    return if state.suppressed_keys.len() != before {
                        FilterResult::Intercept(None)
                    } else {
                        FilterResult::Forward
                    };
                }
                let press = KeyPress {
                    main: mod_key.is_held(modifiers),
                    shift: modifiers.shift,
                    ctrl: modifiers.ctrl,
                    alt: modifiers.alt,
                    keycode,
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
            Action::SwitchVt(vt) => {
                self.backend.switch_vt(vt);
                return;
            }
            // Layouts apply to the screen the pointer is on.
            Action::SetLayout(kind) => match screen {
                Some(screen) => desk.set_layout(screen.id, kind),
                None => Ok(()),
            },
            Action::CycleTab { forward } => {
                match desk.focused().and_then(|id| desk.placement(id)) {
                    Some(Placement::Slot(slot)) => desk.cycle_tab(slot, forward),
                    _ => Ok(()),
                }
            }
        };
        if let Err(error) = result {
            warn!(%error, "shortcut refused");
        }
        self.apply_arrangement();
    }

    fn pointer_moved(&mut self, location: Point<f64, Logical>, time: InputTime) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        self.advance_gesture(location);
        // While the shell drags a window, clients see no pointer at all.
        let dragging = matches!(
            self.gesture,
            Some(Gesture::Placing { .. } | Gesture::Moving { .. })
        );
        let hit = if dragging {
            Hit::default()
        } else {
            self.hit_test(location)
        };

        self.hover = hit.window.zip(hit.button);
        // Over the shell's own chrome no client sets the cursor.
        if hit.surface.is_none() && !pointer.is_grabbed() {
            self.set_cursor(CursorImageStatus::default_named());
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
        match self.gesture {
            Some(Gesture::Pressed {
                window,
                at,
                button: None,
            }) => {
                let travelled = (location - at).to_i32_round::<i32>();
                let far = f64::from(travelled.x.pow(2) + travelled.y.pow(2)).sqrt();
                if far > DRAG_THRESHOLD {
                    self.gesture = Some(Gesture::Placing { window });
                }
            }
            Some(Gesture::Moving { window, grab }) => {
                self.move_floating(window, location, grab);
            }
            _ => {}
        }
    }

    /// Makes `window` float with the pointer holding it at `grab`.
    fn move_floating(
        &mut self,
        window: WindowId,
        pointer: Point<f64, Logical>,
        grab: Point<f64, Logical>,
    ) {
        let point = model_point(pointer);
        let (Some(screen), Some(managed)) = (
            self.shell.screen_at(point).copied(),
            self.shell.by_id(window),
        ) else {
            return;
        };
        let size = managed.frame().size();
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

    /// Places a window dragged by its title bar where it was released.
    fn drop_window(&mut self, window: WindowId, location: Point<f64, Logical>) {
        let point = model_point(location);
        let zones = self.shell.drop_zones(window);
        let Some((zone, target, _)) = zones.into_iter().find(|(rect, ..)| rect.contains(point))
        else {
            return;
        };
        let result = match target {
            DropTarget::Slot(slot) => self.shell.desk.move_to_slot(window, slot),
            // On a free screen the window lands with its title bar under the
            // pointer.
            DropTarget::Float(output) => {
                let bar = self.shell.desk.metrics().titlebar_height;
                let size = self
                    .shell
                    .by_id(window)
                    .map_or_else(|| zone.size(), |managed| managed.frame().size());
                let rect = Rect::new(
                    point.x - zone.x - size.w / 2,
                    point.y - zone.y - bar / 2,
                    size.w,
                    size.h,
                );
                self.shell.desk.float_window(window, output, rect)
            }
        };
        if let Err(error) = result {
            warn!(%error, "cannot place the window there");
        }
        self.apply_arrangement();
    }

    /// The hints drawn above the windows: while a window is dragged by its
    /// title bar, every place it can go, the one under the pointer standing
    /// out.
    pub fn overlay(&self) -> Vec<Shape> {
        let Some(Gesture::Placing { window }) = self.gesture else {
            return Vec::new();
        };
        let point = model_point(self.pointer_location());
        let radius = self.shell.desk.metrics().corner_radius;
        let accent = self.theme.accent;
        let tint = |alpha: u8| Rgba { a: alpha, ..accent };

        let mut shapes = Vec::new();
        for (rect, ..) in self.shell.drop_zones(window) {
            let aimed = rect.contains(point);
            let outline = rect.inset(Insets::uniform(ZONE_INSET));
            let zone = Shape {
                outline,
                area: outline,
                color: tint(if aimed { 0xff } else { 0x8c }),
                radii: [radius; 4],
                border: 2,
            };
            // Topmost first: the outline, then its fill.
            shapes.push(zone);
            shapes.push(Shape {
                color: tint(if aimed { 0x4d } else { 0x1a }),
                border: 0,
                ..zone
            });
        }
        shapes
    }

    fn pointer_button(&mut self, button: u32, state: ButtonState, time: InputTime) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        let location = pointer.current_location();
        let mut for_clients = true;

        match state {
            // During a grab (a popup menu, a drag) clicks belong to the grab.
            ButtonState::Pressed if !pointer.is_grabbed() => {
                let hit = self.hit_test(location);
                if let Some(window) = hit.window {
                    self.focus_window(window);
                }
                if button == BTN_LEFT {
                    for_clients = !self.start_gesture(&hit, location);
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
                    Some(Gesture::Placing { window }) => self.drop_window(window, location),
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
    }

    /// Decides what a left press starts. Returns whether the shell keeps the
    /// press for itself.
    fn start_gesture(&mut self, hit: &Hit, location: Point<f64, Logical>) -> bool {
        let Some(managed) = hit.window.and_then(|window| self.shell.by_id(window)) else {
            return false;
        };
        let window = managed.id;
        let frame = managed.frame();
        let anchored = managed.geometry.anchored;
        let mod_held = self
            .seat
            .get_keyboard()
            .is_some_and(|keyboard| self.mod_key.is_held(&keyboard.modifier_state()));

        if mod_held && !anchored {
            // Anywhere on the window: it will follow the pointer.
            let corner = Point::<f64, Logical>::from((f64::from(frame.x), f64::from(frame.y)));
            self.gesture = Some(Gesture::Moving {
                window,
                grab: location - corner,
            });
            return true;
        }
        if hit.titlebar {
            self.gesture = Some(Gesture::Pressed {
                window,
                at: location,
                button: hit.button,
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

    fn click_titlebar_button(&mut self, window: WindowId, button: TitlebarButton) {
        match button {
            TitlebarButton::Close => {
                let toplevel = self
                    .shell
                    .by_id(window)
                    .and_then(|managed| managed.window.toplevel());
                if let Some(toplevel) = toplevel {
                    toplevel.send_close();
                }
            }
            TitlebarButton::Swap => {
                if let Err(error) = self.shell.desk.swap_with_center(window) {
                    warn!(%error, "cannot swap with the center");
                }
                self.apply_arrangement();
            }
            // Wired in the next steps (tabs, bar).
            TitlebarButton::AddTab | TitlebarButton::Minimize => {
                debug!(?button, "this button does nothing yet");
            }
        }
    }

    /// Finds what is under `location`: popups first, then windows from the
    /// top one down.
    fn hit_test(&self, location: Point<f64, Logical>) -> Hit {
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

        for managed in visible() {
            if let Some(surface) = surface_at(managed, WindowSurfaceType::POPUP) {
                return Hit {
                    window: Some(managed.id),
                    surface: Some(surface),
                    ..Hit::default()
                };
            }
        }

        let point = model_point(location);
        for managed in visible() {
            if let Some(bar) = managed.titlebar().filter(|bar| bar.contains(point)) {
                let layout = TitlebarLayout::new(bar.size(), managed.show_swap);
                let local = ModelPoint::new(point.x - bar.x, point.y - bar.y);
                return Hit {
                    window: Some(managed.id),
                    surface: None,
                    titlebar: true,
                    button: layout.button_at(local),
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
        Hit::default()
    }
}

/// The pixel a position falls in.
fn model_point(location: Point<f64, Logical>) -> ModelPoint {
    ModelPoint::new(location.x.floor() as i32, location.y.floor() as i32)
}
