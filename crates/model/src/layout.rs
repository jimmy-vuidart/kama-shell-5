//! The short list of per-screen layouts and the columns they make.
//!
//! A layout gives a screen its greatest number of columns. The columns shown
//! are the ones that hold windows: one column takes the whole screen, two
//! share it, and so on up to the greatest number.

use serde::{Deserialize, Serialize};

use crate::geometry::{Insets, Point, Rect};
use crate::ids::OutputId;
use crate::output::OutputRole;

/// How a screen arranges the windows tidied into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutKind {
    /// One column: the windows of the screen are tabs of the same pile.
    Full,
    /// Up to two columns, half and half.
    TwoColumns,
    /// Up to three columns, a third each.
    ThreeColumns,
    /// A main column on the left and a side column: two thirds and a third.
    MainSide,
}

impl LayoutKind {
    /// Every layout, in the order the settings show them.
    pub const ALL: [Self; 4] = [
        Self::Full,
        Self::TwoColumns,
        Self::ThreeColumns,
        Self::MainSide,
    ];

    /// The greatest number of columns the screen shows.
    pub const fn max_columns(self) -> usize {
        match self {
            Self::Full => 1,
            Self::TwoColumns | Self::MainSide => 2,
            Self::ThreeColumns => 3,
        }
    }

    /// Landscape side screens share by default; the center stays whole.
    pub const fn default_for(role: OutputRole) -> Self {
        if role.is_side() {
            Self::TwoColumns
        } else {
            Self::Full
        }
    }

    /// User-visible name.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Full => "Plein écran",
            Self::TwoColumns => "2 colonnes",
            Self::ThreeColumns => "3 colonnes",
            Self::MainSide => "Maître / esclave",
        }
    }

    /// The name in the configuration file.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::TwoColumns => "two-columns",
            Self::ThreeColumns => "three-columns",
            Self::MainSide => "main-side",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// The widths of the columns shown, in thousandths of the room they share
/// (the usable width minus the gaps). They add up to 1000, and each column
/// keeps at least [`Split::MIN`] so that none can vanish.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Split(Vec<u16>);

impl Split {
    /// The narrowest a column can be, when there are several: a fifth.
    pub const MIN: u16 = 200;
    const WHOLE: u16 = 1000;

    /// The proportions `count` columns of `kind` start with.
    pub fn default_for(kind: LayoutKind, count: usize) -> Self {
        Self(match (kind, count) {
            (_, 0 | 1) => vec![Self::WHOLE],
            (LayoutKind::MainSide, 2) => vec![667, 333],
            (_, 2) => vec![500, 500],
            (_, 3) => vec![333, 334, 333],
            // No layout has more columns: shared evenly all the same.
            (_, count) => {
                let each = Self::WHOLE / count as u16;
                let mut widths = vec![each; count];
                widths[count - 1] = Self::WHOLE - each * (count as u16 - 1);
                widths
            }
        })
    }

    /// `widths` when they are proportions of `count` columns, else those
    /// `count` columns of `kind` start with. For proportions read back from
    /// a file.
    pub fn normalized(widths: Vec<u16>, kind: LayoutKind, count: usize) -> Self {
        let sum: u32 = widths.iter().map(|width| u32::from(*width)).sum();
        let wide_enough = count < 2 || widths.iter().all(|width| *width >= Self::MIN);
        if widths.len() == count.max(1) && sum == u32::from(Self::WHOLE) && wide_enough {
            Self(widths)
        } else {
            Self::default_for(kind, count)
        }
    }

    pub fn widths(&self) -> &[u16] {
        &self.0
    }

    pub fn count(&self) -> usize {
        self.0.len()
    }

    /// The columns inside `area`, left to right, with `gap` between them
    /// and along the edges of `area`. The last one ends exactly at the gap
    /// before the right edge.
    pub fn column_rects(&self, area: Rect, gap: i32) -> Vec<Rect> {
        let inner = area.inset(Insets::uniform(gap));
        let count = self.count() as i32;
        let room = inner.w - gap * (count - 1);
        let mut rects = Vec::with_capacity(self.count());
        let mut reached = 0u32;
        let mut start = 0;
        for (index, width) in self.0.iter().enumerate() {
            reached += u32::from(*width);
            let end = share(room, reached);
            let x = inner.x + start + index as i32 * gap;
            rects.push(Rect::new(x, inner.y, end - start, inner.h));
            start = end;
        }
        rects
    }

    /// The gaps between the columns, where the user grabs to change their
    /// widths: one fewer than the columns.
    pub fn separators(&self, area: Rect, gap: i32) -> Vec<Rect> {
        let rects = self.column_rects(area, gap);
        rects
            .windows(2)
            .map(|pair| Rect::new(pair[0].right(), pair[0].y, gap, pair[0].h))
            .collect()
    }

