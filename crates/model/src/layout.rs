//! The short list of per-screen layouts and the slots they define.

use serde::{Deserialize, Serialize};

use crate::geometry::{Insets, Point, Rect};
use crate::ids::OutputId;
use crate::output::OutputRole;

/// Share of a split given to its first slot, in thousandths.
///
/// Kept between 20 % and 80 % so that neither slot can vanish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "u16", into = "u16")]
pub struct Ratio(u16);

impl Ratio {
    pub const MIN: Self = Self(200);
    pub const MAX: Self = Self(800);
    pub const HALF: Self = Self(500);

    pub fn from_permille(permille: u16) -> Self {
        Self(permille.clamp(Self::MIN.0, Self::MAX.0))
    }

    pub fn from_percent(percent: u8) -> Self {
        Self::from_permille(u16::from(percent) * 10)
    }

    /// From a fraction such as `0.58`. Not-a-number gives [`Ratio::HALF`].
    pub fn from_fraction(fraction: f64) -> Self {
        if fraction.is_nan() {
            return Self::HALF;
        }
        // The clamp keeps the value far inside `u16`, so the cast is exact.
        Self(
            (fraction * 1000.0)
                .round()
                .clamp(f64::from(Self::MIN.0), f64::from(Self::MAX.0)) as u16,
        )
    }

    pub const fn permille(self) -> u16 {
        self.0
    }

    pub fn fraction(self) -> f64 {
        f64::from(self.0) / 1000.0
    }

    /// Length of the first slot out of `total` pixels, rounded to nearest.
    pub fn first_len(self, total: i32) -> i32 {
        let scaled = i64::from(total) * i64::from(self.0);
        // `scaled / 1000` is no larger than `total`, so it fits back in `i32`.
        ((scaled + 500) / 1000) as i32
    }
}

impl Default for Ratio {
    fn default() -> Self {
        Self::HALF
    }
}

impl From<u16> for Ratio {
    fn from(permille: u16) -> Self {
        Self::from_permille(permille)
    }
}

impl From<Ratio> for u16 {
    fn from(ratio: Ratio) -> Self {
        ratio.0
    }
}

/// How a screen arranges its windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutKind {
    /// One slot filling the screen.
    Full,
    /// Two slots side by side.
    Columns,
    /// Two slots, top and bottom.
    Rows,
    /// No slot: overlapping floating windows, like a classic desktop.
    Free,
    /// Two thirds plus one third; only on the ultra-wide center.
    TwoThirdsOneThird,
}

impl LayoutKind {
    pub const fn slot_count(self) -> usize {
        match self {
            Self::Free => 0,
            Self::Full => 1,
            Self::Columns | Self::Rows | Self::TwoThirdsOneThird => 2,
        }
    }

    pub const fn allowed_on(self, role: OutputRole) -> bool {
        match self {
            Self::TwoThirdsOneThird => matches!(role, OutputRole::Center),
            _ => true,
        }
    }

    /// Landscape side screens share by default; the center stays whole.
    pub const fn default_for(role: OutputRole) -> Self {
        if role.is_side() {
            Self::Columns
        } else {
            Self::Full
        }
    }

