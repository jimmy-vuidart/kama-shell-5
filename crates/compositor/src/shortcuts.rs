//! Keyboard shortcuts of the compositor.

use coquille_model::LayoutKind;
use smithay::input::keyboard::{Keysym, ModifiersState};

/// The modifier that marks a shortcut as meant for the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKey {
    /// On real hardware.
    #[expect(dead_code, reason = "selected by the udev backend, in phase 2")]
    Super,
    /// Nested in another session, which keeps `Super` for itself.
    Alt,
}

impl ModKey {
    pub fn is_held(self, modifiers: &ModifiersState) -> bool {
        match self {
            Self::Super => modifiers.logo,
            Self::Alt => modifiers.alt,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    SetLayout(LayoutKind),
    CycleTab { forward: bool },
}

/// A key going down.
#[derive(Debug, Clone, Copy)]
pub struct KeyPress<'a> {
    /// The shell modifier is held.
    pub main: bool,
    pub shift: bool,
    /// XKB keycode (evdev code + 8): names a physical key, whatever the layout.
    pub keycode: u32,
    /// Keysyms of the key without modifiers.
    pub syms: &'a [Keysym],
}

/// Digit-row keys are matched by position, because layouts such as AZERTY do
/// not produce digits without Shift.
const KEY_1: u32 = 10;
const KEY_2: u32 = 11;

pub fn action_for(press: KeyPress<'_>) -> Option<Action> {
    if !press.main {
        return None;
    }
    let has = |sym: Keysym| press.syms.contains(&sym);
    match press.keycode {
        KEY_1 => Some(Action::SetLayout(LayoutKind::Full)),
        KEY_2 => Some(Action::SetLayout(LayoutKind::Columns)),
        _ if press.shift && has(Keysym::e) => Some(Action::Quit),
        _ if has(Keysym::Next) => Some(Action::CycleTab { forward: true }),
        _ if has(Keysym::Prior) => Some(Action::CycleTab { forward: false }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(main: bool, shift: bool, keycode: u32, syms: &[Keysym]) -> Option<Action> {
        action_for(KeyPress {
            main,
            shift,
            keycode,
            syms,
        })
    }

    #[test]
    fn the_first_digit_key_selects_full() {
        assert_eq!(
            press(true, false, KEY_1, &[Keysym::ampersand]),
            Some(Action::SetLayout(LayoutKind::Full))
        );
    }

    #[test]
    fn the_second_digit_key_selects_columns() {
        assert_eq!(
            press(true, false, KEY_2, &[Keysym::eacute]),
            Some(Action::SetLayout(LayoutKind::Columns))
        );
    }

    #[test]
    fn nothing_happens_without_the_shell_modifier() {
        assert_eq!(press(false, false, KEY_1, &[Keysym::_1]), None);
        assert_eq!(press(false, true, 26, &[Keysym::e]), None);
        assert_eq!(press(false, false, 117, &[Keysym::Next]), None);
    }

    #[test]
    fn mod_shift_e_quits() {
        assert_eq!(press(true, true, 26, &[Keysym::e]), Some(Action::Quit));
    }

    #[test]
    fn mod_e_without_shift_does_nothing() {
        assert_eq!(press(true, false, 26, &[Keysym::e]), None);
    }

    #[test]
    fn page_keys_cycle_tabs() {
        assert_eq!(
            press(true, false, 117, &[Keysym::Next]),
            Some(Action::CycleTab { forward: true })
        );
        assert_eq!(
            press(true, false, 112, &[Keysym::Prior]),
            Some(Action::CycleTab { forward: false })
        );
    }

    #[test]
    fn an_unbound_key_does_nothing() {
        assert_eq!(press(true, false, 38, &[Keysym::a]), None);
    }
}
