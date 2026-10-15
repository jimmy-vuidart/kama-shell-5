//! Taking screenshots of a screen or an interactive region.

use std::path::PathBuf;

use anyhow::Context;
use coquille_model::{OutputId, Rect};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::memory::MemoryRenderBuffer;
use smithay::backend::renderer::gles::GlesRenderbuffer;
use smithay::backend::renderer::{Bind, ExportMem, Offscreen};
use smithay::utils::{Buffer, Physical, Rectangle, Size, Transform};
use tracing::{info, warn};

use crate::render::scene::{self, Layers};
use crate::state::State;

/// Finds or creates the folder where screenshots are stored.
fn screenshots_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_PICTURES_DIR") {
        let path = PathBuf::from(dir).join("Captures d’écran");
        if std::fs::create_dir_all(&path).is_ok() {
            return path;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        let primary = home.join("Images").join("Captures d’écran");
        if primary.exists() || std::fs::create_dir_all(&primary).is_ok() {
            return primary;
        }
        let fallback = home.join("Pictures").join("Screenshots");
        if fallback.exists() || std::fs::create_dir_all(&fallback).is_ok() {
            return fallback;
        }
        return home;
    }
    PathBuf::from("/tmp")
}

/// Generates a unique timestamped file path for the screenshot.
fn screenshot_path() -> PathBuf {
    let dir = screenshots_dir();
    let base = chrono::Local::now()
        .format("Capture d’écran du %Y-%m-%d %H-%M-%S")
        .to_string();
    let mut candidate = dir.join(format!("{base}.png"));
    let mut counter = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{base}_{counter}.png"));
        counter += 1;
    }
    candidate
}

/// Saves the pixmap as a PNG file and copies it to the clipboard if possible.
fn save_and_copy(state: &State, pixmap: &tiny_skia::Pixmap) -> anyhow::Result<PathBuf> {
    let path = screenshot_path();
    pixmap
        .save_png(&path)
        .context("cannot save the screenshot image")?;
    info!(path = %path.display(), "screenshot saved");

    // Copy to clipboard via wl-copy if available.
    match std::fs::File::open(&path) {
        Ok(file) => {
            let mut cmd = std::process::Command::new("wl-copy");
            cmd.arg("--type")
                .arg("image/png")
                .env("WAYLAND_DISPLAY", &state.socket_name)
                .stdin(file);
            match cmd.spawn() {
                Ok(child) => info!(pid = child.id(), "wl-copy started for screenshot"),
                Err(error) => warn!(%error, "cannot start wl-copy"),
            }
        }
        Err(error) => {
            warn!(%error, path = %path.display(), "cannot open screenshot file for copying");
        }
    }
    Ok(path)
}

