//! Resizing a floating window by its edges and corners. Pure geometry.

use coquille_model::{Point, Rect, Size};
use smithay::input::pointer::CursorIcon;

/// How far outside a frame its edge can still be grabbed.
const OUTSIDE: i32 = 6;
/// How far inside.
const INSIDE: i32 = 4;
/// How close to a corner, along an edge, a grab takes both edges.
const CORNER: i32 = 16;

/// The edges of a frame a resize moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    /// The edges grabbed at `point`: those the point is on, and the
    /// neighbouring one near a corner. `None` away from every edge.
    pub fn at(frame: Rect, point: Point) -> Option<Self> {
        let reach = Rect::new(
            frame.x - OUTSIDE,
            frame.y - OUTSIDE,
            frame.w + 2 * OUTSIDE,
            frame.h + 2 * OUTSIDE,
        );
        if !reach.contains(point) {
            return None;
        }
        let on_left = point.x < frame.x + INSIDE;
        let on_right = point.x >= frame.right() - INSIDE;
        let on_top = point.y < frame.y + INSIDE;
        let on_bottom = point.y >= frame.bottom() - INSIDE;
        let (sideways, upright) = (on_left || on_right, on_top || on_bottom);
        if !sideways && !upright {
            return None;
        }
        Some(Self {
            left: on_left || (upright && point.x < frame.x + CORNER),
            right: on_right || (upright && point.x >= frame.right() - CORNER),
            top: on_top || (sideways && point.y < frame.y + CORNER),
            bottom: on_bottom || (sideways && point.y >= frame.bottom() - CORNER),
        })
    }

    /// The pointer shape that shows what dragging these edges does.
    pub fn cursor(self) -> CursorIcon {
        match (self.left, self.right, self.top, self.bottom) {
            (true, _, true, _) => CursorIcon::NwResize,
            (_, true, true, _) => CursorIcon::NeResize,
            (true, _, _, true) => CursorIcon::SwResize,
            (_, true, _, true) => CursorIcon::SeResize,
            (true, ..) => CursorIcon::WResize,
            (_, true, ..) => CursorIcon::EResize,
            (_, _, true, _) => CursorIcon::NResize,
            (_, _, _, true) => CursorIcon::SResize,
            _ => CursorIcon::Default,
        }
    }

    /// `frame` after its grabbed edges moved by (`dx`, `dy`), never smaller
    /// than `min`: an edge stops where the frame would get too small.
    pub fn apply(self, frame: Rect, dx: i32, dy: i32, min: Size) -> Rect {
        let mut rect = frame;
        if self.left {
            // Moving right shrinks: no further than the minimum width.
            let dx = dx.min(frame.w - min.w);
            rect.x += dx;
            rect.w -= dx;
        } else if self.right {
            rect.w = (frame.w + dx).max(min.w);
        }
        if self.top {
            let dy = dy.min(frame.h - min.h);
            rect.y += dy;
            rect.h -= dy;
        } else if self.bottom {
            rect.h = (frame.h + dy).max(min.h);
        }
        rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Rect = Rect::new(100, 100, 400, 300);
    const MIN: Size = Size::new(160, 100);

    fn edges(left: bool, right: bool, top: bool, bottom: bool) -> Edges {
        Edges {
            left,
            right,
            top,
            bottom,
        }
    }

    #[test]
    fn an_edge_is_grabbed_just_outside_and_just_inside_the_frame() {
        let right = Some(edges(false, true, false, false));

        assert_eq!(Edges::at(FRAME, Point::new(503, 250)), right);
        assert_eq!(Edges::at(FRAME, Point::new(497, 250)), right);
        assert_eq!(
            Edges::at(FRAME, Point::new(300, 402)),
            Some(edges(false, false, false, true))
        );
    }

    #[test]
    fn nothing_is_grabbed_away_from_the_edges() {
        assert_eq!(Edges::at(FRAME, Point::new(300, 250)), None);
        assert_eq!(Edges::at(FRAME, Point::new(520, 250)), None);
        assert_eq!(Edges::at(FRAME, Point::new(300, 80)), None);
    }

    #[test]
    fn a_corner_takes_both_its_edges() {
        let top_left = Some(edges(true, false, true, false));

        assert_eq!(Edges::at(FRAME, Point::new(99, 99)), top_left);
        // On the top edge, close to the left corner.
        assert_eq!(Edges::at(FRAME, Point::new(110, 98)), top_left);
        assert_eq!(
            Edges::at(FRAME, Point::new(502, 390)),
            Some(edges(false, true, false, true))
        );
    }

    #[test]
    fn the_far_edges_follow_the_pointer() {
        let resized = edges(false, true, false, true).apply(FRAME, 50, -20, MIN);

        assert_eq!(resized, Rect::new(100, 100, 450, 280));
    }

    #[test]
    fn the_near_edges_move_the_corner_and_keep_the_opposite_side() {
        let resized = edges(true, false, true, false).apply(FRAME, 50, 30, MIN);

        assert_eq!(resized, Rect::new(150, 130, 350, 270));
        assert_eq!(resized.right(), FRAME.right());
        assert_eq!(resized.bottom(), FRAME.bottom());
    }

    #[test]
    fn an_edge_stops_at_the_minimum_size() {
        let from_left = edges(true, false, false, false).apply(FRAME, 390, 0, MIN);
        let from_bottom = edges(false, false, false, true).apply(FRAME, 0, -500, MIN);

        assert_eq!(from_left, Rect::new(340, 100, 160, 300));
        assert_eq!(from_bottom, Rect::new(100, 100, 400, 100));
    }

    #[test]
    fn the_pointer_shows_which_way_the_frame_stretches() {
        assert_eq!(
            edges(false, true, false, false).cursor(),
            CursorIcon::EResize
        );
        assert_eq!(
            edges(true, false, false, false).cursor(),
            CursorIcon::WResize
        );
        assert_eq!(
            edges(false, false, true, false).cursor(),
            CursorIcon::NResize
        );
        assert_eq!(
            edges(false, false, false, true).cursor(),
            CursorIcon::SResize
        );
        assert_eq!(
            edges(true, false, true, false).cursor(),
            CursorIcon::NwResize
        );
        assert_eq!(
            edges(false, true, true, false).cursor(),
            CursorIcon::NeResize
        );
        assert_eq!(
            edges(true, false, false, true).cursor(),
            CursorIcon::SwResize
        );
        assert_eq!(
            edges(false, true, false, true).cursor(),
            CursorIcon::SeResize
        );
    }
}
