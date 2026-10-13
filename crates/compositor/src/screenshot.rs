//! Taking screenshots of a screen or an interactive region.

use std::path::PathBuf;

use anyhow::Context;
use coquille_model::{OutputId, Rect};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderbuffer;
use smithay::backend::renderer::{Bind, ExportMem, Offscreen};
use smithay::utils::{Buffer, Physical, Rectangle, Size, Transform};
use tracing::info;

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
fn save_and_copy(pixmap: &tiny_skia::Pixmap) -> anyhow::Result<PathBuf> {
    let path = screenshot_path();
    pixmap
        .save_png(&path)
        .context("cannot save the screenshot image")?;
    info!(path = %path.display(), "screenshot saved");

    // Copy to clipboard if wl-copy is installed.
    if let Ok(file) = std::fs::File::open(&path) {
        let _ = std::process::Command::new("wl-copy")
            .arg("--type")
            .arg("image/png")
            .stdin(file)
            .spawn();
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
    let rows = pixels.chunks_exact(stride).rev();
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
    save_and_copy(&pixmap)
}

/// Captures a global region and crops it to the selection.
pub fn capture_region(state: &mut State, global_rect: Rect) -> anyhow::Result<PathBuf> {
    let screen = state
        .shell
        .screens
        .iter()
        .find(|screen| screen.rect.intersects(global_rect))
        .or_else(|| state.shell.screens.first())
        .context("no screen found for region")?;
    let screen_id = screen.id;
    let screen_rect = screen.rect;

    let pixmap = capture_output(state, screen_id)?;

    let intersection = screen_rect.intersection(global_rect).unwrap_or(global_rect);
    let local_x = (intersection.x - screen_rect.x).max(0) as u32;
    let local_y = (intersection.y - screen_rect.y).max(0) as u32;
    let crop_w = (intersection.w as u32).min(pixmap.width().saturating_sub(local_x));
    let crop_h = (intersection.h as u32).min(pixmap.height().saturating_sub(local_y));

    if crop_w == 0 || crop_h == 0 {
        anyhow::bail!("selection region is empty");
    }

    let cropped = crop_pixmap(&pixmap, local_x, local_y, crop_w, crop_h)?;
    save_and_copy(&cropped)
}

fn crop_pixmap(
    pixmap: &tiny_skia::Pixmap,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> anyhow::Result<tiny_skia::Pixmap> {
    if w == 0 || h == 0 {
        anyhow::bail!("selection region is empty");
    }
    let mut cropped = tiny_skia::Pixmap::new(w, h).context("empty cropped pixmap")?;
    let src_stride = pixmap.width() as usize * 4;
    let crop_stride = w as usize * 4;

    for row in 0..h {
        let src_start = ((y + row) as usize * src_stride) + (x as usize * 4);
        let src_slice = &pixmap.data()[src_start..src_start + crop_stride];
        let dest_start = row as usize * crop_stride;
        let dest_slice = &mut cropped.data_mut()[dest_start..dest_start + crop_stride];
        dest_slice.copy_from_slice(src_slice);
    }
    Ok(cropped)
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
    fn crop_pixmap_extracts_the_exact_region() {
        let mut original = tiny_skia::Pixmap::new(10, 10).unwrap();
        // Fill pixel at (2, 3) with red
        let index = (3 * 10 + 2) * 4;
        original.data_mut()[index] = 255; // R
        original.data_mut()[index + 1] = 0; // G
        original.data_mut()[index + 2] = 0; // B
        original.data_mut()[index + 3] = 255; // A

        let cropped = crop_pixmap(&original, 2, 3, 4, 4).unwrap();
        assert_eq!(cropped.width(), 4);
        assert_eq!(cropped.height(), 4);
        // The top-left pixel (0, 0) of cropped must be the red pixel
        assert_eq!(cropped.data()[0], 255);
        assert_eq!(cropped.data()[1], 0);
        assert_eq!(cropped.data()[2], 0);
        assert_eq!(cropped.data()[3], 255);
    }
}
