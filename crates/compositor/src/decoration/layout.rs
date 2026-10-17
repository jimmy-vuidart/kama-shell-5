//! Where things sit in a title bar. Pure geometry, shared by painting and
//! hit-testing so that both always agree.

use coquille_model::{Point, Rect, Size};

/// Width of a button cell; cells are as tall as the bar.
pub const BUTTON_WIDTH: i32 = 38;
const RIGHT_PADDING: i32 = 4;
const ICON_SIZE: i32 = 18;
const ICON_LEFT: i32 = 12;
const TEXT_LEFT: i32 = 40;
const TEXT_RIGHT_GAP: i32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TitlebarButton {
    Minimize,
    /// Maximizes the window over the slots of its screen.
    Maximize,
    Close,
}

/// Rectangles relative to the title bar's top-left corner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitlebarLayout {
    pub icon: Rect,
    /// Room for the title and subtitle.
    pub text: Rect,
    /// Left to right.
    pub buttons: Vec<(TitlebarButton, Rect)>,
}

impl TitlebarLayout {
    /// Buttons are right-aligned. When the bar is too narrow, the leftmost
    /// ones are dropped; the close button always stays.
    pub fn new(size: Size, with_buttons: bool) -> Self {
        let wanted = [
            TitlebarButton::Close,
            TitlebarButton::Maximize,
            TitlebarButton::Minimize,
        ]
        .into_iter()
        .filter(|_| with_buttons);

        // Laid out from the right edge; the first one (close) is never dropped.
        let mut buttons = Vec::new();
        let mut x = size.w - RIGHT_PADDING - BUTTON_WIDTH;
        for button in wanted {
            if !buttons.is_empty() && x < TEXT_LEFT {
                break;
            }
            buttons.push((button, Rect::new(x, 0, BUTTON_WIDTH, size.h)));
            x -= BUTTON_WIDTH;
        }
        buttons.reverse();

        let text_right = buttons.first().map_or(size.w, |(_, rect)| rect.x) - TEXT_RIGHT_GAP;
        Self {
            icon: Rect::new(ICON_LEFT, (size.h - ICON_SIZE) / 2, ICON_SIZE, ICON_SIZE),
            text: Rect::new(TEXT_LEFT, 0, (text_right - TEXT_LEFT).max(0), size.h),
            buttons,
        }
    }

    pub fn button_at(&self, point: Point) -> Option<TitlebarButton> {
        self.buttons
            .iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(button, _)| *button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAR: Size = Size::new(400, 38);

    fn kinds(layout: &TitlebarLayout) -> Vec<TitlebarButton> {
        layout.buttons.iter().map(|(button, _)| *button).collect()
    }

    #[test]
    fn buttons_are_right_aligned_in_order() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(
            layout.buttons,
            vec![
                (TitlebarButton::Minimize, Rect::new(282, 0, 38, 38)),
                (TitlebarButton::Maximize, Rect::new(320, 0, 38, 38)),
                (TitlebarButton::Close, Rect::new(358, 0, 38, 38)),
            ]
        );
    }

    #[test]
    fn a_bar_without_buttons_leaves_its_whole_width_to_the_text() {
        let layout = TitlebarLayout::new(BAR, false);

        assert!(layout.buttons.is_empty());
        assert_eq!(layout.text, Rect::new(40, 0, 352, 38));
    }

    #[test]
    fn the_icon_is_vertically_centered_on_the_left() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(layout.icon, Rect::new(12, 10, 18, 18));
    }

    #[test]
    fn the_text_fills_the_room_between_the_icon_and_the_buttons() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(layout.text, Rect::new(40, 0, 234, 38));
    }

    #[test]
    fn a_narrow_bar_drops_the_leftmost_button_but_keeps_close() {
        let layout = TitlebarLayout::new(Size::new(100, 38), true);

        assert_eq!(kinds(&layout), [TitlebarButton::Close]);
        assert_eq!(layout.text.w, 10);
    }

    #[test]
    fn a_bar_narrower_than_a_button_still_has_close() {
        let layout = TitlebarLayout::new(Size::new(30, 38), true);

        assert_eq!(kinds(&layout), [TitlebarButton::Close]);
        assert_eq!(layout.text.w, 0);
    }

    #[test]
    fn a_point_on_a_button_finds_it() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(
            layout.button_at(Point::new(377, 19)),
            Some(TitlebarButton::Close)
        );
        assert_eq!(
            layout.button_at(Point::new(320, 0)),
            Some(TitlebarButton::Maximize)
        );
        assert_eq!(
            layout.button_at(Point::new(282, 0)),
            Some(TitlebarButton::Minimize)
        );
    }

    #[test]
    fn a_point_on_the_title_finds_no_button() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(layout.button_at(Point::new(100, 19)), None);
        assert_eq!(layout.button_at(Point::new(398, 19)), None);
    }
}
