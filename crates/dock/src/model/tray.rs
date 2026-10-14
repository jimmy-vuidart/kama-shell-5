//! The system tray: applications showing their state, tooltips, and
//! menus through the StatusNotifierItem (SNI) and DBusMenu protocols.
//! Every placement rule and metric calculation lives here, tested.

use system_tray::item::{IconPixmap, StatusNotifierItem};
use system_tray::menu::TrayMenu;

pub const ICON_SIZE: f32 = 16.0;
pub const GAP: f32 = 3.0;

/// An icon image ready to be drawn by iced, with RGBA pixels.
#[derive(Clone, PartialEq, Eq)]
pub struct TrayPixmap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl std::fmt::Debug for TrayPixmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrayPixmap")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pixels", &format!("<length: {}>", self.pixels.len()))
            .finish()
    }
}

/// Converts the network-byte-order ARGB32 pixmap from the SNI spec to RGBA8.
pub fn parse_pixmap(pixmap: &IconPixmap) -> Option<TrayPixmap> {
    if pixmap.width <= 0 || pixmap.height <= 0 {
        return None;
    }
    let (width, height) = (pixmap.width as u32, pixmap.height as u32);
    let expected_len = (width * height * 4) as usize;
    if pixmap.pixels.len() != expected_len {
        return None;
    }

    let mut rgba = Vec::with_capacity(expected_len);
    for chunk in pixmap.pixels.chunks_exact(4) {
        let (a, r, g, b) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        rgba.extend_from_slice(&[r, g, b, a]);
    }

    Some(TrayPixmap {
        width,
        height,
        pixels: rgba,
    })
}

/// Selects the best pixmap for our tray icon size (16px).
/// Prefers sizes closest to 16px, or the smallest one >= 16px.
pub fn select_best_pixmap(pixmaps: &[IconPixmap]) -> Option<TrayPixmap> {
    if pixmaps.is_empty() {
        return None;
    }
    let mut best: Option<&IconPixmap> = None;
    let mut best_diff = i32::MAX;

    for pixmap in pixmaps {
        if pixmap.width <= 0 || pixmap.height <= 0 {
            continue;
        }
        let diff = (pixmap.width - 16).abs();
        if diff < best_diff {
            best_diff = diff;
            best = Some(pixmap);
        }
    }

    best.and_then(parse_pixmap)
}

/// A status notifier item from an application.
#[derive(Debug, Clone)]
pub struct TrayItem {
    /// Unique address on the session bus (e.g. `:1.42/StatusNotifierItem`).
    pub address: String,
    pub id: String,
    pub title: String,
    pub tooltip: Option<String>,
    pub icon_name: Option<String>,
    pub pixmap: Option<TrayPixmap>,
    pub menu_path: Option<String>,
    pub menu: Option<TrayMenu>,
}

impl PartialEq for TrayItem {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
            && self.id == other.id
            && self.title == other.title
            && self.tooltip == other.tooltip
            && self.icon_name == other.icon_name
            && self.pixmap == other.pixmap
            && self.menu_path == other.menu_path
            && self.menu.is_some() == other.menu.is_some()
    }
}

impl TrayItem {
    pub fn from_sni(address: String, sni: &StatusNotifierItem, menu: Option<TrayMenu>) -> Self {
        let id = sni.id.clone();
        let title = sni.title.clone().unwrap_or_else(|| id.clone());
        let tooltip = sni.tool_tip.as_ref().map(|tip| {
            if tip.description.is_empty() {
                tip.title.clone()
            } else if tip.title.is_empty() {
                tip.description.clone()
            } else {
                format!("{} · {}", tip.title, tip.description)
            }
        });

        let icon_name = sni
            .icon_name
            .clone()
            .filter(|name| !name.is_empty())
            .or_else(|| sni.overlay_icon_name.clone().filter(|s| !s.is_empty()))
            .or_else(|| sni.attention_icon_name.clone().filter(|s| !s.is_empty()));

        let pixmaps = sni.icon_pixmap.as_deref().unwrap_or_default();
        let pixmap = select_best_pixmap(pixmaps);

        Self {
            address,
            id,
            title,
            tooltip,
            icon_name,
            pixmap,
            menu_path: sni.menu.clone(),
            menu,
        }
    }
}

/// Number of columns required for `count` items arranged in 2 lines.
pub fn horizontal_columns(count: usize) -> usize {
    count.div_ceil(2)
}

