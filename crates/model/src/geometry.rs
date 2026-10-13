//! Integer geometry in logical pixels.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Size {
    pub w: i32,
    pub h: i32,
}

impl Size {
    pub const fn new(w: i32, h: i32) -> Self {
        Self { w, h }
    }
}

/// Distances from each edge of a rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    pub const ZERO: Self = Self::uniform(0);

    pub const fn uniform(value: i32) -> Self {
        Self {
            left: value,
            top: value,
            right: value,
            bottom: value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn from_size(size: Size) -> Self {
        Self::new(0, 0, size.w, size.h)
    }

    pub const fn origin(self) -> Point {
        Point::new(self.x, self.y)
    }

    pub const fn size(self) -> Size {
        Size::new(self.w, self.h)
    }

    /// First column past the right edge.
    pub const fn right(self) -> i32 {
        self.x + self.w
    }

    /// First row past the bottom edge.
    pub const fn bottom(self) -> i32 {
        self.y + self.h
    }

    pub const fn is_empty(self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    /// Whether `point` is inside; the right and bottom edges are excluded.
    pub const fn contains(self, point: Point) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }

    /// The same rectangle, moved by `offset`.
    pub const fn translated(self, offset: Point) -> Self {
        Self::new(self.x + offset.x, self.y + offset.y, self.w, self.h)
    }

    /// Shrinks the rectangle by `insets`; the size never goes below zero.
    pub fn inset(self, insets: Insets) -> Self {
        Self {
            x: self.x + insets.left,
            y: self.y + insets.top,
            w: (self.w - insets.left - insets.right).max(0),
            h: (self.h - insets.top - insets.bottom).max(0),
        }
    }

    /// Splits into a left part `first` pixels wide and a right part, separated
    /// by `gap`. No pixel is lost: both widths plus the gap equal `self.w`.
    pub fn split_columns(self, first: i32, gap: i32) -> (Self, Self) {
        let first = first.clamp(0, (self.w - gap).max(0));
        let second = (self.w - gap - first).max(0);
        (
            Self::new(self.x, self.y, first, self.h),
            Self::new(self.right() - second, self.y, second, self.h),
        )
    }

    /// Splits into a top part `first` pixels high and a bottom part, separated
    /// by `gap`.
    pub fn split_rows(self, first: i32, gap: i32) -> (Self, Self) {
        let first = first.clamp(0, (self.h - gap).max(0));
        let second = (self.h - gap - first).max(0);
        (
            Self::new(self.x, self.y, self.w, first),
            Self::new(self.x, self.bottom() - second, self.w, second),
        )
    }

    /// A rectangle of `size` centered in `within`.
    pub fn centered(size: Size, within: Self) -> Self {
        Self::new(
            within.x + (within.w - size.w) / 2,
            within.y + (within.h - size.h) / 2,
            size.w,
            size.h,
        )
    }

    /// Shrinks then moves the rectangle so that it lies entirely in `bounds`.
    pub fn clamped_to(self, bounds: Self) -> Self {
        let w = self.w.min(bounds.w);
        let h = self.h.min(bounds.h);
        Self {
            x: self.x.clamp(bounds.x, bounds.right() - w),
            y: self.y.clamp(bounds.y, bounds.bottom() - h),
            w,
            h,
        }
    }

    /// Whether two rectangles overlap.
    pub const fn intersects(self, other: Self) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// The rectangle covered by both, if they overlap.
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right > x && bottom > y {
            Some(Self::new(x, y, right - x, bottom - y))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_excludes_the_right_and_bottom_edges() {
        let rect = Rect::new(10, 20, 100, 50);

        assert!(rect.contains(Point::new(10, 20)));
        assert!(rect.contains(Point::new(109, 69)));
        assert!(!rect.contains(Point::new(110, 69)));
        assert!(!rect.contains(Point::new(109, 70)));
        assert!(!rect.contains(Point::new(9, 20)));
    }

    #[test]
    fn translating_moves_the_origin_and_keeps_the_size() {
        let rect = Rect::new(8, 46, 948, 1026).translated(Point::new(1920, -10));

        assert_eq!(rect, Rect::new(1928, 36, 948, 1026));
    }

    #[test]
    fn inset_shrinks_each_side() {
        let rect = Rect::new(0, 0, 1920, 1080).inset(Insets {
            left: 52,
            top: 1,
            right: 2,
            bottom: 3,
        });

        assert_eq!(rect, Rect::new(52, 1, 1866, 1076));
    }

    #[test]
    fn inset_never_yields_a_negative_size() {
        let rect = Rect::new(0, 0, 10, 10).inset(Insets::uniform(8));

        assert_eq!(rect.size(), Size::new(0, 0));
    }

    #[test]
    fn split_columns_loses_no_pixel() {
        let (left, right) = Rect::new(60, 8, 1852, 1064).split_columns(922, 8);

        assert_eq!(left, Rect::new(60, 8, 922, 1064));
        assert_eq!(right, Rect::new(990, 8, 922, 1064));
        assert_eq!(left.w + 8 + right.w, 1852);
    }

    #[test]
    fn split_columns_keeps_both_parts_inside_when_first_is_too_large() {
        let (left, right) = Rect::new(0, 0, 100, 50).split_columns(500, 8);

        assert_eq!(left, Rect::new(0, 0, 92, 50));
        assert_eq!(right, Rect::new(100, 0, 0, 50));
    }

    #[test]
    fn split_rows_loses_no_pixel() {
        let (top, bottom) = Rect::new(8, 8, 1904, 1064).split_rows(528, 8);

        assert_eq!(top, Rect::new(8, 8, 1904, 528));
        assert_eq!(bottom, Rect::new(8, 544, 1904, 528));
    }

    #[test]
    fn centered_places_the_size_in_the_middle() {
        let rect = Rect::centered(Size::new(800, 600), Rect::new(52, 0, 1868, 1080));

        assert_eq!(rect, Rect::new(586, 240, 800, 600));
    }

    #[test]
    fn clamped_to_moves_a_rect_that_sticks_out() {
        let bounds = Rect::new(0, 0, 1920, 1080);

        assert_eq!(
            Rect::new(1800, -40, 400, 300).clamped_to(bounds),
            Rect::new(1520, 0, 400, 300)
        );
    }

    #[test]
    fn clamped_to_shrinks_a_rect_larger_than_the_bounds() {
        let bounds = Rect::new(10, 10, 500, 400);

        assert_eq!(Rect::new(0, 0, 900, 900).clamped_to(bounds), bounds);
    }

    #[test]
    fn rect_intersection_and_intersects() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(50, 50, 100, 100);
        let c = Rect::new(200, 200, 50, 50);

        assert!(a.intersects(b));
        assert!(b.intersects(a));
        assert!(!a.intersects(c));
        assert_eq!(a.intersection(b), Some(Rect::new(50, 50, 50, 50)));
        assert_eq!(a.intersection(c), None);
    }
}
