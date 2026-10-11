//! The nested backend: the compositor runs in a window of the host session.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::ExportMem;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTarget};
use smithay::backend::winit::{self, WinitEventLoop, WinitGraphicsBackend};
use smithay::desktop::space::SpaceElement;
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::winit::cursor::Cursor;
use smithay::reexports::winit::dpi::LogicalSize;
use smithay::reexports::winit::monitor::Fullscreen;
use smithay::reexports::winit::window::WindowAttributes;
use smithay::utils::{IsAlive, Logical, Physical, Point, Rectangle, Size};
use smithay::wayland::compositor::with_states;
use tracing::{info, warn};

use crate::render::scene;
use crate::render::shaders::Shaders;
use crate::state::State;

const TITLE: &str = "coquille";
const INITIAL_SIZE: (f64, f64) = (1600.0, 900.0);

pub struct Winit {
    pub backend: WinitGraphicsBackend<GlesRenderer>,
    pub damage_tracker: Option<OutputDamageTracker>,
    pub shaders: Shaders,
    /// Something changed: draw a frame at the next tick.
    pub redraw_needed: bool,
    /// Redraw everything on the next frame, whatever the damage.
    pub full_redraw: bool,
    /// The host window is still the renderer's current surface. Cleared by
    /// whoever uses the renderer outside of a frame.
    pub window_is_current: bool,
    /// Debug: save the next frame there, then quit.
    pub screenshot: Option<PathBuf>,
}

/// Opens the host window and prepares the renderer. In full screen, the
/// window covers the monitor it appears on, which makes the simulated output
/// as large as a real one.
pub fn init(fullscreen: bool) -> anyhow::Result<(Winit, WinitEventLoop)> {
    let attributes = WindowAttributes::default()
        .with_title(TITLE)
        .with_surface_size(LogicalSize::new(INITIAL_SIZE.0, INITIAL_SIZE.1))
        .with_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
    let (mut backend, events) = winit::init_from_attributes::<GlesRenderer>(attributes)
        .map_err(|error| anyhow::anyhow!("cannot open the host window: {error}"))?;
    let shaders = Shaders::compile(backend.renderer()).context("cannot compile the shaders")?;

    let winit = Winit {
        backend,
        damage_tracker: None,
        shaders,
        redraw_needed: true,
        full_redraw: true,
        window_is_current: false,
        screenshot: None,
    };
    Ok((winit, events))
}

impl State {
    /// A client (or the shell) changed the pointer image. Named cursors are
    /// drawn by the host; surface cursors are drawn by the compositor.
    pub fn set_cursor(&mut self, image: CursorImageStatus) {
        if self.cursor_status == image {
            return;
        }
        let window = self.backend.backend.window();
        match &image {
            CursorImageStatus::Named(icon) => {
                window.set_cursor(Cursor::Icon(*icon));
                window.set_cursor_visible(true);
            }
            CursorImageStatus::Hidden | CursorImageStatus::Surface(_) => {
                window.set_cursor_visible(false);
            }
        }
        self.cursor_status = image;
        self.queue_redraw();
    }

    /// The client's cursor surface and where to draw it.
    fn cursor_surface(&mut self) -> Option<(WlSurface, Point<i32, Logical>)> {
        let CursorImageStatus::Surface(surface) = &self.cursor_status else {
            return None;
        };
        if !surface.alive() {
            self.set_cursor(CursorImageStatus::default_named());
            return None;
        }
        let hotspot = with_states(surface, |states| {
            states
                .data_map
                .get::<std::sync::Mutex<CursorImageAttributes>>()
                .and_then(|attributes| attributes.lock().ok().map(|attributes| attributes.hotspot))
                .unwrap_or_default()
        });
        let location = self.seat.get_pointer()?.current_location().to_i32_round();
        Some((surface.clone(), location - hotspot))
    }

    /// Called at the output's refresh rate: draws if anything changed.
    ///
    /// Frames are paced by this tick rather than by the host's redraw
    /// requests, which stop coming once a frame brought no change.
    pub fn frame_tick(&mut self) {
        if std::mem::take(&mut self.backend.redraw_needed) {
            self.render();
        }
    }

