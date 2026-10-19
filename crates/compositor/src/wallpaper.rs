//! Loading, scaling, and caching screen wallpapers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use coquille_model::{OutputId, OutputRole, Rect, Scale, Size};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use tiny_skia::{Color, FilterQuality, IntSize, Pixmap, PixmapPaint, Transform};
use tracing::warn;

use crate::config::{WallpaperConfig, WallpaperDefinition, WallpaperMode};
use crate::theme::{Rgba, ThemeKind};

/// The rendered wallpaper of one screen.
pub struct ScreenWallpaper {
    pub output_id: OutputId,
    pub buffer: MemoryRenderBuffer,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    source: Option<PathBuf>,
    mode: WallpaperMode,
    size: (i32, i32),
    backdrop: (u8, u8, u8),
}

/// Manages and caches wallpaper textures for each output.
#[derive(Default)]
pub struct WallpaperManager {
    cache: HashMap<OutputId, (CacheKey, MemoryRenderBuffer)>,
}

impl WallpaperManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates the wallpapers for active screens, reusing cached textures
    /// when settings and dimensions have not changed.
    pub fn update(
        &mut self,
        screens: &[(OutputId, OutputRole, Rect, Scale)],
        config: &WallpaperConfig,
        kind: ThemeKind,
        backdrop: Rgba,
    ) -> Vec<ScreenWallpaper> {
        let active_ids: Vec<OutputId> = screens.iter().map(|(id, ..)| *id).collect();
        self.cache.retain(|id, _| active_ids.contains(id));

        let mut result = Vec::new();
        for &(output_id, role, rect, scale) in screens {
            // As many pixels as the screen has: the image is never enlarged.
            let pixels = scale.physical(rect.size());
            let size = (pixels.w, pixels.h);
            let path = resolve_wallpaper_path(config, role, pixels, kind);
            let mode = config.mode();
            let key = CacheKey {
                source: path.clone(),
                mode,
                size,
                backdrop: (backdrop.r, backdrop.g, backdrop.b),
            };

            let cached = self
                .cache
                .get(&output_id)
                .filter(|(cached_key, _)| *cached_key == key)
                .map(|(_, buffer)| buffer.clone());

            let buffer = match cached {
                Some(buf) => buf,
                None => {
                    let pixmap = path
                        .as_ref()
                        .and_then(|p| load_and_scale(p, mode, size, backdrop));
                    let Some(pixmap) = pixmap else {
                        self.cache.remove(&output_id);
                        continue;
                    };
                    let buf = texture_from_pixmap(&pixmap);
                    self.cache.insert(output_id, (key, buf.clone()));
                    buf
                }
            };

            result.push(ScreenWallpaper {
                output_id,
                buffer,
                rect,
            });
        }
        result
    }
}

/// Converts a tiny-skia Pixmap into a Smithay MemoryRenderBuffer.
fn texture_from_pixmap(pixmap: &Pixmap) -> MemoryRenderBuffer {
    let size = (pixmap.width() as i32, pixmap.height() as i32);
    MemoryRenderBuffer::from_slice(
        pixmap.data(),
        Fourcc::Abgr8888,
        size,
        1,
        smithay::utils::Transform::Normal,
        None,
    )
}

/// Resolves the file path of the wallpaper to use for a given screen.
pub fn resolve_wallpaper_path(
    config: &WallpaperConfig,
    role: OutputRole,
    screen_size: Size,
    kind: ThemeKind,
) -> Option<PathBuf> {
    // 1. Role-specific override
    let configured_screen = match role {
        OutputRole::Left => config.left.as_deref(),
        OutputRole::Center => config.center.as_deref(),
        OutputRole::Right => config.right.as_deref(),
    };

    if let Some(path_str) = configured_screen {
        let path = expand_path(path_str);
        if path.is_file() {
            return Some(path);
        }
        warn!(path = %path.display(), ?role, "specified wallpaper not found");
    }

    // 2. Global path
    if let Some(path_str) = config.path.as_deref() {
        let path = expand_path(path_str);
        if path.is_file() {
            return Some(path);
        }
        warn!(path = %path.display(), "global wallpaper not found");
    }

    // 3. Built-in / default glass wallpaper, at night in the dark theme
    resolve_default_wallpaper(screen_size, kind, config.definition())
}