/// Dimensions `(width, height)` of the horizontal tray for `count` items in 2 lines.
pub fn horizontal_size(count: usize) -> (f32, f32) {
    if count == 0 {
        return (0.0, 0.0);
    }
    let cols = horizontal_columns(count);
    let width = cols as f32 * ICON_SIZE + (cols.saturating_sub(1)) as f32 * GAP;
    let height = 2.0 * ICON_SIZE + GAP;
    (width, height)
}

/// Grid coordinate `(col, row)` of item `index` in the horizontal 2-line layout.
pub fn horizontal_coord(index: usize) -> (usize, usize) {
    (index / 2, index % 2)
}

/// Number of rows required for `count` items arranged in 2 columns.
pub fn vertical_rows(count: usize) -> usize {
    count.div_ceil(2)
}

/// Dimensions `(width, height)` of the vertical tray for `count` items in 2 columns.
pub fn vertical_size(count: usize) -> (f32, f32) {
    if count == 0 {
        return (0.0, 0.0);
    }
    let rows = vertical_rows(count);
    let width = 2.0 * ICON_SIZE + GAP;
    let height = rows as f32 * ICON_SIZE + (rows.saturating_sub(1)) as f32 * GAP;
    (width, height)
}

/// Grid coordinate `(col, row)` of item `index` in the vertical 2-column layout.
pub fn vertical_coord(index: usize) -> (usize, usize) {
    (index % 2, index / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_tray_metrics_in_two_lines() {
        assert_eq!(horizontal_columns(0), 0);
        assert_eq!(horizontal_columns(1), 1);
        assert_eq!(horizontal_columns(2), 1);
        assert_eq!(horizontal_columns(3), 2);
        assert_eq!(horizontal_columns(4), 2);
        assert_eq!(horizontal_columns(5), 3);

        assert_eq!(horizontal_size(0), (0.0, 0.0));
        assert_eq!(horizontal_size(1), (16.0, 35.0));
        assert_eq!(horizontal_size(2), (16.0, 35.0));
        assert_eq!(horizontal_size(3), (16.0 * 2.0 + 3.0, 35.0));
        assert_eq!(horizontal_size(4), (35.0, 35.0));

        assert_eq!(horizontal_coord(0), (0, 0));
        assert_eq!(horizontal_coord(1), (0, 1));
        assert_eq!(horizontal_coord(2), (1, 0));
        assert_eq!(horizontal_coord(3), (1, 1));
        assert_eq!(horizontal_coord(4), (2, 0));
    }

    #[test]
    fn vertical_tray_metrics_in_two_columns() {
        assert_eq!(vertical_rows(0), 0);
        assert_eq!(vertical_rows(1), 1);
        assert_eq!(vertical_rows(2), 1);
        assert_eq!(vertical_rows(3), 2);
        assert_eq!(vertical_rows(4), 2);
        assert_eq!(vertical_rows(5), 3);

        assert_eq!(vertical_size(0), (0.0, 0.0));
        assert_eq!(vertical_size(1), (35.0, 16.0));
        assert_eq!(vertical_size(2), (35.0, 16.0));
        assert_eq!(vertical_size(3), (35.0, 16.0 * 2.0 + 3.0));
        assert_eq!(vertical_size(4), (35.0, 35.0));

        assert_eq!(vertical_coord(0), (0, 0));
        assert_eq!(vertical_coord(1), (1, 0));
        assert_eq!(vertical_coord(2), (0, 1));
        assert_eq!(vertical_coord(3), (1, 1));
        assert_eq!(vertical_coord(4), (0, 2));
    }

    #[test]
    fn pixmap_argb_to_rgba_conversion() {
        // Pixel: A=255, R=10, G=20, B=30
        let pixmap = IconPixmap {
            width: 1,
            height: 1,
            pixels: vec![255, 10, 20, 30],
        };
        let parsed = parse_pixmap(&pixmap).expect("valid pixmap");
        assert_eq!(parsed.width, 1);
        assert_eq!(parsed.height, 1);
        assert_eq!(parsed.pixels, vec![10, 20, 30, 255]);
    }

    #[test]
    fn invalid_pixmap_rejected() {
        let pixmap = IconPixmap {
            width: 2,
            height: 2,
            pixels: vec![1, 2, 3], // Should be 2*2*4 = 16 bytes
        };
        assert!(parse_pixmap(&pixmap).is_none());
    }
}
