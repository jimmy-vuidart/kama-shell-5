//! Where the capsules of a slot sit in the band above it. Pure geometry,
//! shared by painting and hit-testing so that both always agree.

use coquille_model::{Point, Rect, Size};

/// Space between the edge of the group and its capsules.
const GROUP_PADDING: i32 = 3;
const CAPSULE_GAP: i32 = 2;
const PADDING: i32 = 10;
const ICON_SIZE: i32 = 14;
const ICON_GAP: i32 = 6;
const BUTTON_SIZE: i32 = 20;
/// Space between the label and the buttons, and right of the buttons.
const BUTTONS_GAP: i32 = 4;
const BUTTONS_WIDTH: i32 = 2 * BUTTON_SIZE;
const LABEL_MAX_WIDTH: i32 = 160;
/// A capsule squeezed down to its icon.
const MIN_WIDTH: i32 = PADDING + ICON_SIZE + PADDING;
/// Below this a capsule has no room for buttons next to its icon.
const BUTTONS_MIN_WIDTH: i32 = PADDING + ICON_SIZE + BUTTONS_GAP + BUTTONS_WIDTH + BUTTONS_GAP;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapsuleButton {
    Minimize,
    Close,
}

/// How wide a capsule wants to be for a label of this width. The room of the
/// buttons is part of it even where they only show under the pointer:
/// hovering a capsule must not move it, and a click aimed at a name must not
/// land on a button that came over it.
fn wanted_width(label_width: i32) -> i32 {
    let label = label_width.clamp(0, LABEL_MAX_WIDTH);
    PADDING + ICON_SIZE + ICON_GAP + label + BUTTONS_GAP + BUTTONS_WIDTH + BUTTONS_GAP
}

/// Rectangles relative to the band's top-left corner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleCell {
    pub rect: Rect,
    pub icon: Rect,
    pub label: Rect,
    /// Left to right; empty when the capsule is too narrow.
    pub buttons: Vec<(CapsuleButton, Rect)>,
}

impl CapsuleCell {
    fn new(rect: Rect) -> Self {
        let mut buttons = Vec::new();
        if rect.w >= BUTTONS_MIN_WIDTH {
            let y = rect.y + (rect.h - BUTTON_SIZE) / 2;
            let left = rect.right() - BUTTONS_GAP - BUTTONS_WIDTH;
            let kinds = [CapsuleButton::Minimize, CapsuleButton::Close];
            for (button, x) in kinds.into_iter().zip([left, left + BUTTON_SIZE]) {
                buttons.push((button, Rect::new(x, y, BUTTON_SIZE, BUTTON_SIZE)));
            }
        }
        let label_left = rect.x + PADDING + ICON_SIZE + ICON_GAP;
        let label_right = match buttons.first() {
            Some((_, first)) => first.x - BUTTONS_GAP,
            None => rect.right() - PADDING,
        };
        Self {
            rect,
            icon: Rect::new(
                rect.x + PADDING,
                rect.y + (rect.h - ICON_SIZE) / 2,
                ICON_SIZE,
                ICON_SIZE,
            ),
            label: Rect::new(
                label_left,
                rect.y,
                (label_right - label_left).max(0),
                rect.h,
            ),
            buttons,
        }
    }
}

/// The capsules of one slot, in the band above it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GroupLayout {
    /// The rounded box around the capsules; never wider than the band.
    pub rect: Rect,
    pub capsules: Vec<CapsuleCell>,
}

impl GroupLayout {
    /// One capsule per label width, as the painter measures them. The group
    /// is centered in the band. Capsules are as wide as their content; when
    /// the band is too narrow, the widest ones shrink first, all of them down
    /// to their icon at most. Past that, the last ones are cut by the end of
    /// the band.
    pub fn new(band: Size, label_widths: &[i32]) -> Self {
        let Ok(count) = i32::try_from(label_widths.len()) else {
            return Self::default();
        };
        if count == 0 || band.w <= 0 || band.h <= 0 {
            return Self::default();
        }
        let wanted: Vec<i32> = label_widths.iter().copied().map(wanted_width).collect();
        let between = CAPSULE_GAP * (count - 1);
        let room = band.w - 2 * GROUP_PADDING - between;
        let limit = width_limit(&wanted, room);
        let widths = wanted.iter().map(|width| (*width).min(limit));

        let total: i32 = widths.clone().sum::<i32>() + between + 2 * GROUP_PADDING;
        let rect = Rect::new(((band.w - total) / 2).max(0), 0, total.min(band.w), band.h);

        let mut x = rect.x + GROUP_PADDING;
        let capsules = widths
            .map(|width| {
                let cell = Rect::new(x, GROUP_PADDING, width, band.h - 2 * GROUP_PADDING);
                x += width + CAPSULE_GAP;
                CapsuleCell::new(cell)
            })
            .collect();
        Self { rect, capsules }
    }

    pub fn capsule_at(&self, point: Point) -> Option<usize> {
        if !self.rect.contains(point) {
            return None;
        }
        self.capsules
            .iter()
            .position(|capsule| capsule.rect.contains(point))
    }

    pub fn button_at(&self, point: Point) -> Option<(usize, CapsuleButton)> {
        let index = self.capsule_at(point)?;
        let mut buttons = self.capsules[index].buttons.iter();
        let (button, _) = buttons.find(|(_, rect)| rect.contains(point))?;
        Some((index, *button))
    }
}