    /// The proportions with separation `index` under `point`: only the two
    /// columns on either side of it change, and neither goes below
    /// [`Split::MIN`]. `None` when there is no such separation.
    pub fn with_separator(&self, index: usize, area: Rect, gap: i32, point: Point) -> Option<Self> {
        if index + 1 >= self.count() {
            return None;
        }
        let inner = area.inset(Insets::uniform(gap));
        let room = inner.w - gap * (self.count() as i32 - 1);
        if room <= 0 {
            return None;
        }
        // The pointer holds the middle of the gap.
        let offset = point.x - inner.x - index as i32 * gap - gap / 2;
        let at = (i64::from(offset) * i64::from(Self::WHOLE) + i64::from(room) / 2)
            .div_euclid(i64::from(room));
        let before: i64 = self.0[..index].iter().map(|width| i64::from(*width)).sum();
        let pair = i64::from(self.0[index]) + i64::from(self.0[index + 1]);
        let min = i64::from(Self::MIN);
        // `pair` is at least twice the minimum, so the clamp has room, and
        // the result is no larger than `pair`, which fits in `u16`.
        let first = (at - before).clamp(min, pair - min);
        let mut widths = self.0.clone();
        widths[index] = first as u16;
        widths[index + 1] = (pair - first) as u16;
        Some(Self(widths))
    }
}

/// The part of `room` pixels up to `reached` thousandths, rounded to nearest.
fn share(room: i32, reached: u32) -> i32 {
    let scaled = i64::from(room) * i64::from(reached);
    // No larger than `room` as long as `reached` is at most 1000.
    ((scaled + 500) / 1000) as i32
}

/// A slot of a screen: one of the columns it shows. Slots are numbered left
/// to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotId {
    pub output: OutputId,
    pub index: usize,
}

impl SlotId {
    pub const fn new(output: OutputId, index: usize) -> Self {
        Self { output, index }
    }
}

/// A pile of tabs: the one of a slot, or the one of the windows maximized
/// over the slots of a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StackId {
    Slot(SlotId),
    Maximized(OutputId),
}

impl StackId {
    pub const fn output(self) -> OutputId {
        match self {
            Self::Slot(slot) => slot.output,
            Self::Maximized(output) => output,
        }
    }
}

impl From<SlotId> for StackId {
    fn from(slot: SlotId) -> Self {
        Self::Slot(slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: Rect = Rect::new(52, 0, 1868, 1080);
    const ULTRAWIDE: Rect = Rect::new(0, 0, 3440, 1440);

    fn split(widths: &[u16]) -> Split {
        Split(widths.to_vec())
    }

    #[test]
    fn greatest_numbers_of_columns() {
        assert_eq!(LayoutKind::Full.max_columns(), 1);
        assert_eq!(LayoutKind::TwoColumns.max_columns(), 2);
        assert_eq!(LayoutKind::ThreeColumns.max_columns(), 3);
        assert_eq!(LayoutKind::MainSide.max_columns(), 2);
    }

    #[test]
    fn side_screens_default_to_two_columns_and_the_center_to_full() {
        assert_eq!(
            LayoutKind::default_for(OutputRole::Left),
            LayoutKind::TwoColumns
        );
        assert_eq!(
            LayoutKind::default_for(OutputRole::Right),
            LayoutKind::TwoColumns
        );
        assert_eq!(
            LayoutKind::default_for(OutputRole::Center),
            LayoutKind::Full
        );
    }

    #[test]
    fn layouts_are_named_as_in_the_file() {
        for kind in LayoutKind::ALL {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.name()));
            assert_eq!(LayoutKind::named(kind.name()), Some(kind));
        }
        assert_eq!(LayoutKind::named("rows"), None);
    }

    #[test]
    fn default_proportions() {
        assert_eq!(Split::default_for(LayoutKind::Full, 1), split(&[1000]));
        assert_eq!(
            Split::default_for(LayoutKind::TwoColumns, 2),
            split(&[500, 500])
        );
        assert_eq!(
            Split::default_for(LayoutKind::ThreeColumns, 2),
            split(&[500, 500])
        );
        assert_eq!(
            Split::default_for(LayoutKind::ThreeColumns, 3),
            split(&[333, 334, 333])
        );
        assert_eq!(
            Split::default_for(LayoutKind::MainSide, 2),
            split(&[667, 333])
        );
        assert_eq!(Split::default_for(LayoutKind::MainSide, 1), split(&[1000]));
    }

