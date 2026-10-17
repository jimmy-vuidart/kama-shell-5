//! The icons of applications in title bars: found in the icon theme of the
//! shell, drawn once at the size a title bar needs, and kept until the theme
//! or the accent changes.

use std::collections::HashMap;
use std::path::Path;

use tiny_skia::{FilterQuality, IntSize, Pixmap, PixmapPaint, Transform};

use crate::theme::{Rgba, ThemeKind};

/// Icons already drawn, by name and size. A name that led nowhere is kept
/// too, so that it is not looked up again for each frame.
#[derive(Default)]
pub struct IconCache {
    /// The theme and the accent the icon theme was chosen for, and its name.
    theme: Option<((ThemeKind, Rgba), String)>,
    drawn: HashMap<(String, u32), Option<Pixmap>>,
}

impl IconCache {
    /// Takes the icon theme that goes with a theme and an accent. When it is
    /// another one, the icons drawn so far are forgotten.
    pub fn follow(&mut self, kind: ThemeKind, accent: Rgba) {
        let style = (kind, accent);
        if self
            .theme
            .as_ref()
            .is_some_and(|(chosen, _)| *chosen == style)
        {
            return;
        }
        let name = coquille_apps::icon_theme_for(kind.into(), accent);
        if self.theme.as_ref().is_none_or(|(_, used)| *used != name) {
            tracing::info!(theme = name, "icon theme");
            self.drawn.clear();
        }
        self.theme = Some((style, name));
    }

    /// The icon `name` (as a desktop entry names it) as a square of `size`
    /// pixels; `None` when the theme has none, or not in a format that can
    /// be drawn. The theme is the one `follow` took.
    pub fn get(&mut self, name: &str, size: u32) -> Option<&Pixmap> {
        let key = (name.to_owned(), size);
        if !self.drawn.contains_key(&key) {
            let theme = self.theme.as_ref().map_or("hicolor", |(_, name)| name);
            let icon =
                coquille_apps::find_icon(name, theme).and_then(|path| rasterize(&path, size));
            if icon.is_none() {
                tracing::debug!(name, "no icon to draw for this application");
            }
            self.drawn.insert(key.clone(), icon);
        }
        self.drawn.get(&key)?.as_ref()
    }
}

/// Draws the image in a file as a square of `size` pixels.
pub fn rasterize(path: &Path, size: u32) -> Option<Pixmap> {
    let data = std::fs::read(path).ok()?;
    let is_svg = path.extension().is_some_and(|extension| extension == "svg");
    if is_svg {
        rasterize_svg(&data, size)
    } else {
        rasterize_png(&data, size)
    }
}

/// A vector icon, drawn at exactly the size wanted.
fn rasterize_svg(data: &[u8], size: u32) -> Option<Pixmap> {
    use resvg::{tiny_skia as svg_skia, usvg};

    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
    let drawing = tree.size();
    let scale = size as f32 / drawing.width().max(drawing.height());
    // Centered, should the drawing not be square.
    let offset = |length: f32| (size as f32 - length * scale) / 2.0;
    let place = svg_skia::Transform::from_scale(scale, scale)
        .post_translate(offset(drawing.width()), offset(drawing.height()));
    let mut pixmap = svg_skia::Pixmap::new(size, size)?;
    resvg::render(&tree, place, &mut pixmap.as_mut());
    // The renderer has its own version of the drawing library; pixels are
    // laid out the same way in both.
    Pixmap::from_vec(pixmap.take(), IntSize::from_wh(size, size)?)
}

/// A bitmap icon, scaled to the size wanted.
fn rasterize_png(data: &[u8], size: u32) -> Option<Pixmap> {
    let image = Pixmap::decode_png(data).ok()?;
    let mut pixmap = Pixmap::new(size, size)?;
    let scale = size as f32 / image.width().max(image.height()) as f32;
    let offset = |length: u32| (size as f32 - length as f32 * scale) / 2.0;
    let place = Transform::from_scale(scale, scale)
        .post_translate(offset(image.width()), offset(image.height()));
    let paint = PixmapPaint {
        quality: FilterQuality::Bicubic,
        ..PixmapPaint::default()
    };
    pixmap.draw_pixmap(0, 0, image.as_ref(), &paint, place, None);
    Some(pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How opaque the pixel at a place is.
    fn alpha(pixmap: &Pixmap, x: u32, y: u32) -> u8 {
        pixmap.pixel(x, y).map_or(0, |pixel| pixel.alpha())
    }

    #[test]
    fn a_vector_icon_is_drawn_at_the_size_asked() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48">
            <rect x="0" y="0" width="48" height="24" fill="#ff0000"/></svg>"##;

        let icon = rasterize_svg(svg, 18).unwrap();

        assert_eq!((icon.width(), icon.height()), (18, 18));
        // The upper half is painted, the lower one is not.
        assert_eq!(alpha(&icon, 9, 4), 255);
        assert_eq!(alpha(&icon, 9, 14), 0);
    }

    #[test]
    fn a_drawing_that_is_not_square_is_centered() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20">
            <rect width="40" height="20" fill="#00ff00"/></svg>"##;

        let icon = rasterize_svg(svg, 20).unwrap();

        assert_eq!(alpha(&icon, 10, 10), 255);
        assert_eq!(alpha(&icon, 10, 1), 0);
        assert_eq!(alpha(&icon, 10, 18), 0);
    }

    #[test]
    fn a_bitmap_icon_is_scaled_to_the_size_asked() {
        let mut large = Pixmap::new(64, 64).unwrap();
        large.fill(tiny_skia::Color::from_rgba8(0, 0, 255, 255));
        let png = large.encode_png().unwrap();

        let icon = rasterize_png(&png, 14).unwrap();

        assert_eq!((icon.width(), icon.height()), (14, 14));
        assert_eq!(alpha(&icon, 7, 7), 255);
    }

    #[test]
    fn a_file_is_drawn_according_to_its_kind() {
        let directory = std::env::temp_dir();
        let svg = directory.join(format!("coquille-icon-{}.svg", std::process::id()));
        std::fs::write(&svg, br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8"/></svg>"#).unwrap();

        let icon = rasterize(&svg, 16);
        std::fs::remove_file(&svg).unwrap();

        assert_eq!(icon.map(|icon| icon.width()), Some(16));
    }

    #[test]
    fn what_is_not_an_image_gives_no_icon() {
        assert!(rasterize_svg(b"not an image", 18).is_none());
        assert!(rasterize_png(b"not an image", 18).is_none());
        assert!(rasterize(Path::new("/nonexistent/icon.png"), 18).is_none());
    }
}
