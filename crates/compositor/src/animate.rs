//! Which change of the shell animates, and how: the glue between the model's
//! decisions and the motions of `animation`.
//!
//! A window that the model moves goes there instead of jumping; one that
//! opens grows in, one that closes fades out, one minimized goes into the
//! dock and comes back from it. What the user drags follows the pointer and
//! never lags behind it, and nothing moves on the screen of a game.

use std::time::Instant;

use coquille_model::{OutputId, Point as ModelPoint, Rect, WindowGeometry, WindowId};
use smithay::desktop::{LayerSurface, layer_map_for_output};
use smithay::utils::IsAlive;

use crate::animation::{Area, Arrival, Curve, DOCK_SCALE, Entrance, Exit, Look, Side, durations};
use crate::input::Gesture;
use crate::render::scene::PanelGhost;
use crate::state::State;

/// The namespace of the bar of the dock.
const BAR: &str = "coquille-dock";

impl State {
    /// The model placed the windows anew: each one that changed starts its
    /// motion from where it showed, `before`.
    pub fn animate_changes(&mut self, before: &[(WindowId, WindowGeometry, Rect)]) {
        let now = Instant::now();
        let pace = self.pace;
        // What the user holds follows the pointer, not a curve.
        let held = match self.gesture {
            Some(Gesture::Moving { window, .. } | Gesture::Resizing { window, .. }) => Some(window),
            _ => None,
        };
        let splitting = match self.gesture {
            Some(Gesture::Splitting { output, .. }) => Some(output),
            _ => None,
        };
        // Screens where a window grows over the others: those it covers stay
        // drawn under it until it is there.
        let covering: Vec<OutputId> = self
            .shell
            .windows
            .iter()
            .filter(|managed| {
                let new = managed.geometry;
                let old = before.iter().find(|(id, ..)| *id == managed.id);
                new.visible && new.maximized && old.is_some_and(|(_, old, _)| !old.maximized)
            })
            .map(|managed| managed.geometry.output)
            .collect();
        let docks = self.dock_bars();

        let shell = &mut self.shell;
        let screens = &shell.screens;
        for managed in &mut shell.windows {
            let Some((_, old, old_frame)) = before.iter().find(|(id, ..)| *id == managed.id) else {
                continue;
            };
            let new = managed.geometry;
            let frame = managed.frame();
            let motion = &mut managed.motion;
            if old.anchored || new.anchored {
                *motion = Default::default();
                continue;
            }
            if old.focused != new.focused {
                let tween = pace.tween(now, durations::RING, Curve::EaseOut);
                motion.ring(new.focused, tween);
            }
            let screen = screens.iter().find(|screen| screen.id == new.output);
            let dock = || {
                let screen = screen.map_or(frame, |screen| screen.rect);
                let bar = docks.iter().find(|(id, _)| *id == new.output);
                dock_area(frame, bar.map(|(_, bar)| *bar), screen)
            };
            if old.visible && !old.minimized && new.minimized {
                let tween = pace.tween(now, durations::MINIMIZE, Curve::EaseOut);
                motion.minimize(dock(), tween);
            } else if old.minimized && !new.minimized && new.visible {
                let tween = pace.tween(now, durations::RESTORE, Curve::Settle);
                motion.enter(Entrance::Restore { from: dock() }, tween);
            } else if old.visible && new.visible && *old_frame != frame {
                // A floating window that stays so moves at the pointer or at
                // its client's will: it follows at once.
                let steady = (old.floating && new.floating)
                    || held == Some(managed.id)
                    || splitting == Some(new.output);
                let tween = (!steady)
                    .then(|| pace.tween(now, durations::MOVE, Curve::Settle))
                    .flatten();
                motion.move_from(*old_frame, now, tween);
            } else if old.visible && !new.visible && covering.contains(&new.output) {
                motion.linger(pace.tween(now, durations::MOVE, Curve::Linear));
            }
        }
    }

    /// A window drew for the first time: it grows in, unless it opens
    /// hidden (behind a game) or as a game.
    pub fn window_drawn(&mut self, id: WindowId) {
        let tween = self
            .pace
            .tween(Instant::now(), durations::OPEN, Curve::EaseOut);
        if let Some(managed) = self.shell.windows.iter_mut().find(|m| m.id == id) {
            let geometry = managed.geometry;
            if geometry.visible && !geometry.anchored {
                managed.motion.enter(Entrance::Open, tween);
                self.queue_redraw();
            }
        }
    }

    /// The bar of the dock on each screen that has one, in the global space.
    fn dock_bars(&self) -> Vec<(OutputId, Rect)> {
        self.outputs
            .iter()
            .filter_map(|screen| {
                let info = self.shell.screen(screen.id)?;
                let map = layer_map_for_output(&screen.output);
                let bar = map.layers().find(|layer| layer.namespace() == BAR)?;
                let place = map.layer_geometry(bar)?;
                let rect = Rect::new(place.loc.x, place.loc.y, place.size.w, place.size.h);
                Some((
                    screen.id,
                    rect.translated(ModelPoint::new(info.rect.x, info.rect.y)),
                ))
            })
            .collect()
    }

