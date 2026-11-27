//! Reading the configuration file again while the compositor runs, when the
//! settings screen asks for it.
//!
//! What takes effect at once: theme, accent, terminal, keyboard layout,
//! the role, layout and scale of each screen, sticky edges, and what game
//! mode may do on the hardware. XWayland (the scale of X11 applications
//! included) is decided when the compositor starts.

use coquille_model::{OutputId, OutputRole, Point as ModelPoint, Rect, Size};
use smithay::input::keyboard::XkbConfig;
use smithay::output::Scale as OutputScale;
use smithay::utils::{Logical, Point};
use tracing::{info, warn};

use crate::backend::Backend;
use crate::outputs::{Connected, RoleConfig, assign_roles, origins};
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
        self.pace.set_enabled(config.animations.enabled());
        self.set_outputs(&config.outputs, &config.scale);
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

    /// Reassigns each lit screen's role on the hardware backend when
    /// `[outputs]` changed, moving outputs, windows, panels and XWayland's
    /// primary output at once.
    fn set_outputs(&mut self, roles: &RoleConfig, scales: &config::ScaleConfig) {
        if matches!(self.backend, Backend::Winit(_)) {
            return;
        }
        let lit: Vec<(OutputId, String, Size)> = self
            .outputs
            .iter()
            .filter_map(|screen| {
                let mode = screen.output.current_mode()?;
                Some((
                    screen.id,
                    screen.output.name(),
                    Size::new(mode.size.w, mode.size.h),
                ))
            })
            .collect();
        let connected: Vec<Connected> = lit
            .iter()
            .map(|(_, name, size)| Connected {
                name: name.clone(),
                size: *size,
            })
            .collect();
        let assigned = assign_roles(&connected, roles);

        let mut new_roles: Vec<(OutputId, OutputRole, Size)> = Vec::with_capacity(lit.len());
        let mut changed = false;
        for ((id, name, pixel_size), role) in lit.iter().zip(assigned) {
            let current = self.shell.screen(*id);
            let Some(role) = role.or_else(|| current.map(|s| s.role)) else {
                continue;
            };
            if current.is_some_and(|s| s.role != role) {
                info!(name, ?role, "screen role changed");
                changed = true;
            }
            new_roles.push((*id, role, *pixel_size));
        }
        if !changed {
            return;
        }

        let old_origins: Vec<(OutputId, ModelPoint)> = self
            .shell
            .screens
            .iter()
            .map(|s| (s.id, s.rect.origin()))
            .collect();
        let pointer_before = self.pointer_screen().map(|screen| {
            let loc = self.pointer_location();
            (
                screen.id,
                Point::<f64, Logical>::from((
                    loc.x - f64::from(screen.rect.x),
                    loc.y - f64::from(screen.rect.y),
                )),
            )
        });

        let placed: Vec<(OutputRole, Size)> = new_roles
            .iter()
            .map(|(id, role, pixel_size)| {
                let scale = if self.scale_pinned {
                    self.shell.scale_of(*id)
                } else {
                    scales.of(*role)
                };
                (*role, scale.logical(*pixel_size))
            })
            .collect();
        let corners = origins(&placed);
        let screens: Vec<ScreenInfo> = new_roles
            .iter()
            .zip(corners)
            .map(|((id, role, pixel_size), corner)| {
                let scale = if self.scale_pinned {
                    self.shell.scale_of(*id)
                } else {
                    scales.of(*role)
                };
                let size = scale.logical(*pixel_size);
                if let Some(output) = self.output(*id) {
                    let output_scale = Some(OutputScale::Fractional(scale.get()));
                    output.change_current_state(
                        None,
                        None,
                        output_scale,
                        Some((corner.x, corner.y).into()),
                    );
                }
                ScreenInfo {
                    id: *id,
                    role: *role,
                    rect: Rect::new(corner.x, corner.y, size.w, size.h),
                    scale,
                }
            })
            .collect();

        self.save_proportions();
        let proportions = self.saved_proportions.clone();
        let role_pairs: Vec<(OutputId, OutputRole)> =
            screens.iter().map(|s| (s.id, s.role)).collect();
        if let Err(error) = self.shell.desk.set_output_roles(&role_pairs, &proportions) {
            warn!(%error, "cannot reassign screen roles");
            return;
        }
        let ids: Vec<OutputId> = screens.iter().map(|s| s.id).collect();
        let moved_outputs: Vec<OutputId> = screens
            .iter()
            .filter(|s| {
                old_origins
                    .iter()
                    .find(|(id, _)| *id == s.id)
                    .is_some_and(|(_, old)| *old != s.rect.origin())
            })
            .map(|s| s.id)
            .collect();
        self.shell.move_screens(screens);
        for id in ids {
            self.arrange_panels(id);
        }
        self.apply_arrangement();
        // A screen that slid along the global desktop kept its windows on the
        // same physical monitor: do not animate them flying across bezels.
        for managed in &mut self.shell.windows {
            if moved_outputs.contains(&managed.geometry.output) {
                managed.motion = Default::default();
            }
        }
        self.update_xwayland_primary();
        if let Some((id, local)) = pointer_before {
            if let Some(screen) = self.shell.screen(id) {
                let target = Point::<f64, Logical>::from((
                    f64::from(screen.rect.x) + local.x,
                    f64::from(screen.rect.y) + local.y,
                ));
                self.relocate_pointer(target);
            }
        }
        self.after_rescale();
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
        // A new layout starts with every lock off: Num Lock and Caps Lock
        // stay as they were.
        let before = keyboard.modifier_state();
        match keyboard.set_xkb_config(self, config) {
            Ok(()) => {
                let mut locks = keyboard.modifier_state();
                locks.num_lock = before.num_lock;
                locks.caps_lock = before.caps_lock;
                keyboard.set_modifier_state(locks);
                keyboard.advertise_modifier_state(self);
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
