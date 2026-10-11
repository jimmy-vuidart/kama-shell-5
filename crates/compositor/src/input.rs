//! Input events: shortcuts, focus on click, title bar buttons.

use coquille_model::{Placement, Point as ModelPoint, WindowId};
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    InputTime, KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
};
use smithay::desktop::WindowSurfaceType;
use smithay::input::keyboard::{FilterResult, Keycode};
use smithay::input::pointer::{AxisFrame, ButtonEvent, CursorImageStatus, MotionEvent};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, SERIAL_COUNTER};
use tracing::{debug, info, warn};

use crate::debug::Step;
use crate::decoration::layout::{TitlebarButton, TitlebarLayout};
use crate::shell::ManagedWindow;
use crate::shortcuts::{Action, KeyPress, action_for};
use crate::state::State;

/// Linux input code of the left mouse button.
const BTN_LEFT: u32 = 0x110;

/// What lies under a point of the output.
#[derive(Debug, Default)]
struct Hit {
    window: Option<WindowId>,
    /// The client surface there, with its position on the output.
    surface: Option<(WlSurface, Point<f64, Logical>)>,
    button: Option<TitlebarButton>,
}

impl State {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                self.key(event.key_code(), event.state(), Event::time(&event));
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                if let Some(mode) = self.output.current_mode() {
                    let location = event.position_transformed(mode.size.to_logical(1));
                    self.pointer_moved(location, event.time());
                }
            }
            InputEvent::PointerButton { event, .. } => {
                self.pointer_button(event.button_code(), event.state(), event.time());
            }
            InputEvent::PointerAxis { event, .. } => self.on_axis::<I>(event),
            _ => {}
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
                    keycode,
                    syms: &handle.raw_syms(),
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
        let desk = &mut self.shell.desk;
        let result = match action {
            Action::Quit => {
                info!("quitting on request");
                self.loop_signal.stop();
                return;
            }
            Action::SetLayout(kind) => desk.set_layout(self.shell.output, kind),
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
        let hit = self.hit_test(location);

        let hover = hit.window.zip(hit.button);
        if hover != self.hover {
            self.hover = hover;
            self.queue_redraw();
        }
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
        if matches!(self.cursor_status, CursorImageStatus::Surface(_)) {
            self.queue_redraw();
        }
    }

    fn pointer_button(&mut self, button: u32, state: ButtonState, time: InputTime) {
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();

        let hit = self.hit_test(pointer.current_location());
        let on_button = hit.window.zip(hit.button);
        match state {
            // During a grab (a popup menu, a drag) clicks belong to the grab.
            ButtonState::Pressed if !pointer.is_grabbed() => {
                if let Some(window) = hit.window {
                    self.focus_window(window);
                }
                if button == BTN_LEFT {
                    self.pressed = on_button;
                }
            }
            ButtonState::Released if button == BTN_LEFT => {
                // A click counts when it ends on the button it began on.
                if let Some(pressed) = self.pressed.take().filter(|p| Some(*p) == on_button) {
                    self.click_titlebar_button(pressed.0, pressed.1);
                }
            }
            _ => {}
        }

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
        let desk = &mut self.shell.desk;
        desk.note_output_used(self.shell.output);
        if desk.focused() == Some(window) {
            return;
        }
        if let Err(error) = desk.focus(window) {
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
            // Wired in the next phases (tabs, swap with the center, bar).
            TitlebarButton::AddTab | TitlebarButton::Swap | TitlebarButton::Minimize => {
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
        // Where the window's surface tree starts on the output.
        let origin = |managed: &ManagedWindow| {
            let client = managed.geometry.client;
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
                    button: None,
                };
            }
        }

        let point = ModelPoint::new(location.x.floor() as i32, location.y.floor() as i32);
        for managed in visible() {
            let geometry = managed.geometry;
            if let Some(bar) = geometry.titlebar.filter(|bar| bar.contains(point)) {
                let layout = TitlebarLayout::new(bar.size(), self.shell.role.is_side());
                let local = ModelPoint::new(point.x - bar.x, point.y - bar.y);
                return Hit {
                    window: Some(managed.id),
                    surface: None,
                    button: layout.button_at(local),
                };
            }
            if geometry.client.contains(point) {
                let kind = WindowSurfaceType::TOPLEVEL | WindowSurfaceType::SUBSURFACE;
                return Hit {
                    window: Some(managed.id),
                    surface: surface_at(managed, kind),
                    button: None,
                };
            }
        }
        Hit::default()
    }
}
