//! Reading the configuration file again while the compositor runs, when the
//! settings screen asks for it.
//!
//! What takes effect at once: theme, terminal, keyboard layout,
//! sticky edges, and what game mode may do on the hardware. The role of
//! each screen and XWayland are decided when the compositor starts.

use smithay::input::keyboard::XkbConfig;
use tracing::{info, warn};

use crate::backend::Backend;
use crate::state::State;
use crate::{config, default_terminal, keyboard_layout};

impl State {
    /// Reads the configuration again and applies what can change now. A
    /// file that is gone or broken changes nothing.
    pub fn reload_config(&mut self) {
        let Some(config) = config::reload(self.config_path.as_deref()) else {
            return;
        };
        info!("configuration read again");

        // A file that names no theme leaves the one in use.
        let kind = config.theme.unwrap_or(self.theme.kind);
        self.set_theme(kind);
        self.terminal = config.terminal.clone().unwrap_or_else(default_terminal);
        self.sticky_edges_ms = config.game.sticky_edges_ms();
        if !self.keyboard_pinned {
            let keyboard = &config.keyboard;
            self.set_keyboard_layout(keyboard.layout.as_deref(), keyboard.variant.as_deref());
        }
        if let Backend::Udev(udev) = &mut self.backend {
            udev.set_game(&config.game);
        }
        // A screen that just lost its variable refresh rate needs a frame
        // to learn it.
        self.queue_redraw();
        self.config_changed();
    }

    /// Gives the keyboard the layout the configuration names, or the one of
    /// the system. A layout XKB does not know leaves the keyboard as it is.
    fn set_keyboard_layout(&mut self, layout: Option<&str>, variant: Option<&str>) {
        let wanted = keyboard_layout::detect(layout, variant);
        if wanted == self.keyboard {
            return;
        }
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let config = XkbConfig {
            layout: &wanted.layout,
            variant: &wanted.variant,
            options: wanted.options.clone(),
            ..XkbConfig::default()
        };
        match keyboard.set_xkb_config(self, config) {
            Ok(()) => {
                info!(
                    layout = wanted.layout,
                    variant = wanted.variant,
                    "keyboard layout changed"
                );
                self.keyboard = wanted;
            }
            Err(error) => warn!(
                layout = wanted.layout,
                variant = wanted.variant,
                ?error,
                "not a keyboard layout: the keyboard stays as it was"
            ),
        }
    }
}