/// Renders a screen offscreen into a pixmap, excluding the drawer and cursor.
pub fn capture_output(state: &mut State, output_id: OutputId) -> anyhow::Result<tiny_skia::Pixmap> {
    let screen_info = *state.shell.screen(output_id).context("screen not found")?;
    let (width, height) = (screen_info.rect.w, screen_info.rect.h);
    let buffer_size: Size<i32, Buffer> = (width, height).into();
    let physical_size: Size<i32, Physical> = (width, height).into();

    state.shell.refresh_chrome(
        &mut state.painter,
        &state.theme,
        state.hover,
        state.capsule_hover,
    );
    state.refresh_bar();

    // Filter out the drawer so it never appears in the screenshot.
    let all_panels = state.panels();
    let panels: Vec<_> = all_panels
        .into_iter()
        .filter(|panel| panel.surface.namespace() != "coquille-drawer")
        .collect();

    let wallpapers = state.active_wallpapers();
    let shaders = state.backend.shaders().clone();
    let renderer = state.backend.renderer();

    let layers = Layers {
        view: screen_info.rect,
        overlay: &[],
        panel: state.bar.as_ref().map(|bar| (&bar.buffer, bar.rect)),
        panels: &panels,
        cursor: None,
        wallpapers: &wallpapers,
        freeze: &[],
        glass: &state.glass,
        panel_glass: &mut state.panel_glass,
        bar_glass: &mut state.bar_glass,
        drop_glass: &mut state.drop_glass,
        bar_shadow: &mut state.bar_shadow,
    };
    let elements = scene::build(renderer, &shaders, &mut state.shell, &state.theme, layers);

    let mut offscreen: GlesRenderbuffer =
        Offscreen::<GlesRenderbuffer>::create_buffer(renderer, Fourcc::Abgr8888, buffer_size)
            .map_err(|error| anyhow::anyhow!("cannot allocate offscreen buffer: {error:?}"))?;
    let mut target = renderer
        .bind(&mut offscreen)
        .map_err(|error| anyhow::anyhow!("cannot bind offscreen buffer: {error:?}"))?;

    let mut tracker = OutputDamageTracker::new(physical_size, 1.0, Transform::Normal);
    let clear = state.theme.backdrop.premultiplied();
    tracker
        .render_output(renderer, &mut target, 0, &elements, clear)
        .map_err(|error| anyhow::anyhow!("cannot render offscreen frame: {error:?}"))?;

    let region = Rectangle::from_size(buffer_size);
    let mapping = renderer
        .copy_framebuffer(&target, region, Fourcc::Abgr8888)
        .map_err(|error| anyhow::anyhow!("cannot copy framebuffer: {error:?}"))?;
    let pixels = renderer
        .map_texture(&mapping)
        .map_err(|error| anyhow::anyhow!("cannot map texture: {error:?}"))?;

    let (w, h) = (u32::try_from(width)?, u32::try_from(height)?);
    let stride = w as usize * 4;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).context("empty screenshot")?;
    // In GlesRenderer with Transform::Normal, Wayland y=0 maps to OpenGL y=-1.0,
    // which glReadPixels reads first (y=0 in the framebuffer). The rows are
    // therefore already ordered from top to bottom.
    let rows = pixels.chunks_exact(stride);
    for (dest, src) in pixmap.data_mut().chunks_exact_mut(stride).zip(rows) {
        dest.copy_from_slice(src);
        dest.iter_mut()
            .skip(3)
            .step_by(4)
            .for_each(|alpha| *alpha = 0xff);
    }
    Ok(pixmap)
}

/// Captures the specified screen (or the one under the pointer) and saves it.
pub fn capture_screen(state: &mut State, output_id: Option<OutputId>) -> anyhow::Result<PathBuf> {
    let screen_id = output_id
        .or_else(|| state.pointer_screen().map(|screen| screen.id))
        .or_else(|| state.outputs.first().map(|s| s.id))
        .context("no screen to capture")?;
    let pixmap = capture_output(state, screen_id)?;
    save_and_copy(state, &pixmap)
}

/// Pre-captured screen buffer for interactive region selection.
pub struct ScreenFreeze {
    pub output_id: OutputId,
    pub rect: Rect,
    pub pixmap: tiny_skia::Pixmap,
    pub buffer: MemoryRenderBuffer,
}

pub struct ScreenshotFreeze {
    pub screens: Vec<ScreenFreeze>,
}

impl ScreenshotFreeze {
    pub fn buffers(&self) -> Vec<(MemoryRenderBuffer, Rect)> {
        self.screens
            .iter()
            .map(|s| (s.buffer.clone(), s.rect))
            .collect()
    }
}

/// Pre-captures all screens into offscreen pixmaps and textures.
pub fn freeze_screens(state: &mut State) -> ScreenshotFreeze {
    let screen_ids: Vec<_> = state.shell.screens.iter().map(|s| (s.id, s.rect)).collect();
    let mut screens = Vec::new();
    for (id, rect) in screen_ids {
        match capture_output(state, id) {
            Ok(pixmap) => {
                let buffer = MemoryRenderBuffer::from_slice(
                    pixmap.data(),
                    Fourcc::Abgr8888,
                    (pixmap.width() as i32, pixmap.height() as i32),
                    1,
                    Transform::Normal,
                    None,
                );
                screens.push(ScreenFreeze {
                    output_id: id,
                    rect,
                    pixmap,
                    buffer,
                });
            }
            Err(error) => {
                warn!(%error, ?id, "cannot pre-capture screen for region screenshot");
            }
        }
    }
    ScreenshotFreeze { screens }
}

