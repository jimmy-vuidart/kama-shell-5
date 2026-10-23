//! Keyboard shortcuts of the compositor.

use std::path::Path;

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
    /// Opens the settings screen of the shell.
    OpenSettings,
    /// Brings back the window minimized last.
    RestoreWindow,
    /// Puts the fullscreen window (the game) back in a window.
    LeaveFullscreen,
    /// Switches to another virtual terminal: the way out if all else fails.
    SwitchVt(i32),
    /// A brightness key of a laptop: the screen gets brighter or dimmer.
    Brightness {
        up: bool,
    },
    /// Starts interactive region screenshot selection.
    ScreenshotRegion,
    /// Alt+Tab: the window switcher, to the next window used, or with
    /// Shift to the previous one.
    SwitchWindow {
        forward: bool,
    },
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
    // Keys of the hardware: they work without the shell modifier, in a
    // game too.
    if has(Keysym::XF86_MonBrightnessUp) {
        return Some(Action::Brightness { up: true });
    }
    if has(Keysym::XF86_MonBrightnessDown) {
        return Some(Action::Brightness { up: false });
    }
    if has(Keysym::Print) || has(Keysym::Sys_Req) {
        return Some(Action::ScreenshotRegion);
    }
    // Alt itself, whatever the shell modifier: the habit of every desktop.
    if press.alt && !press.ctrl && has(Keysym::Tab) {
        return Some(Action::SwitchWindow {
            forward: !press.shift,
        });
    }

    if !press.main {
        return None;
    }
    if press.shift && has(Keysym::e) {
        Some(Action::Quit)
    } else if has(Keysym::Return) {
        Some(Action::SpawnTerminal)
    } else if has(Keysym::s) {
        Some(Action::OpenSettings)
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

/// The command of the settings screen: the program installed next to the
/// compositor (`beside`), else the one of the `PATH`, on the configuration
/// file the compositor reads, if it was given one.
pub fn settings_command(beside: Option<&Path>, config: Option<&Path>) -> String {
    const PROGRAM: &str = "coquille-settings";
    let program = beside
        .map(|dir| dir.join(PROGRAM))
        .filter(|program| program.is_file())
        .map_or_else(|| PROGRAM.to_owned(), |path| path.display().to_string());
    let mut command = quoted(&program);
    if let Some(config) = config {
        command.push_str(" --config ");
        command.push_str(&quoted(&config.display().to_string()));
    }
    command
}

/// A text as one word of a shell command.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
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
    fn the_brightness_keys_work_without_the_shell_modifier() {
        let up = press(false, false, &[Keysym::XF86_MonBrightnessUp]);
        let down = press(true, false, &[Keysym::XF86_MonBrightnessDown]);

        assert_eq!(up, Some(Action::Brightness { up: true }));
        assert_eq!(down, Some(Action::Brightness { up: false }));
    }

    #[test]
    fn mod_s_opens_the_settings() {
        assert_eq!(press(true, false, &[Keysym::s]), Some(Action::OpenSettings));
        assert_eq!(press(false, false, &[Keysym::s]), None);
    }

    #[test]
    fn the_settings_open_on_the_file_of_the_compositor() {
        let nowhere = Path::new("/nonexistent/coquille");

        assert_eq!(settings_command(Some(nowhere), None), "'coquille-settings'");
        assert_eq!(
            settings_command(None, Some(Path::new("/tmp/l'essai/config.toml"))),
            "'coquille-settings' --config '/tmp/l'\\''essai/config.toml'"
        );
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

    #[test]
    fn alt_tab_switches_windows_and_shift_goes_back() {
        let alt = |shift| KeyPress {
            alt: true,
            shift,
            syms: &[Keysym::Tab],
            ..PLAIN
        };

        assert_eq!(
            action_for(alt(false)),
            Some(Action::SwitchWindow { forward: true })
        );
        assert_eq!(
            action_for(alt(true)),
            Some(Action::SwitchWindow { forward: false })
        );
        // Nested, Alt is also the shell modifier.
        let nested = KeyPress {
            main: true,
            ..alt(false)
        };
        assert_eq!(
            action_for(nested),
            Some(Action::SwitchWindow { forward: true })
        );
        assert_eq!(press(false, false, &[Keysym::Tab]), None);
        assert_eq!(press(true, false, &[Keysym::Tab]), None);
    }

    #[test]
    fn print_screen_triggers_region_screenshot() {
        assert_eq!(
            press(false, false, &[Keysym::Print]),
            Some(Action::ScreenshotRegion)
        );
        assert_eq!(
            press(true, false, &[Keysym::Print]),
            Some(Action::ScreenshotRegion)
        );
        assert_eq!(
            press(false, false, &[Keysym::Sys_Req]),
            Some(Action::ScreenshotRegion)
        );
    }
}