/// The widest a capsule may be for all of them to fit in `room`, never under
/// the width of a capsule squeezed down to its icon.
fn width_limit(wanted: &[i32], room: i32) -> i32 {
    let widest = wanted.iter().copied().max().unwrap_or(MIN_WIDTH);
    let fits = |limit: i32| wanted.iter().map(|width| (*width).min(limit)).sum::<i32>() <= room;
    if fits(widest) {
        return widest;
    }
    // The largest limit that fits, by bisection: `low` fits or is the floor.
    let (mut low, mut high) = (MIN_WIDTH, widest);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if fits(middle) {
            low = middle;
        } else {
            high = middle;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAND: Size = Size::new(948, 32);

    fn widths(layout: &GroupLayout) -> Vec<i32> {
        layout.capsules.iter().map(|cell| cell.rect.w).collect()
    }

    #[test]
    fn a_group_is_centered_in_its_band_and_as_tall() {
        let layout = GroupLayout::new(BAND, &[60]);

        assert_eq!(layout.rect, Rect::new(402, 0, 144, 32));
        assert_eq!(layout.capsules[0].rect, Rect::new(405, 3, 138, 26));
    }

    #[test]
    fn the_icon_the_label_and_the_buttons_follow_each_other() {
        let layout = GroupLayout::new(BAND, &[60]);
        let cell = &layout.capsules[0];

        assert_eq!(cell.icon, Rect::new(415, 9, 14, 14));
        assert_eq!(cell.label, Rect::new(435, 3, 60, 26));
        assert_eq!(
            cell.buttons,
            [
                (CapsuleButton::Minimize, Rect::new(499, 6, 20, 20)),
                (CapsuleButton::Close, Rect::new(519, 6, 20, 20)),
            ]
        );
    }

    #[test]
    fn capsules_follow_each_other_with_a_small_gap() {
        let layout = GroupLayout::new(BAND, &[60, 40]);

        assert_eq!(layout.rect, Rect::new(342, 0, 264, 32));
        assert_eq!(layout.capsules[0].rect, Rect::new(345, 3, 138, 26));
        assert_eq!(layout.capsules[1].rect, Rect::new(485, 3, 118, 26));
    }

    #[test]
    fn a_long_label_does_not_make_a_capsule_endless() {
        let layout = GroupLayout::new(BAND, &[500]);

        assert_eq!(widths(&layout), [238]);
    }

    #[test]
    fn the_buttons_never_come_over_the_label() {
        let layout = GroupLayout::new(BAND, &[4]);
        let cell = &layout.capsules[0];

        assert_eq!(cell.buttons.len(), 2);
        assert!(cell.label.right() <= cell.buttons[0].1.x);
    }

    #[test]
    fn a_crowded_group_shrinks_its_widest_capsules_first() {
        let band = Size::new(500, 32);

        let layout = GroupLayout::new(band, &[40, 160, 160]);

        assert_eq!(widths(&layout), [118, 186, 186]);
        assert_eq!(layout.rect, Rect::new(0, 0, 500, 32));
    }

    #[test]
    fn capsules_are_never_narrower_than_their_icon() {
        let band = Size::new(100, 32);

        let layout = GroupLayout::new(band, &[80; 5]);

        assert_eq!(widths(&layout), [34; 5]);
        assert_eq!(layout.capsules[0].icon, Rect::new(13, 9, 14, 14));
        // The group stops with the band; what does not fit is cut.
        assert_eq!(layout.rect, Rect::new(0, 0, 100, 32));
        assert_eq!(layout.capsule_at(Point::new(50, 16)), Some(1));
        assert_eq!(layout.capsule_at(Point::new(150, 16)), None);
    }

    #[test]
    fn a_capsule_too_narrow_for_them_has_no_buttons() {
        let roomy = GroupLayout::new(Size::new(300, 32), &[160; 4]);
        let tight = GroupLayout::new(Size::new(296, 32), &[160; 4]);

        assert_eq!(widths(&roomy), [72; 4]);
        assert!(roomy.capsules.iter().all(|cell| cell.buttons.len() == 2));
        assert!(roomy.capsules.iter().all(|cell| cell.label.w == 0));
        assert_eq!(widths(&tight), [71; 4]);
        assert!(tight.capsules.iter().all(|cell| cell.buttons.is_empty()));
        assert_eq!(tight.capsules[0].label.w, 31);
    }

    #[test]
    fn a_point_finds_its_capsule_and_its_button() {
        let layout = GroupLayout::new(BAND, &[60, 60]);
        // Capsules at 335..473 and 475..613, their buttons from 429 and 569.
        let on_first = Point::new(400, 16);

        assert_eq!(layout.capsule_at(on_first), Some(0));
        assert_eq!(layout.button_at(on_first), None);
        assert_eq!(
            layout.button_at(Point::new(440, 16)),
            Some((0, CapsuleButton::Minimize))
        );
        assert_eq!(layout.capsule_at(Point::new(600, 16)), Some(1));
        assert_eq!(
            layout.button_at(Point::new(600, 16)),
            Some((1, CapsuleButton::Close))
        );
    }

    #[test]
    fn a_point_between_capsules_or_outside_the_group_finds_nothing() {
        let layout = GroupLayout::new(BAND, &[60, 60]);

        assert_eq!(layout.capsule_at(Point::new(474, 16)), None);
        assert_eq!(layout.capsule_at(Point::new(100, 16)), None);
        assert_eq!(layout.capsule_at(Point::new(400, 1)), None);
        assert_eq!(layout.button_at(Point::new(100, 16)), None);
    }

    #[test]
    fn no_capsule_or_no_room_gives_nothing() {
        assert_eq!(GroupLayout::new(BAND, &[]), GroupLayout::default());
        assert_eq!(
            GroupLayout::new(Size::new(0, 32), &[60]),
            GroupLayout::default()
        );

        let cramped = GroupLayout::new(Size::new(20, 32), &[60]);
        assert!(cramped.rect.w <= 20);
        assert!(cramped.capsules.iter().all(|cell| cell.label.w >= 0));
    }
}