    /// Draws a frame, then tells visible clients they may draw their next one.
    pub fn render(&mut self) {
        self.shell
            .refresh_chrome(&mut self.painter, &self.theme, self.hover);
        let cursor = self.cursor_surface();
        let screenshot = self.backend.screenshot.take();

        let output = self.output.clone();
        let winit = &mut self.backend;
        // The buffer age can only be asked while the window is current, which
        // it is from one frame to the next unless the renderer was used in
        // between. Without an age, everything is redrawn.
        let full_redraw = std::mem::take(&mut winit.full_redraw) || screenshot.is_some();
        let age = if full_redraw || !std::mem::replace(&mut winit.window_is_current, true) {
            0
        } else {
            winit.backend.buffer_age().unwrap_or(0)
        };
        let size = winit.backend.window_size();
        let damage_tracker = winit
            .damage_tracker
            .get_or_insert_with(|| OutputDamageTracker::from_output(&output));

        let damage = match winit.backend.bind() {
            Ok((renderer, mut framebuffer)) => {
                let elements = scene::build(
                    renderer,
                    &winit.shaders,
                    &mut self.shell,
                    &self.theme,
                    cursor,
                );
                let clear = self.theme.backdrop.premultiplied();
                let rendered =
                    damage_tracker.render_output(renderer, &mut framebuffer, age, &elements, clear);
                match rendered {
                    Ok(result) => {
                        let damage = result.damage.cloned();
                        if let Some(path) = &screenshot {
                            match save_screenshot(renderer, &framebuffer, size, path) {
                                Ok(()) => info!(path = %path.display(), "screenshot saved"),
                                Err(error) => warn!(%error, "cannot save the screenshot"),
                            }
                        }
                        damage
                    }
                    Err(error) => {
                        warn!(%error, "cannot render");
                        None
                    }
                }
            }
            Err(error) => {
                warn!(%error, "cannot bind the host window");
                None
            }
        };
        // Reading the frame back leaves the window not current; the
        // compositor quits right after a screenshot, so nothing is shown.
        if let Some(damage) = damage.filter(|_| screenshot.is_none()) {
            if let Err(error) = winit.backend.submit(Some(&damage)) {
                warn!(%error, "cannot present the frame");
            }
        }

        let time = self.start_time.elapsed();
        let throttle = Some(Duration::ZERO);
        for managed in &self.shell.windows {
            if managed.geometry.visible {
                managed
                    .window
                    .send_frame(&output, time, throttle, |_, _| Some(output.clone()));
                managed.window.refresh();
            }
        }
        if let CursorImageStatus::Surface(surface) = &self.cursor_status {
            send_frames_surface_tree(surface, &output, time, throttle, |_, _| {
                Some(output.clone())
            });
        }
        self.popups.cleanup();

        if screenshot.is_some() {
            self.loop_signal.stop();
        }
    }
}

/// Debug helper: writes the frame just drawn as a PNG.
fn save_screenshot(
    renderer: &mut GlesRenderer,
    framebuffer: &GlesTarget<'_>,
    size: Size<i32, Physical>,
    path: &Path,
) -> anyhow::Result<()> {
    let region = Rectangle::from_size((size.w, size.h).into());
    let mapping = renderer.copy_framebuffer(framebuffer, region, Fourcc::Abgr8888)?;
    let pixels = renderer.map_texture(&mapping)?;

    let (width, height) = (u32::try_from(size.w)?, u32::try_from(size.h)?);
    let stride = width as usize * 4;
    let mut image = tiny_skia::Pixmap::new(width, height).context("empty screenshot")?;
    // The framebuffer is read bottom row first.
    let rows = pixels.chunks_exact(stride).rev();
    for (target, source) in image.data_mut().chunks_exact_mut(stride).zip(rows) {
        target.copy_from_slice(source);
        // The window's alpha channel is meaningless: make the image opaque.
        target
            .iter_mut()
            .skip(3)
            .step_by(4)
            .for_each(|alpha| *alpha = 0xff);
    }
    image.save_png(path)?;
    Ok(())
}
