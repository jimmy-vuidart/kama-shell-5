//! Where the tiles of the window switcher (Alt+Tab) sit in its panel. Pure
//! geometry, shared by painting and placing.

use coquille_model::{Rect, Size};

/// Space between the edge of the panel and its tiles.
pub const PADDING: i32 = 12;
pub const GAP: i32 = 8;
pub const TILE: Size = Size::new(136, 124);
pub const ICON_SIZE: i32 = 48;
/// From the top of a tile to its icon.
pub const ICON_TOP: i32 = 18;
/// From the top of a tile to the line of the name of the application, then
/// of the title of the window.
pub const NAME_TOP: i32 = 74;
pub const TITLE_TOP: i32 = 94;
/// Space kept between a tile's edges and its texts.
pub const TEXT_INSET: i32 = 10;
/// Space kept between the panel and the edges of its screen.
const SCREEN_MARGIN: i32 = 48;

/// The panel of the switcher: its size, and the place of each tile in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherLayout {
    pub size: Size,
    pub tiles: Vec<Rect>,
}

impl SwitcherLayout {
    /// `count` tiles in rows of as many as a screen `width` wide holds.
    pub fn new(count: usize, width: i32) -> Self {
        let room = width - 2 * SCREEN_MARGIN - 2 * PADDING + GAP;
        let fit = usize::try_from(room / (TILE.w + GAP)).unwrap_or(0).max(1);
        let columns = count.clamp(1, fit);
        let rows = count.div_ceil(columns).max(1);
        // At most a few hundred tiles: the casts cannot truncate.
        let span = |cells: usize, cell: i32| 2 * PADDING + cells as i32 * (cell + GAP) - GAP;
        let tiles = (0..count)
            .map(|index| {
                let (column, row) = ((index % columns) as i32, (index / columns) as i32);
                Rect::new(
                    PADDING + column * (TILE.w + GAP),
                    PADDING + row * (TILE.h + GAP),
                    TILE.w,
                    TILE.h,
                )
            })
            .collect();
        Self {
            size: Size::new(span(columns, TILE.w), span(rows, TILE.h)),
            tiles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_few_windows_make_one_row() {
        let layout = SwitcherLayout::new(3, 3440);

        assert_eq!(layout.size, Size::new(12 + 3 * 136 + 2 * 8 + 12, 148));
        assert_eq!(layout.tiles[1], Rect::new(12 + 136 + 8, 12, 136, 124));
    }

    #[test]
    fn too_many_windows_for_the_screen_go_on_more_rows() {
        let layout = SwitcherLayout::new(15, 1920);

        // 1920 - 96 - 24 + 8 = 1808 holds 12 tiles of 144.
        assert_eq!(layout.tiles[11].y, 12);
        assert_eq!(layout.tiles[12], Rect::new(12, 12 + 124 + 8, 136, 124));
        assert_eq!(layout.size.h, 2 * 12 + 2 * 124 + 8);
        assert!(layout.size.w <= 1920 - 96);
    }

    #[test]
    fn a_narrow_screen_still_holds_one_tile_a_row() {
        let layout = SwitcherLayout::new(2, 100);

        assert_eq!(layout.tiles[1].x, 12);
        assert_eq!(layout.tiles[1].y, 12 + 124 + 8);
    }
}