    /// A panel of the shell goes: what it showed last fades out, sliding
    /// back toward where it came from.
    pub fn panel_leaves(&mut self, panel: &LayerSurface, output: OutputId) {
        if !panel.namespace().starts_with("coquille-") {
            return;
        }
        let surface = panel.wl_surface().clone();
        let Some(motion) = self.panel_motions.remove(&surface) else {
            return;
        };
        let (Some(screen), Some(info)) = (self.output(output), self.shell.screen(output)) else {
            return;
        };
        let map = layer_map_for_output(screen);
        let Some(place) = map.layer_geometry(panel) else {
            return;
        };
        let bbox = panel.bbox();
        let place = Rect::new(
            info.rect.x + place.loc.x + bbox.loc.x,
            info.rect.y + place.loc.y + bbox.loc.y,
            bbox.size.w,
            bbox.size.h,
        );
        drop(map);
        let shown = self.panels().into_iter().find(|p| p.surface == *panel);
        let arrival = shown.map_or_else(
            || crate::panels::arrival_of(panel, None, info.rect.size()),
            |shown| shown.arrival,
        );
        let liquid = self
            .liquid
            .get(&(panel.namespace().to_owned(), info.role))
            .cloned()
            .unwrap_or_default();
        if arrival == Arrival::Own {
            return;
        }
        let tween = self
            .pace
            .tween(Instant::now(), durations::PANEL_CLOSE, Curve::EaseIn);
        if let (Some(tween), false) = (tween, motion.snapshot.is_empty()) {
            let slide = arrival.offset(place).map(|(dx, dy)| (dx / 2.0, dy / 2.0));
            let exit = Exit { tween, slide };
            self.panel_ghosts.push(PanelGhost::new(
                panel.layer(),
                place,
                motion.snapshot,
                liquid,
                exit,
            ));
            self.queue_redraw();
        }
    }

    /// How opaque the drop zones are: they fade in when they show.
    pub fn overlay_alpha(&mut self, shown: bool) -> f32 {
        if !shown || self.screenshot_mode.is_some() {
            self.overlay_since = None;
            return 1.0;
        }
        let now = Instant::now();
        let since = *self.overlay_since.get_or_insert(now);
        match self.pace.tween(since, durations::FADE, Curve::EaseOut) {
            Some(tween) => tween.at(now) as f32,
            None => 1.0,
        }
    }

    /// How the window switcher shows, at `rect`: it grows in when it opens.
    pub fn switcher_look(&self, rect: Rect) -> Look {
        let opened = self.switcher.as_ref().map(|switcher| switcher.opened);
        let entry = opened
            .and_then(|opened| self.pace.tween(opened, durations::FADE, Curve::EaseOut))
            .map(|tween| crate::animation::Entry { tween, slide: None });
        entry.map_or(Look::still(rect), |entry| entry.look(rect, Instant::now()))
    }

    /// A frame was drawn: forgets the motions that are over, and asks for
    /// another frame while any goes on.
    pub fn keep_animating(&mut self) {
        let now = Instant::now();
        let mut busy = false;
        for managed in &mut self.shell.windows {
            busy |= managed.motion.settle(now);
        }
        self.shell.ghosts.retain(|ghost| !ghost.exit.done(now));
        self.panel_ghosts.retain(|ghost| !ghost.exit.done(now));
        self.panel_motions.retain(|surface, _| surface.alive());
        busy |= !self.shell.ghosts.is_empty() || !self.panel_ghosts.is_empty();
        busy |= self.panel_motions.values().any(|motion| motion.busy(now));
        busy |= self.overlay_since.is_some_and(|since| {
            self.pace
                .tween(since, durations::FADE, Curve::Linear)
                .is_some_and(|tween| !tween.done(now))
        });
        busy |= self.switcher.as_ref().is_some_and(|switcher| {
            self.pace
                .tween(switcher.opened, durations::FADE, Curve::Linear)
                .is_some_and(|tween| !tween.done(now))
        });
        if busy {
            self.queue_redraw();
        }
    }
}

/// Where a window at `frame` goes into the dock: the bar of its screen,
/// level with the window, or the bottom of the screen without one.
fn dock_area(frame: Rect, bar: Option<Rect>, screen: Rect) -> Area {
    let center = ModelPoint::new(frame.x + frame.w / 2, frame.y + frame.h / 2);
    let (x, y) = match bar {
        Some(bar) => Side::of_bar(bar, screen).into_bar(bar, center),
        None => (f64::from(center.x), f64::from(screen.bottom())),
    };
    Area::from(frame).shrunk_to(x, y, DOCK_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_goes_into_the_bar_of_its_screen() {
        let screen = Rect::new(0, 0, 1920, 1080);
        let bar = Rect::new(8, 8, 44, 1064);
        let area = dock_area(Rect::new(800, 200, 600, 400), Some(bar), screen);

        assert_eq!(area.center(), (30.0, 400.0));
        assert!(area.w < 60.0);
    }

    #[test]
    fn without_a_dock_a_window_goes_to_the_bottom_of_its_screen() {
        let screen = Rect::new(1920, 0, 3440, 1440);
        let area = dock_area(Rect::new(2000, 100, 1000, 800), None, screen);

        assert_eq!(area.center(), (2500.0, 1440.0));
    }
}
