//! The small bar offered under a window that just opened floating: leave it,
//! tidy it on its screen, or send it to the center.

use coquille_model::{Placement, Point as ModelPoint, Rect, Size as ModelSize, SlotId, WindowId};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::utils::Transform;
use tracing::warn;

use crate::decoration::paint::BarSpec;
use crate::input::Gesture;
use crate::state::State;

/// Space kept between the bar and what it sits next to.
const MARGIN: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarAction {
    /// Leave the window floating.
    Keep,
    /// Tidy it into this slot of its screen.
    Tidy(SlotId),
    /// Tidy it into the center screen.
    Center,
}

/// The bar as it is shown.
pub struct Bar {
    pub window: WindowId,
    /// In the global space.
    pub rect: Rect,
    pub buttons: Vec<(BarAction, Rect)>,
    pub buffer: MemoryRenderBuffer,
    /// What it was painted from, and the window frame it was placed under.
    spec: BarSpec,
    anchor: Rect,
}

/// Where a bar of `size` goes for a window at `frame`: centered under it, or
/// over its bottom edge when there is no room below, and always inside
/// `screen`.
pub fn bar_position(frame: Rect, size: ModelSize, screen: Rect) -> ModelPoint {
    let x = if size.w + 2 * MARGIN > screen.w {
        screen.x
    } else {
        let centered = frame.x + (frame.w - size.w) / 2;
        centered.clamp(screen.x + MARGIN, screen.right() - MARGIN - size.w)
    };
    let below = frame.bottom() + MARGIN;
    let y = if below + size.h + MARGIN <= screen.bottom() {
        below
    } else {
        frame.bottom().min(screen.bottom()) - MARGIN - size.h
    };
    ModelPoint::new(x, y)
}

impl State {
    /// What the bar proposes for `window`, with the label of each choice.
    fn bar_choices(&self, window: WindowId) -> Vec<(BarAction, String)> {
        let desk = &self.shell.desk;
        let mut choices = vec![(BarAction::Keep, "Laisser flottante".to_owned())];

        if let Some(slot) = desk.suggested_slot(window) {
            let occupied = desk
                .screen(slot.output)
                .is_some_and(|screen| !screen.slots()[slot.index].is_empty());
            let label = if occupied { "En onglet" } else { "Ranger ici" };
            choices.push((BarAction::Tidy(slot), label.to_owned()));
        }

        if desk.can_send_to_center(window) {
            choices.push((BarAction::Center, "Au centre".to_owned()));
        }
        choices
    }

    /// Keeps the bar in step with the window it is offered for; called before
    /// each frame. The offer ends as soon as the user turns to something
    /// else, or handles the window directly.
    pub fn refresh_bar(&mut self) {
        // A window opened during a game waits without the focus: its offer
        // stands until the user leaves the game.
        let playing = self.shell.desk.game_in_play().is_some();
        let offered = self.placement_offer.and_then(|window| {
            let managed = self.shell.by_id(window)?;
            let geometry = managed.geometry;
            let still_open = geometry.floating
                && !geometry.anchored
                && geometry.visible
                && (geometry.focused || playing);
            let handled = matches!(
                self.gesture,
                Some(Gesture::Placing { .. } | Gesture::Moving { .. } | Gesture::Resizing { .. })
            );
            (still_open && !handled).then(|| (window, managed.frame(), geometry.output))
        });
        let Some((window, frame, output)) = offered else {
            self.placement_offer = None;
            self.bar = None;
            return;
        };

        let choices = self.bar_choices(window);
        let spec = BarSpec {
            labels: choices.iter().map(|(_, label)| label.clone()).collect(),
            hovered: self.bar_hover,
            theme: self.theme.kind,
            accent: self.theme.accent,
        };
        if self
            .bar
            .as_ref()
            .is_some_and(|bar| bar.spec == spec && bar.anchor == frame)
        {
            return;
        }

        let screen = self.shell.screen(output).map(|screen| screen.rect);
        let painted = self.painter.placement_bar(&spec);
        let (Some(screen), Some(painted)) = (screen, painted) else {
            self.bar = None;
            return;
        };
        let size = ModelSize::new(
            painted.pixmap.width() as i32,
            painted.pixmap.height() as i32,
        );
        let corner = bar_position(frame, size, screen);
        let buttons = choices
            .iter()
            .zip(&painted.buttons)
            .map(|((action, _), cell)| (*action, cell.translated(corner)))
            .collect();
        self.bar = Some(Bar {
            window,
            rect: Rect::new(corner.x, corner.y, size.w, size.h),
            buttons,
            buffer: MemoryRenderBuffer::from_slice(
                painted.pixmap.data(),
                Fourcc::Abgr8888,
                (size.w, size.h),
                1,
                Transform::Normal,
                None,
            ),
            spec,
            anchor: frame,
        });
    }

    /// The user chose a button of the bar.
    pub fn run_bar_action(&mut self, index: usize) {
        let Some(bar) = self.bar.take() else {
            return;
        };
        self.placement_offer = None;
        let Some((action, _)) = bar.buttons.get(index) else {
            return;
        };
        let desk = &mut self.shell.desk;
        let result = match *action {
            BarAction::Keep => Ok(()),
            BarAction::Tidy(slot) => desk.move_to_slot(bar.window, slot),
            BarAction::Center => desk.send_to_center(bar.window),
        };
        if let Err(error) = result {
            warn!(%error, "cannot place the window as asked");
        }
        // A floating window that stays floating keeps its place as it is.
        if !matches!(desk.placement(bar.window), Some(Placement::Floating { .. })) || result.is_ok()
        {
            self.apply_arrangement();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::new(1920, 0, 3440, 1440);
    const BAR: ModelSize = ModelSize::new(400, 40);

    #[test]
    fn the_bar_sits_centered_under_the_window() {
        let frame = Rect::new(2600, 300, 800, 600);

        assert_eq!(bar_position(frame, BAR, SCREEN), ModelPoint::new(2800, 908));
    }

    #[test]
    fn a_window_near_the_side_keeps_the_bar_on_its_screen() {
        let narrow = Rect::new(1928, 300, 200, 300);

        assert_eq!(
            bar_position(narrow, BAR, SCREEN),
            ModelPoint::new(1928, 608)
        );
    }

    #[test]
    fn without_room_below_the_bar_covers_the_bottom_of_the_window() {
        let tall = Rect::new(2600, 8, 800, 1424);

        assert_eq!(bar_position(tall, BAR, SCREEN), ModelPoint::new(2800, 1384));
    }

    #[test]
    fn a_bar_wider_than_the_screen_starts_at_its_left_edge() {
        let screen = Rect::new(0, 0, 300, 900);
        let frame = Rect::new(8, 100, 284, 300);

        assert_eq!(bar_position(frame, BAR, screen).x, 0);
    }
}