/// Expands leading `~` to the user's home directory.
fn expand_path(path_str: &str) -> PathBuf {
    if let Some(stripped) = path_str.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(stripped);
        }
    }
    PathBuf::from(path_str)
}

/// Looks for default wallpapers in standard asset and data directories:
/// `glass-dark-*` in the dark theme, falling back on the light `glass-*`.
fn resolve_default_wallpaper(
    screen_size: Size,
    kind: ThemeKind,
    definition: WallpaperDefinition,
) -> Option<PathBuf> {
    let dirs = wallpaper_search_dirs();
    let prefixes: &[&str] = match kind {
        ThemeKind::Dark => &["glass-dark-", "glass-"],
        ThemeKind::Light => &["glass-"],
    };
    let sizes = glass_sizes(screen_size, definition);
    prefixes
        .iter()
        .find_map(|prefix| find_glass(&dirs, prefix, &sizes))
}

/// The sizes of the glass wallpaper that suit a screen of `screen_size`
/// pixels, best first. A 21:9 screen has a single one; a 16:9 screen takes
/// the 1080p or 4K version, as chosen or as its definition asks.
fn glass_sizes(screen_size: Size, definition: WallpaperDefinition) -> Vec<(i32, i32)> {
    const HD: (i32, i32) = (1920, 1080);
    const UHD: (i32, i32) = (3840, 2160);
    const WIDE: (i32, i32) = (3440, 1440);
    let exact = (screen_size.w, screen_size.h);
    // Wide (21:9 or wider) vs standard (16:9)
    let mut sizes = if screen_size.w as f32 / screen_size.h as f32 >= 2.0 {
        vec![exact, WIDE]
    } else {
        match definition {
            WallpaperDefinition::Hd => vec![HD, UHD],
            WallpaperDefinition::Uhd => vec![UHD, HD],
            WallpaperDefinition::Auto => {
                // Never enlarged when a bigger one exists.
                let larger = screen_size.w > HD.0 || screen_size.h > HD.1;
                let (first, second) = if larger { (UHD, HD) } else { (HD, UHD) };
                vec![exact, first, second]
            }
        }
    };
    // The screen's own size may be one of the others, right after it.
    sizes.dedup();
    sizes
}

/// The first glass wallpaper named `<prefix><width>x<height>.png` found among
/// `sizes`, or any of that prefix.
fn find_glass(dirs: &[PathBuf], prefix: &str, sizes: &[(i32, i32)]) -> Option<PathBuf> {
    for (w, h) in sizes {
        let name = format!("{prefix}{w}x{h}.png");
        for dir in dirs {
            let path = dir.join(&name);
            if path.is_file() {
                return Some(path);
            }
        }
    }

    // Any available glass wallpaper of this kind: a size follows the prefix
    let is_glass = |name: &str| {
        name.strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
            && name.ends_with(".png")
    };
    for dir in dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(is_glass)
                {
                    return Some(path);
                }
            }
        }
    }

    None
}

/// Known directories to check for wallpapers.
fn wallpaper_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // 1. Current working directory: assets/wallpapers
    let local = PathBuf::from("assets/wallpapers");
    if local.is_dir() {
        dirs.push(local);
    }

    // 2. Compile-time repository assets (for tests and local development)
    let manifest_assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/wallpapers");
    if manifest_assets.is_dir() && !dirs.contains(&manifest_assets) {
        dirs.push(manifest_assets);
    }

    // 3. Relative to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            // target/debug/coquille -> ../../assets/wallpapers
            let dev = bin_dir.join("../../assets/wallpapers");
            if dev.is_dir() && !dirs.contains(&dev) {
                dirs.push(dev);
            }
            // ~/.local/bin/coquille -> ../share/coquille/wallpapers
            let installed = bin_dir.join("../share/coquille/wallpapers");
            if installed.is_dir() && !dirs.contains(&installed) {
                dirs.push(installed);
            }
        }
    }

    // 3. User data dir: $XDG_DATA_HOME/coquille/wallpapers
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        let xdg = PathBuf::from(data_home).join("coquille/wallpapers");
        if xdg.is_dir() && !dirs.contains(&xdg) {
            dirs.push(xdg);
        }
    } else if let Some(home) = std::env::var_os("HOME") {
        let user_share = PathBuf::from(home).join(".local/share/coquille/wallpapers");
        if user_share.is_dir() && !dirs.contains(&user_share) {
            dirs.push(user_share);
        }
    }

    // 4. System data dir: /usr/share/coquille/wallpapers
    let sys = PathBuf::from("/usr/share/coquille/wallpapers");
    if sys.is_dir() && !dirs.contains(&sys) {
        dirs.push(sys);
    }

    dirs
}

