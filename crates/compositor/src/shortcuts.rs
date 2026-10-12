//! Keyboard shortcuts of the compositor.

use coquille_model::LayoutKind;
use smithay::input::keyboard::{Keysym, ModifiersState};

/// The modifier that marks a shortcut as meant for the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKey {
    /// On real hardware.
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

    /// `syms` are those of the modifier key itself, left or right.
    pub fn is_key(self, syms: &[Keysym]) -> bool {
        let keys = match self {
            Self::Super => [Keysym::Super_L, Keysym::Super_R],
            Self::Alt => [Keysym::Alt_L, Keysym::Alt_R],
        };
        syms.iter().any(|sym| keys.contains(sym))
    }

    /// No other modifier is held: the shell modifier alone can be a tap.
    pub fn is_alone(self, modifiers: &ModifiersState) -> bool {
        let others = match self {
            Self::Super => modifiers.alt,
            Self::Alt => modifiers.logo,
        };
        !(others || modifiers.ctrl || modifiers.shift)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    SetLayout(LayoutKind),
    CycleTab {
        forward: bool,
    },
    /// Opens a terminal: the way to start anything until the launcher exists.
    SpawnTerminal,
    /// Brings back the window minimized last.
    RestoreWindow,
    /// Puts the fullscreen window (the game) back in a window.
    LeaveFullscreen,
    /// Switches to another virtual terminal: the way out if all else fails.
    SwitchVt(i32),
}

/// A key going down.
#[derive(Debug, Clone, Copy)]
pub struct KeyPress<'a> {
    /// The shell modifier is held.
    pub main: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// XKB keycode (evdev code + 8): names a physical key, whatever the layout.
    pub keycode: u32,
    /// Keysyms of the key without modifiers.
    pub syms: &'a [Keysym],
    /// Keysym of the key with the modifiers held.
    pub modified: Keysym,
}

/// Digit-row keys are matched by position, because layouts such as AZERTY do
/// not produce digits without Shift.
const KEY_1: u32 = 10;
const KEY_2: u32 = 11;
const KEY_3: u32 = 12;
const KEY_4: u32 = 13;
const KEY_5: u32 = 14;

pub fn action_for(press: KeyPress<'_>) -> Option<Action> {
    let has = |sym: Keysym| press.syms.contains(&sym);

    // Ways out that must work even if the shell modifier does not.
    let first_vt = Keysym::XF86_Switch_VT_1.raw();
    let last_vt = Keysym::XF86_Switch_VT_12.raw();
    if (first_vt..=last_vt).contains(&press.modified.raw()) {
        // At most 12, so the cast cannot truncate.
        return Some(Action::SwitchVt(
            (press.modified.raw() - first_vt + 1) as i32,
        ));
    }
    if press.ctrl && press.alt && has(Keysym::BackSpace) {
        return Some(Action::Quit);
    }

    if !press.main {
        return None;
    }
    match press.keycode {
        KEY_1 => Some(Action::SetLayout(LayoutKind::Full)),
        KEY_2 => Some(Action::SetLayout(LayoutKind::Columns)),
        KEY_3 => Some(Action::SetLayout(LayoutKind::Rows)),
        KEY_4 => Some(Action::SetLayout(LayoutKind::Free)),
        KEY_5 => Some(Action::SetLayout(LayoutKind::TwoThirdsOneThird)),
        _ if press.shift && has(Keysym::e) => Some(Action::Quit),
        _ if has(Keysym::Return) => Some(Action::SpawnTerminal),
        _ if has(Keysym::r) => Some(Action::RestoreWindow),
        _ if has(Keysym::Escape) => Some(Action::LeaveFullscreen),
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
            ..PLAIN
        })
    }

    /// A key with no modifier; tests override what matters to them.
    const PLAIN: KeyPress<'static> = KeyPress {
        main: false,
        shift: false,
        ctrl: false,
        alt: false,
        keycode: 0,
        syms: &[],
        modified: Keysym::NoSymbol,
    };

    #[test]
    fn mod_r_restores_the_last_minimized_window() {
        assert_eq!(
            press(true, false, 27, &[Keysym::r]),
            Some(Action::RestoreWindow)
        );
        assert_eq!(press(false, false, 27, &[Keysym::r]), None);
    }

    #[test]
    fn the_shell_modifier_key_is_recognised_on_both_sides() {
        assert!(ModKey::Super.is_key(&[Keysym::Super_L]));
        assert!(ModKey::Super.is_key(&[Keysym::Super_R]));
        assert!(!ModKey::Super.is_key(&[Keysym::Alt_L]));
        assert!(ModKey::Alt.is_key(&[Keysym::Alt_L]));
        assert!(!ModKey::Alt.is_key(&[Keysym::Super_L]));
        assert!(!ModKey::Alt.is_key(&[]));
    }

    #[test]
    fn the_shell_modifier_is_alone_without_another_modifier() {
        let held = |ctrl, shift, alt, logo| ModifiersState {
            ctrl,
            shift,
            alt,
            logo,
            ..ModifiersState::default()
        };

        assert!(ModKey::Super.is_alone(&held(false, false, false, true)));
        assert!(!ModKey::Super.is_alone(&held(true, false, false, true)));
        assert!(!ModKey::Super.is_alone(&held(false, true, false, true)));
        assert!(!ModKey::Super.is_alone(&held(false, false, true, true)));
        assert!(ModKey::Alt.is_alone(&held(false, false, true, false)));
        assert!(!ModKey::Alt.is_alone(&held(false, false, true, true)));
    }

    #[test]
    fn mod_escape_puts_the_game_back_in_a_window() {
        assert_eq!(
            press(true, false, 9, &[Keysym::Escape]),
            Some(Action::LeaveFullscreen)
        );
        assert_eq!(press(false, false, 9, &[Keysym::Escape]), None);
    }

    #[test]
    fn mod_return_opens_a_terminal() {
        assert_eq!(
            press(true, false, 36, &[Keysym::Return]),
            Some(Action::SpawnTerminal)
        );
        assert_eq!(press(false, false, 36, &[Keysym::Return]), None);
    }

    #[test]
    fn the_vt_keys_switch_terminal_without_the_shell_modifier() {
        let third = KeyPress {
            ctrl: true,
            alt: true,
            keycode: 69,
            syms: &[Keysym::F3],
            modified: Keysym::XF86_Switch_VT_3,
            ..PLAIN
        };
        let twelfth = KeyPress {
            modified: Keysym::XF86_Switch_VT_12,
            ..third
        };

        assert_eq!(action_for(third), Some(Action::SwitchVt(3)));
        assert_eq!(action_for(twelfth), Some(Action::SwitchVt(12)));
    }

    #[test]
    fn ctrl_alt_backspace_quits_whatever_the_shell_modifier() {
        let emergency = KeyPress {
            ctrl: true,
            alt: true,
            keycode: 22,
            syms: &[Keysym::BackSpace],
            ..PLAIN
        };

        assert_eq!(action_for(emergency), Some(Action::Quit));
        assert_eq!(
            action_for(KeyPress {
                alt: false,
                ..emergency
            }),
            None
        );
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
    fn the_other_digit_keys_select_the_other_layouts() {
        let layout = |keycode| press(true, false, keycode, &[]);

        assert_eq!(layout(12), Some(Action::SetLayout(LayoutKind::Rows)));
        assert_eq!(layout(13), Some(Action::SetLayout(LayoutKind::Free)));
        assert_eq!(
            layout(14),
            Some(Action::SetLayout(LayoutKind::TwoThirdsOneThird))
        );
        assert_eq!(layout(15), None);
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