    /// User-visible name.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Full => "Plein",
            Self::Columns => "Côte à côte",
            Self::Rows => "Haut / bas",
            Self::Free => "Écran libre",
            Self::TwoThirdsOneThird => "Deux tiers + un tiers",
        }
    }

    /// The gap between the two slots, where the user grabs to change the
    /// ratio. `None` for layouts in one piece or with a fixed split.
    pub fn separator(self, area: Rect, ratio: Ratio, gap: i32) -> Option<Rect> {
        let first = *self.slot_rects(area, ratio, gap).first()?;
        match self {
            Self::Columns => Some(Rect::new(first.right(), first.y, gap, first.h)),
            Self::Rows => Some(Rect::new(first.x, first.bottom(), first.w, gap)),
            _ => None,
        }
    }

    /// The ratio that puts the separation under `point`.
    pub fn ratio_at(self, area: Rect, gap: i32, point: Point) -> Option<Ratio> {
        let inner = area.inset(Insets::uniform(gap));
        let (offset, room) = match self {
            Self::Columns => (point.x - inner.x, inner.w - gap),
            Self::Rows => (point.y - inner.y, inner.h - gap),
            _ => return None,
        };
        if room <= 0 {
            return None;
        }
        // The pointer holds the middle of the gap.
        let first = offset - gap / 2;
        Some(Ratio::from_fraction(f64::from(first) / f64::from(room)))
    }

    /// Slot rectangles inside `area`, with `gap` between slots and along the
    /// edges of `area`. `ratio` only matters to `Columns` and `Rows`.
    pub fn slot_rects(self, area: Rect, ratio: Ratio, gap: i32) -> Vec<Rect> {
        let inner = area.inset(Insets::uniform(gap));
        match self {
            Self::Free => Vec::new(),
            Self::Full => vec![inner],
            Self::Columns => {
                let (left, right) = inner.split_columns(ratio.first_len(inner.w - gap), gap);
                vec![left, right]
            }
            Self::Rows => {
                let (top, bottom) = inner.split_rows(ratio.first_len(inner.h - gap), gap);
                vec![top, bottom]
            }
            Self::TwoThirdsOneThird => {
                let (wide, narrow) = inner.split_columns(((inner.w - gap) * 2 + 1) / 3, gap);
                vec![wide, narrow]
            }
        }
    }
}