/// Loads an image file (PNG or JPEG) and scales it according to `mode`.
fn load_and_scale(
    path: &Path,
    mode: WallpaperMode,
    (tw, th): (i32, i32),
    backdrop: Rgba,
) -> Option<Pixmap> {
    let src = load_image(path)?;
    scale_pixmap(&src, mode, tw as u32, th as u32, backdrop)
}

/// Loads an image into a premultiplied Pixmap.
fn load_image(path: &Path) -> Option<Pixmap> {
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(error) => {
            warn!(path = %path.display(), %error, "cannot read wallpaper file");
            return None;
        }
    };

    // Fast-path: PNG decoded by tiny-skia
    if let Ok(pixmap) = Pixmap::decode_png(&data) {
        return Some(pixmap);
    }

    // Fallback: image crate (supports JPEG, etc.)
    let img = match image::load_from_memory(&data) {
        Ok(img) => img,
        Err(error) => {
            warn!(path = %path.display(), %error, "cannot decode wallpaper image");
            return None;
        }
    };

    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let int_size = IntSize::from_wh(w, h)?;
    let mut raw = rgba.into_raw();

    // Premultiply alpha
    for chunk in raw.chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        if a < 255 {
            chunk[0] = ((chunk[0] as u32 * a + 127) / 255) as u8;
            chunk[1] = ((chunk[1] as u32 * a + 127) / 255) as u8;
            chunk[2] = ((chunk[2] as u32 * a + 127) / 255) as u8;
        }
    }

    Pixmap::from_vec(raw, int_size)
}

