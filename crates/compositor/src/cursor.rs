//! Pointer images from the XCursor theme, for when no host draws them.

use std::collections::HashMap;
use std::fs;

use coquille_model::Scale;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::input::pointer::CursorIcon;
use smithay::utils::Transform;
use tracing::warn;
use xcursor::CursorTheme;
use xcursor::parser::{Image, parse_xcursor};

const DEFAULT_THEME: &str = "default";
const DEFAULT_SIZE: u32 = 24;

/// A pointer image ready to draw.
pub struct Sprite {
    pub buffer: MemoryRenderBuffer,
    /// The pixel of the image that sits at the pointer position.
    pub hotspot: (i32, i32),
    /// Its size, in pixels.
    pub pixels: (i32, i32),
}

pub struct CursorImages {
    theme: CursorTheme,
    /// In logical pixels: the images are read that many times the scale.
    size: u32,
    sprites: HashMap<(CursorIcon, Scale), Sprite>,
}

impl CursorImages {
    /// Uses `XCURSOR_THEME` and `XCURSOR_SIZE`, like every other program.
    pub fn load() -> Self {
        let name = std::env::var("XCURSOR_THEME").unwrap_or_else(|_| DEFAULT_THEME.to_owned());
        let size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|size| size.parse().ok())
            .unwrap_or(DEFAULT_SIZE);
        Self {
            theme: CursorTheme::load(&name),
            size,
            sprites: HashMap::new(),
        }
    }

    /// The image of `icon` for a screen of `scale`. A shape the theme lacks
    /// falls back to the default arrow, and a missing theme to a plain arrow
    /// drawn here.
    pub fn get(&mut self, icon: CursorIcon, scale: Scale) -> &Sprite {
        let key = (icon, scale);
        if !self.sprites.contains_key(&key) {
            let size = self.pixels(scale);
            let image = self
                .read(icon.name(), size)
                .or_else(|| self.read(CursorIcon::Default.name(), size))
                .or_else(|| self.read("left_ptr", size));
            let sprite = match image {
                Some(image) => sprite_from(&image),
                None => {
                    warn!(icon = icon.name(), "no cursor image in the theme");
                    fallback_arrow()
                }
            };
            self.sprites.insert(key, sprite);
        }
        &self.sprites[&key]
    }

    /// How many pixels a pointer image has on a screen of `scale`.
    pub fn pixels(&self, scale: Scale) -> u32 {
        (f64::from(self.size) * scale.get()).round() as u32
    }

    /// The default arrow as the theme has it, `size` pixels large, for those
    /// who draw it themselves (the X server).
    pub fn default_image(&self, size: u32) -> Option<Image> {
        self.read(CursorIcon::Default.name(), size)
            .or_else(|| self.read("left_ptr", size))
    }

    /// Reads the image of a shape at the size closest to the wanted one.
    /// Animated cursors show their first frame.
    fn read(&self, name: &str, size: u32) -> Option<Image> {
        let path = self.theme.load_icon(name)?;
        let images = parse_xcursor(&fs::read(path).ok()?)?;
        images
            .into_iter()
            .min_by_key(|image| image.size.abs_diff(size))
    }
}

fn sprite_from(image: &Image) -> Sprite {
    Sprite {
        buffer: MemoryRenderBuffer::from_slice(
            &image.pixels_rgba,
            Fourcc::Abgr8888,
            (image.width as i32, image.height as i32),
            1,
            Transform::Normal,
            None,
        ),
        hotspot: (image.xhot as i32, image.yhot as i32),
        pixels: (image.width as i32, image.height as i32),
    }
}

/// A white arrow with a dark outline, pointing at its top-left corner.
fn fallback_arrow() -> Sprite {
    use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform as Matrix};

    const SIZE: u32 = 24;
    let mut buffer = MemoryRenderBuffer::default();
    let arrow = Pixmap::new(SIZE, SIZE).and_then(|mut pixmap| {
        let mut path = PathBuilder::new();
        path.move_to(1.5, 1.5);
        path.line_to(1.5, 17.5);
        path.line_to(6.0, 13.5);
        path.line_to(9.0, 20.0);
        path.line_to(11.5, 19.0);
        path.line_to(8.5, 12.5);
        path.line_to(14.0, 12.5);
        path.close();
        let path = path.finish()?;

        let mut paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        paint.set_color(Color::WHITE);
        pixmap.fill_path(&path, &paint, FillRule::Winding, Matrix::identity(), None);
        paint.set_color(Color::BLACK);
        let outline = Stroke {
            width: 1.0,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &outline, Matrix::identity(), None);
        Some(pixmap)
    });
    if let Some(arrow) = arrow {
        buffer = MemoryRenderBuffer::from_slice(
            arrow.data(),
            Fourcc::Abgr8888,
            (SIZE as i32, SIZE as i32),
            1,
            Transform::Normal,
            None,
        );
    }
    Sprite {
        buffer,
        hotspot: (1, 1),
        pixels: (SIZE as i32, SIZE as i32),
    }
}