    #[test]
    fn proportions_read_back_are_repaired() {
        let kind = LayoutKind::ThreeColumns;
        assert_eq!(
            Split::normalized(vec![300, 400, 300], kind, 3),
            split(&[300, 400, 300])
        );
        // Wrong count, wrong sum, a column too narrow.
        assert_eq!(
            Split::normalized(vec![500, 500], kind, 3),
            Split::default_for(kind, 3)
        );
        assert_eq!(
            Split::normalized(vec![300, 300, 300], kind, 3),
            Split::default_for(kind, 3)
        );
        assert_eq!(
            Split::normalized(vec![100, 450, 450], kind, 3),
            Split::default_for(kind, 3)
        );
    }

    #[test]
    fn one_column_fills_the_area_minus_the_gap() {
        assert_eq!(
            split(&[1000]).column_rects(SIDE, 8),
            vec![Rect::new(60, 8, 1852, 1064)]
        );
    }

    #[test]
    fn two_columns_split_evenly_by_default() {
        assert_eq!(
            split(&[500, 500]).column_rects(SIDE, 8),
            vec![Rect::new(60, 8, 922, 1064), Rect::new(990, 8, 922, 1064)]
        );
    }

    #[test]
    fn columns_follow_their_proportions() {
        let rects = split(&[580, 420]).column_rects(SIDE, 8);

        assert_eq!(
            rects,
            vec![Rect::new(60, 8, 1070, 1064), Rect::new(1138, 8, 774, 1064)]
        );
        assert_eq!(rects[1].right(), SIDE.right() - 8);
    }

    #[test]
    fn three_columns_end_at_the_right_edge() {
        let rects = Split::default_for(LayoutKind::ThreeColumns, 3).column_rects(ULTRAWIDE, 8);

        assert_eq!(
            rects,
            vec![
                Rect::new(8, 8, 1135, 1424),
                Rect::new(1151, 8, 1138, 1424),
                Rect::new(2297, 8, 1135, 1424),
            ]
        );
        assert_eq!(rects[2].right(), 3440 - 8);
    }

    #[test]
    fn main_side_gives_two_thirds_to_the_main_column() {
        let rects = Split::default_for(LayoutKind::MainSide, 2).column_rects(ULTRAWIDE, 8);

        assert_eq!(
            rects,
            vec![Rect::new(8, 8, 2278, 1424), Rect::new(2294, 8, 1138, 1424)]
        );
    }

    #[test]
    fn separators_are_the_gaps_between_columns() {
        let area = Rect::new(0, 0, 1920, 1080);

        assert_eq!(
            split(&[500, 500]).separators(area, 8),
            vec![Rect::new(956, 8, 8, 1064)]
        );
        assert_eq!(split(&[333, 334, 333]).separators(area, 8).len(), 2);
        assert!(split(&[1000]).separators(area, 8).is_empty());
    }

    #[test]
    fn a_separation_follows_the_pointer() {
        let area = Rect::new(0, 0, 1920, 1080);

        assert_eq!(
            split(&[500, 500]).with_separator(0, area, 8, Point::new(1112, 500)),
            Some(split(&[580, 420]))
        );
    }

    #[test]
    fn moving_a_separation_only_changes_its_two_columns() {
        let area = Rect::new(0, 0, 1920, 1080);
        let three = split(&[333, 334, 333]);
        let moved = three
            .with_separator(1, area, 8, Point::new(1500, 500))
            .unwrap();

        assert_eq!(moved.widths()[0], 333);
        assert_eq!(
            moved.widths().iter().map(|w| u32::from(*w)).sum::<u32>(),
            1000
        );
        assert!(moved.widths()[1] > 334);
        let separation = moved.separators(area, 8)[1];
        assert!((separation.x + 4 - 1500).abs() <= 2, "{separation:?}");
    }

    #[test]
    fn a_separation_keeps_both_columns_wide_enough() {
        let three = split(&[333, 334, 333]);

        let far_left = three
            .with_separator(1, SIDE, 8, Point::new(0, 500))
            .unwrap();
        assert_eq!(far_left, split(&[333, 200, 467]));
        let far_right = three
            .with_separator(1, SIDE, 8, Point::new(5000, 500))
            .unwrap();
        assert_eq!(far_right, split(&[333, 467, 200]));
    }

    #[test]
    fn there_is_no_separation_past_the_last_column() {
        assert_eq!(
            split(&[500, 500]).with_separator(1, SIDE, 8, Point::new(900, 500)),
            None
        );
        assert_eq!(
            split(&[1000]).with_separator(0, SIDE, 8, Point::new(900, 500)),
            None
        );
    }

    #[test]
    fn a_zero_gap_fills_the_whole_area() {
        let area = Rect::new(0, 0, 100, 100).inset(Insets::uniform(10));

        assert_eq!(
            split(&[1000]).column_rects(area, 0),
            vec![Rect::new(10, 10, 80, 80)]
        );
    }
}