/// Scales and positions `src` into a new `Pixmap` of size `(tw, th)`.
pub fn scale_pixmap(
    src: &Pixmap,
    mode: WallpaperMode,
    tw: u32,
    th: u32,
    backdrop: Rgba,
) -> Option<Pixmap> {
    let mut target = Pixmap::new(tw, th)?;
    let bg_color = Color::from_rgba8(backdrop.r, backdrop.g, backdrop.b, 255);
    let (sw, sh) = (src.width(), src.height());

    let paint = PixmapPaint {
        quality: FilterQuality::Bicubic,
        ..PixmapPaint::default()
    };

    match mode {
        WallpaperMode::Fill => {
            let scale = (tw as f32 / sw as f32).max(th as f32 / sh as f32);
            let scaled_w = sw as f32 * scale;
            let scaled_h = sh as f32 * scale;
            let ox = (tw as f32 - scaled_w) / 2.0;
            let oy = (th as f32 - scaled_h) / 2.0;
            let place = Transform::from_scale(scale, scale).post_translate(ox, oy);
            target.draw_pixmap(0, 0, src.as_ref(), &paint, place, None);
        }
        WallpaperMode::Fit => {
            target.fill(bg_color);
            let scale = (tw as f32 / sw as f32).min(th as f32 / sh as f32);
            let scaled_w = sw as f32 * scale;
            let scaled_h = sh as f32 * scale;
            let ox = (tw as f32 - scaled_w) / 2.0;
            let oy = (th as f32 - scaled_h) / 2.0;
            let place = Transform::from_scale(scale, scale).post_translate(ox, oy);
            target.draw_pixmap(0, 0, src.as_ref(), &paint, place, None);
        }
        WallpaperMode::Center => {
            target.fill(bg_color);
            let ox = (tw as f32 - sw as f32) / 2.0;
            let oy = (th as f32 - sh as f32) / 2.0;
            let place = Transform::from_translate(ox, oy);
            target.draw_pixmap(0, 0, src.as_ref(), &paint, place, None);
        }
        WallpaperMode::Tile => {
            let mut y = 0;
            while y < th {
                let mut x = 0;
                while x < tw {
                    target.draw_pixmap(
                        x as i32,
                        y as i32,
                        src.as_ref(),
                        &paint,
                        Transform::identity(),
                        None,
                    );
                    x += sw;
                }
                y += sh;
            }
        }
    }

    Some(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaling_fill_covers_target() {
        let src = Pixmap::new(100, 50).unwrap();
        let scaled =
            scale_pixmap(&src, WallpaperMode::Fill, 200, 200, Rgba::rgb(0x13_11_0d)).unwrap();
        assert_eq!(scaled.width(), 200);
        assert_eq!(scaled.height(), 200);
    }

    #[test]
    fn scaling_fit_preserves_dimensions() {
        let src = Pixmap::new(100, 50).unwrap();
        let scaled =
            scale_pixmap(&src, WallpaperMode::Fit, 200, 200, Rgba::rgb(0x13_11_0d)).unwrap();
        assert_eq!(scaled.width(), 200);
        assert_eq!(scaled.height(), 200);
    }

    #[test]
    fn scaling_center_preserves_dimensions() {
        let src = Pixmap::new(100, 50).unwrap();
        let scaled =
            scale_pixmap(&src, WallpaperMode::Center, 200, 200, Rgba::rgb(0x13_11_0d)).unwrap();
        assert_eq!(scaled.width(), 200);
        assert_eq!(scaled.height(), 200);
    }

    #[test]
    fn scaling_tile_preserves_dimensions() {
        let src = Pixmap::new(50, 50).unwrap();
        let scaled =
            scale_pixmap(&src, WallpaperMode::Tile, 150, 150, Rgba::rgb(0x13_11_0d)).unwrap();
        assert_eq!(scaled.width(), 150);
        assert_eq!(scaled.height(), 150);
    }

    #[test]
    fn default_wallpaper_finds_glass_in_assets() {
        // Run from workspace root
        let resolved = resolve_default_wallpaper(
            Size::new(1920, 1080),
            ThemeKind::Light,
            WallpaperDefinition::Auto,
        );
        assert!(resolved.is_some());
        let path = resolved.unwrap();
        assert!(path.to_str().unwrap().contains("glass-1920x1080.png"));
    }

    #[test]
    fn dark_theme_finds_the_night_glass() {
        let auto = WallpaperDefinition::Auto;
        let path = resolve_default_wallpaper(Size::new(3440, 1440), ThemeKind::Dark, auto).unwrap();
        assert!(path.to_str().unwrap().ends_with("glass-dark-3440x1440.png"));
        let path = resolve_default_wallpaper(Size::new(1920, 1080), ThemeKind::Dark, auto).unwrap();
        assert!(path.to_str().unwrap().ends_with("glass-dark-1920x1080.png"));
        let path =
            resolve_default_wallpaper(Size::new(3840, 2160), ThemeKind::Light, auto).unwrap();
        assert!(path.to_str().unwrap().ends_with("glass-3840x2160.png"));
    }

    #[test]
    fn a_sixteen_by_nine_screen_takes_the_chosen_definition() {
        use WallpaperDefinition::{Auto, Hd, Uhd};
        let hd = Size::new(1920, 1080);
        let uhd = Size::new(3840, 2160);
        // Automatic: the screen's own size, else never enlarged.
        assert_eq!(glass_sizes(hd, Auto), vec![(1920, 1080), (3840, 2160)]);
        assert_eq!(glass_sizes(Size::new(2560, 1600), Auto)[1], (3840, 2160));
        assert_eq!(glass_sizes(Size::new(1366, 768), Auto)[1], (1920, 1080));
        // Chosen: whatever the screen.
        assert_eq!(glass_sizes(uhd, Hd), vec![(1920, 1080), (3840, 2160)]);
        assert_eq!(glass_sizes(hd, Uhd), vec![(3840, 2160), (1920, 1080)]);
        // A 21:9 screen has a single version.
        assert_eq!(glass_sizes(Size::new(3440, 1440), Uhd), vec![(3440, 1440)]);

        let path = resolve_default_wallpaper(uhd, ThemeKind::Dark, Hd).unwrap();
        assert!(path.to_str().unwrap().ends_with("glass-dark-1920x1080.png"));
        let path = resolve_default_wallpaper(hd, ThemeKind::Light, Uhd).unwrap();
        assert!(path.to_str().unwrap().ends_with("glass-3840x2160.png"));
    }
}
