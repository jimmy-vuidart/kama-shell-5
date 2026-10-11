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
/// Space between tabs, and around them in the bar.
const TAB_GAP: i32 = 4;
const TAB_MAX_WIDTH: i32 = 220;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TitlebarButton {
    AddTab,
    /// "Swap with the center"; side screens only.
    Swap,
    Minimize,
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
    pub fn new(size: Size, show_swap: bool) -> Self {
        let order = [
            TitlebarButton::Close,
            TitlebarButton::Minimize,
            TitlebarButton::Swap,
            TitlebarButton::AddTab,
        ];
        let wanted = order
            .into_iter()
            .filter(|button| show_swap || *button != TitlebarButton::Swap);

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

    /// Cells of `count` tabs, which take the place of the icon and the title:
    /// equal widths, never wider than a comfortable maximum.
    pub fn tabs(&self, count: usize) -> Vec<Rect> {
        let Ok(count) = i32::try_from(count) else {
            return Vec::new();
        };
        if count == 0 {
            return Vec::new();
        }
        let left = ICON_LEFT - TAB_GAP;
        let room = (self.text.right() - left - TAB_GAP * (count - 1)).max(0);
        let width = (room / count).min(TAB_MAX_WIDTH);
        let height = self.text.h - 2 * TAB_GAP;
        (0..count)
            .map(|index| Rect::new(left + index * (width + TAB_GAP), TAB_GAP, width, height))
            .collect()
    }

    pub fn tab_at(&self, count: usize, point: Point) -> Option<usize> {
        self.tabs(count).iter().position(|tab| tab.contains(point))
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
                (TitlebarButton::AddTab, Rect::new(244, 0, 38, 38)),
                (TitlebarButton::Swap, Rect::new(282, 0, 38, 38)),
                (TitlebarButton::Minimize, Rect::new(320, 0, 38, 38)),
                (TitlebarButton::Close, Rect::new(358, 0, 38, 38)),
            ]
        );
    }

    #[test]
    fn the_swap_button_is_absent_when_not_shown() {
        let layout = TitlebarLayout::new(BAR, false);

        assert_eq!(
            kinds(&layout),
            [
                TitlebarButton::AddTab,
                TitlebarButton::Minimize,
                TitlebarButton::Close
            ]
        );
        assert_eq!(layout.buttons[0].1, Rect::new(282, 0, 38, 38));
    }

    #[test]
    fn the_icon_is_vertically_centered_on_the_left() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(layout.icon, Rect::new(12, 10, 18, 18));
    }

    #[test]
    fn the_text_fills_the_room_between_the_icon_and_the_buttons() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(layout.text, Rect::new(40, 0, 196, 38));
    }

    #[test]
    fn a_narrow_bar_drops_the_leftmost_buttons_but_keeps_close() {
        let layout = TitlebarLayout::new(Size::new(120, 38), true);

        assert_eq!(
            kinds(&layout),
            [TitlebarButton::Minimize, TitlebarButton::Close]
        );
        assert_eq!(layout.text.w, 0);
    }

    #[test]
    fn a_bar_narrower_than_a_button_still_has_close() {
        let layout = TitlebarLayout::new(Size::new(30, 38), true);

        assert_eq!(kinds(&layout), [TitlebarButton::Close]);
    }

    #[test]
    fn tabs_share_the_room_left_of_the_buttons() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(
            layout.tabs(2),
            [Rect::new(8, 4, 112, 30), Rect::new(124, 4, 112, 30)]
        );
    }

    #[test]
    fn tabs_are_not_wider_than_their_maximum() {
        let layout = TitlebarLayout::new(Size::new(1000, 38), false);

        assert_eq!(
            layout.tabs(2),
            [Rect::new(8, 4, 220, 30), Rect::new(232, 4, 220, 30)]
        );
    }

    #[test]
    fn a_single_tab_is_laid_out_too() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(layout.tabs(1), [Rect::new(8, 4, 220, 30)]);
    }

    #[test]
    fn no_tab_gives_no_cell_and_a_narrow_bar_no_negative_width() {
        assert!(TitlebarLayout::new(BAR, true).tabs(0).is_empty());

        let narrow = TitlebarLayout::new(Size::new(60, 38), true);
        assert!(narrow.tabs(3).iter().all(|tab| tab.w >= 0));
    }

    #[test]
    fn a_point_on_a_button_finds_it() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(
            layout.button_at(Point::new(377, 19)),
            Some(TitlebarButton::Close)
        );
        assert_eq!(
            layout.button_at(Point::new(282, 0)),
            Some(TitlebarButton::Swap)
        );
    }

    #[test]
    fn a_point_on_the_title_finds_no_button() {
        let layout = TitlebarLayout::new(BAR, true);

        assert_eq!(layout.button_at(Point::new(100, 19)), None);
        assert_eq!(layout.button_at(Point::new(398, 19)), None);
    }
}