/// Crops a global region from the pre-captured screen freeze and saves it.
pub fn save_frozen_region(
    state: &State,
    freeze: Option<&ScreenshotFreeze>,
    global_rect: Rect,
) -> anyhow::Result<PathBuf> {
    let Some(freeze) = freeze else {
        anyhow::bail!("no pre-captured screen freeze available");
    };
    if global_rect.w <= 0 || global_rect.h <= 0 {
        anyhow::bail!("selection region is empty");
    }

    let (w, h) = (global_rect.w as u32, global_rect.h as u32);
    let mut cropped = tiny_skia::Pixmap::new(w, h).context("cannot allocate cropped pixmap")?;
    cropped.fill(tiny_skia::Color::BLACK);

    let mut copied_any = false;
    for screen in &freeze.screens {
        if let Some(inter) = screen.rect.intersection(global_rect) {
            if inter.w > 0 && inter.h > 0 {
                let src_x = (inter.x - screen.rect.x) as u32;
                let src_y = (inter.y - screen.rect.y) as u32;
                let dst_x = (inter.x - global_rect.x) as u32;
                let dst_y = (inter.y - global_rect.y) as u32;
                let copy_w = (inter.w as u32).min(screen.pixmap.width().saturating_sub(src_x));
                let copy_h = (inter.h as u32).min(screen.pixmap.height().saturating_sub(src_y));
                if copy_w > 0 && copy_h > 0 {
                    copy_rect(
                        &screen.pixmap,
                        &mut cropped,
                        (src_x, src_y),
                        (dst_x, dst_y),
                        (copy_w, copy_h),
                    );
                    copied_any = true;
                }
            }
        }
    }

    if !copied_any {
        anyhow::bail!("selection region did not intersect any screen");
    }

    save_and_copy(state, &cropped)
}

fn copy_rect(
    src: &tiny_skia::Pixmap,
    dst: &mut tiny_skia::Pixmap,
    src_pos: (u32, u32),
    dst_pos: (u32, u32),
    size: (u32, u32),
) {
    let (src_x, src_y) = src_pos;
    let (dst_x, dst_y) = dst_pos;
    let (w, h) = size;
    let src_stride = src.width() as usize * 4;
    let dst_stride = dst.width() as usize * 4;
    let copy_stride = w as usize * 4;

    for row in 0..h {
        let src_start = ((src_y + row) as usize * src_stride) + (src_x as usize * 4);
        let dst_start = ((dst_y + row) as usize * dst_stride) + (dst_x as usize * 4);
        let src_slice = &src.data()[src_start..src_start + copy_stride];
        let dst_slice = &mut dst.data_mut()[dst_start..dst_start + copy_stride];
        dst_slice.copy_from_slice(src_slice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_file_name_is_valid() {
        let path = screenshot_path();
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("Capture d’écran du "));
        assert!(name.ends_with(".png"));
    }

    #[test]
    fn copy_rect_extracts_the_exact_region() {
        let mut original = tiny_skia::Pixmap::new(10, 10).unwrap();
        // Fill pixel at (2, 3) with red
        let index = (3 * 10 + 2) * 4;
        original.data_mut()[index] = 255; // R
        original.data_mut()[index + 1] = 0; // G
        original.data_mut()[index + 2] = 0; // B
        original.data_mut()[index + 3] = 255; // A

        let mut cropped = tiny_skia::Pixmap::new(4, 4).unwrap();
        copy_rect(&original, &mut cropped, (2, 3), (0, 0), (4, 4));
        assert_eq!(cropped.width(), 4);
        assert_eq!(cropped.height(), 4);
        // The top-left pixel (0, 0) of cropped must be the red pixel
        assert_eq!(cropped.data()[0], 255);
        assert_eq!(cropped.data()[1], 0);
        assert_eq!(cropped.data()[2], 0);
        assert_eq!(cropped.data()[3], 255);
    }
}
