//! Reading the configuration file again while the compositor runs, when the
//! settings screen asks for it.
//!
//! What takes effect at once: theme, accent, terminal, keyboard layout,
//! the layout and the scale of each screen, sticky edges, and what game
//! mode may do on the hardware. The role of each screen and XWayland (the
//! scale of X11 applications included) are decided when the compositor
//! starts.

use coquille_model::{OutputId, Size};
use smithay::input::keyboard::XkbConfig;
use smithay::output::Scale as OutputScale;
use tracing::{info, warn};

use crate::backend::Backend;
use crate::outputs::origins;
use crate::shell::ScreenInfo;
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
        self.restyle(kind, config.accent());
        self.terminal = config.terminal.clone().unwrap_or_else(default_terminal);
        self.sticky_edges_ms = config.game.sticky_edges_ms();
        if !self.keyboard_pinned {
            let keyboard = &config.keyboard;
            self.set_keyboard_layout(keyboard.layout.as_deref(), keyboard.variant.as_deref());
        }
        if let Backend::Udev(udev) = &mut self.backend {
            udev.set_game(&config.game);
            udev.set_pointers(&config.mouse, &config.touchpad);
        }
        self.set_layouts(&config.layout);
        if !self.scale_pinned {
            self.set_scales(&config.scale);
        }
        self.glass = config.glass;
        self.wallpaper_config = config.wallpaper.clone();
        self.refresh_wallpapers();
        self.full_redraw = true;
        // A screen that just lost its variable refresh rate needs a frame
        // to learn it.
        self.queue_redraw();
        self.config_changed();
    }

    /// Gives each screen the layout the configuration names; windows move
    /// only where a screen loses columns.
    fn set_layouts(&mut self, layout: &config::LayoutConfig) {
        let screens: Vec<_> = self
            .shell
            .desk
            .screens()
            .iter()
            .map(|screen| (screen.id(), screen.role(), screen.layout()))
            .collect();
        let mut changed = false;
        for (output, role, current) in screens {
            let wanted = layout.of(role);
            if wanted != current && self.shell.desk.set_layout(output, wanted).is_ok() {
                info!(?role, layout = wanted.name(), "layout changed");
                changed = true;
            }
        }
        if changed {
            self.apply_arrangement();
        }
    }

    /// Gives each screen the scale the configuration names: screens keep
    /// their pixels and their order, and take as much logical room as they
    /// now have; windows, panels and clients follow.
    pub fn set_scales(&mut self, scales: &config::ScaleConfig) {
        let wanted = |screen: &ScreenInfo| scales.of(screen.role);
        if let Backend::Winit(winit) = &mut self.backend {
            let scale = winit.simulated.scale(scales);
            if scale == winit.scale {
                return;
            }
            info!(scale = scale.get(), "scale changed");
            winit.scale = scale;
            let size = winit.backend.window_size();
            self.resize_window(size);
            self.after_rescale();
            return;
        }
        if self
            .shell
            .screens
            .iter()
            .all(|screen| wanted(screen) == screen.scale)
        {
            return;
        }
        let pixels: Vec<(OutputId, Option<Size>)> = self
            .shell
            .screens
            .iter()
            .map(|screen| {
                let mode = self
                    .output(screen.id)
                    .and_then(|output| output.current_mode());
                let size = mode.map(|mode| Size::new(mode.size.w, mode.size.h));
                (screen.id, size)
            })
            .collect();
        let mut screens = self.shell.screens.clone();
        for (screen, (_, pixels)) in screens.iter_mut().zip(&pixels) {
            let Some(pixels) = pixels else {
                continue;
            };
            let scale = wanted(screen);
            if scale != screen.scale {
                info!(role = ?screen.role, scale = scale.get(), "scale changed");
            }
            screen.scale = scale;
            let size = scale.logical(*pixels);
            screen.rect.w = size.w;
            screen.rect.h = size.h;
        }
        let placed: Vec<_> = screens
            .iter()
            .map(|screen| (screen.role, screen.rect.size()))
            .collect();
        for (screen, corner) in screens.iter_mut().zip(origins(&placed)) {
            screen.rect.x = corner.x;
            screen.rect.y = corner.y;
            if let Some(output) = self.output(screen.id) {
                let scale = Some(OutputScale::Fractional(screen.scale.get()));
                output.change_current_state(None, None, scale, Some((corner.x, corner.y).into()));
            }
        }
        let ids: Vec<OutputId> = screens.iter().map(|screen| screen.id).collect();
        self.shell.move_screens(screens);
        for id in ids {
            self.arrange_panels(id);
        }
        self.apply_arrangement();
        self.after_rescale();
    }

    /// What follows a change of scale, on either backend.
    fn after_rescale(&mut self) {
        let largest = self.shell.screens.iter().map(|screen| screen.scale).max();
        if self.xdisplay.is_some() && largest != Some(self.x11_scale) {
            info!(
                x11_scale = self.x11_scale.get(),
                "X11 applications keep their scale until the next start"
            );
        }
        self.refresh_wallpapers();
        self.shell_changed();
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