/// A slot of a screen. Slots are numbered left to right, top to bottom.
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

    #[test]
    fn ratio_is_kept_between_20_and_80_percent() {
        assert_eq!(Ratio::from_percent(5), Ratio::MIN);
        assert_eq!(Ratio::from_percent(95), Ratio::MAX);
        assert_eq!(Ratio::from_fraction(1.5), Ratio::MAX);
        assert_eq!(Ratio::from_fraction(-3.0), Ratio::MIN);
        assert_eq!(Ratio::from_percent(58).permille(), 580);
    }

    #[test]
    fn ratio_from_not_a_number_is_half() {
        assert_eq!(Ratio::from_fraction(f64::NAN), Ratio::HALF);
    }

    #[test]
    fn ratio_first_len_rounds_to_nearest() {
        assert_eq!(Ratio::HALF.first_len(1844), 922);
        assert_eq!(Ratio::from_percent(58).first_len(1844), 1070);
        assert_eq!(Ratio::HALF.first_len(5), 3);
    }

    #[test]
    fn deserialized_ratio_is_clamped() {
        let ratio: Ratio = serde_json::from_str("950").unwrap();

        assert_eq!(ratio, Ratio::MAX);
    }

    #[test]
    fn slot_counts() {
        assert_eq!(LayoutKind::Full.slot_count(), 1);
        assert_eq!(LayoutKind::Columns.slot_count(), 2);
        assert_eq!(LayoutKind::Rows.slot_count(), 2);
        assert_eq!(LayoutKind::Free.slot_count(), 0);
        assert_eq!(LayoutKind::TwoThirdsOneThird.slot_count(), 2);
    }

    #[test]
    fn two_thirds_one_third_is_only_allowed_on_the_center() {
        assert!(LayoutKind::TwoThirdsOneThird.allowed_on(OutputRole::Center));
        assert!(!LayoutKind::TwoThirdsOneThird.allowed_on(OutputRole::Left));
        assert!(!LayoutKind::TwoThirdsOneThird.allowed_on(OutputRole::Right));
    }

    #[test]
    fn every_other_layout_is_allowed_everywhere() {
        for kind in [
            LayoutKind::Full,
            LayoutKind::Columns,
            LayoutKind::Rows,
            LayoutKind::Free,
        ] {
            for role in [OutputRole::Left, OutputRole::Center, OutputRole::Right] {
                assert!(kind.allowed_on(role), "{kind:?} on {role:?}");
            }
        }
    }

    #[test]
    fn side_screens_default_to_columns_and_the_center_to_full() {
        assert_eq!(
            LayoutKind::default_for(OutputRole::Left),
            LayoutKind::Columns
        );
        assert_eq!(
            LayoutKind::default_for(OutputRole::Right),
            LayoutKind::Columns
        );
        assert_eq!(
            LayoutKind::default_for(OutputRole::Center),
            LayoutKind::Full
        );
    }

    #[test]
    fn full_fills_the_area_minus_the_gap() {
        assert_eq!(
            LayoutKind::Full.slot_rects(SIDE, Ratio::HALF, 8),
            vec![Rect::new(60, 8, 1852, 1064)]
        );
    }

    #[test]
    fn columns_split_evenly_by_default() {
        assert_eq!(
            LayoutKind::Columns.slot_rects(SIDE, Ratio::HALF, 8),
            vec![Rect::new(60, 8, 922, 1064), Rect::new(990, 8, 922, 1064)]
        );
    }

    #[test]
    fn columns_follow_the_ratio() {
        let rects = LayoutKind::Columns.slot_rects(SIDE, Ratio::from_percent(58), 8);

        assert_eq!(
            rects,
            vec![Rect::new(60, 8, 1070, 1064), Rect::new(1138, 8, 774, 1064)]
        );
        assert_eq!(rects[1].right(), SIDE.right() - 8);
    }

    #[test]
    fn rows_split_top_and_bottom() {
        assert_eq!(
            LayoutKind::Rows.slot_rects(SIDE, Ratio::HALF, 8),
            vec![Rect::new(60, 8, 1852, 528), Rect::new(60, 544, 1852, 528)]
        );
    }

    #[test]
    fn two_thirds_one_third_ignores_the_ratio() {
        let rects = LayoutKind::TwoThirdsOneThird.slot_rects(ULTRAWIDE, Ratio::MIN, 8);

        assert_eq!(
            rects,
            vec![Rect::new(8, 8, 2277, 1424), Rect::new(2293, 8, 1139, 1424)]
        );
        assert_eq!(rects[1].right(), 3440 - 8);
    }

    #[test]
    fn the_separation_of_columns_is_the_gap_between_them() {
        let area = Rect::new(0, 0, 1920, 1080);

        assert_eq!(
            LayoutKind::Columns.separator(area, Ratio::HALF, 8),
            Some(Rect::new(956, 8, 8, 1064))
        );
        assert_eq!(
            LayoutKind::Rows.separator(area, Ratio::HALF, 8),
            Some(Rect::new(8, 536, 1904, 8))
        );
    }

    #[test]
    fn layouts_with_a_fixed_or_no_split_have_no_separation() {
        let area = Rect::new(0, 0, 3440, 1440);
        for kind in [
            LayoutKind::Full,
            LayoutKind::Free,
            LayoutKind::TwoThirdsOneThird,
        ] {
            assert_eq!(kind.separator(area, Ratio::HALF, 8), None, "{kind:?}");
            assert_eq!(
                kind.ratio_at(area, 8, Point::new(900, 700)),
                None,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn the_ratio_puts_the_separation_under_the_pointer() {
        let area = Rect::new(0, 0, 1920, 1080);

        assert_eq!(
            LayoutKind::Columns.ratio_at(area, 8, Point::new(1112, 500)),
            Some(Ratio::from_percent(58))
        );
        assert_eq!(
            LayoutKind::Rows.ratio_at(area, 8, Point::new(900, 540)),
            Some(Ratio::HALF)
        );
    }

    #[test]
    fn the_ratio_under_the_pointer_stays_within_bounds() {
        let area = Rect::new(52, 0, 1868, 1080);

        assert_eq!(
            LayoutKind::Columns.ratio_at(area, 8, Point::new(0, 500)),
            Some(Ratio::MIN)
        );
        assert_eq!(
            LayoutKind::Columns.ratio_at(area, 8, Point::new(5000, 500)),
            Some(Ratio::MAX)
        );
    }

    #[test]
    fn free_has_no_slot() {
        assert!(LayoutKind::Free.slot_rects(SIDE, Ratio::HALF, 8).is_empty());
    }

    #[test]
    fn a_zero_gap_fills_the_whole_area() {
        let area = Rect::new(0, 0, 100, 100).inset(Insets::uniform(10));

        assert_eq!(
            LayoutKind::Full.slot_rects(area, Ratio::HALF, 0),
            vec![Rect::new(10, 10, 80, 80)]
        );
    }
}
