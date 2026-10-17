//! Keyboard shortcuts of the compositor.

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
    /// Keysyms of the key without modifiers.
    pub syms: &'a [Keysym],
    /// Keysym of the key with the modifiers held.
    pub modified: Keysym,
}

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
    if press.shift && has(Keysym::e) {
        Some(Action::Quit)
    } else if has(Keysym::Return) {
        Some(Action::SpawnTerminal)
    } else if has(Keysym::r) {
        Some(Action::RestoreWindow)
    } else if has(Keysym::Escape) {
        Some(Action::LeaveFullscreen)
    } else if has(Keysym::Next) {
        Some(Action::CycleTab { forward: true })
    } else if has(Keysym::Prior) {
        Some(Action::CycleTab { forward: false })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(main: bool, shift: bool, syms: &[Keysym]) -> Option<Action> {
        action_for(KeyPress {
            main,
            shift,
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
        syms: &[],
        modified: Keysym::NoSymbol,
    };

    #[test]
    fn mod_r_restores_the_last_minimized_window() {
        assert_eq!(
            press(true, false, &[Keysym::r]),
            Some(Action::RestoreWindow)
        );
        assert_eq!(press(false, false, &[Keysym::r]), None);
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
            press(true, false, &[Keysym::Escape]),
            Some(Action::LeaveFullscreen)
        );
        assert_eq!(press(false, false, &[Keysym::Escape]), None);
    }

    #[test]
    fn mod_return_opens_a_terminal() {
        assert_eq!(
            press(true, false, &[Keysym::Return]),
            Some(Action::SpawnTerminal)
        );
        assert_eq!(press(false, false, &[Keysym::Return]), None);
    }

    #[test]
    fn the_vt_keys_switch_terminal_without_the_shell_modifier() {
        let third = KeyPress {
            ctrl: true,
            alt: true,
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
    fn nothing_happens_without_the_shell_modifier() {
        assert_eq!(press(false, false, &[Keysym::_1]), None);
        assert_eq!(press(false, true, &[Keysym::e]), None);
        assert_eq!(press(false, false, &[Keysym::Next]), None);
    }

    #[test]
    fn mod_shift_e_quits() {
        assert_eq!(press(true, true, &[Keysym::e]), Some(Action::Quit));
    }

    #[test]
    fn mod_e_without_shift_does_nothing() {
        assert_eq!(press(true, false, &[Keysym::e]), None);
    }

    #[test]
    fn page_keys_cycle_tabs() {
        assert_eq!(
            press(true, false, &[Keysym::Next]),
            Some(Action::CycleTab { forward: true })
        );
        assert_eq!(
            press(true, false, &[Keysym::Prior]),
            Some(Action::CycleTab { forward: false })
        );
    }

    #[test]
    fn an_unbound_key_does_nothing() {
        assert_eq!(press(true, false, &[Keysym::a]), None);
    }
}
