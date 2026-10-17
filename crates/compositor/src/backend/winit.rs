//! The nested backend: the compositor runs in a window of the host session,
//! which stands for one screen or for the three of them side by side.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use coquille_model::{OutputId, OutputRole, Rect, Size as ModelSize};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::ExportMem;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTarget};
use smithay::backend::winit::{self, WinitEventLoop, WinitGraphicsBackend};
use smithay::desktop::space::SpaceElement;
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::input::pointer::CursorImageStatus;
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::winit::cursor::Cursor as HostCursor;
use smithay::reexports::winit::dpi::LogicalSize;
use smithay::reexports::winit::monitor::Fullscreen;
use smithay::reexports::winit::window::WindowAttributes;
use smithay::utils::{Physical, Rectangle, Size, Transform};
use tracing::{info, warn};

use super::Backend;
use crate::outputs::split_canvas;
use crate::render::scene::{self, Layers};
use crate::render::shaders::Shaders;
use crate::shell::ScreenInfo;
use crate::state::State;

const TITLE: &str = "coquille";
const INITIAL_SIZE: (f64, f64) = (1600.0, 900.0);
/// Refresh rate announced for the simulated screens, in mHz.
pub const REFRESH: i32 = 60_000;

/// What the nested window stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Simulated {
    /// A single screen with this role.
    One(OutputRole),
    /// The left, center and right screens side by side.
    Three,
}

pub struct Winit {
    pub backend: WinitGraphicsBackend<GlesRenderer>,
    pub shaders: Shaders,
    simulated: Simulated,
    /// For the size the window had when it was created.
    damage_tracker: Option<(Size<i32, Physical>, OutputDamageTracker)>,
    /// The host window is still the renderer's current surface. Cleared by
    /// whoever uses the renderer outside of a frame.
    pub window_is_current: bool,
    /// Debug: save the next frame there, then quit.
    pub screenshot: Option<PathBuf>,
}

/// Opens the host window and prepares the renderer. In full screen, the
/// window covers the monitor it appears on, which makes the simulated screens
/// as large as a real one.
pub fn init(simulated: Simulated, fullscreen: bool) -> anyhow::Result<(Winit, WinitEventLoop)> {
    let attributes = WindowAttributes::default()
        .with_title(TITLE)
        .with_surface_size(LogicalSize::new(INITIAL_SIZE.0, INITIAL_SIZE.1))
        .with_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
    let (mut backend, events) = winit::init_from_attributes::<GlesRenderer>(attributes)
        .map_err(|error| anyhow::anyhow!("cannot open the host window: {error}"))?;
    let shaders = Shaders::compile(backend.renderer()).context("cannot compile the shaders")?;

    let winit = Winit {
        backend,
        shaders,
        simulated,
        damage_tracker: None,
        window_is_current: false,
        screenshot: None,
    };
    Ok((winit, events))
}

impl Winit {
    /// The simulated screens for a window of this size.
    fn layout(&self, size: Size<i32, Physical>) -> Vec<ScreenInfo> {
        let canvas = ModelSize::new(size.w, size.h);
        let screens = match self.simulated {
            Simulated::One(role) => vec![(role, Rect::from_size(canvas))],
            Simulated::Three => split_canvas(canvas).to_vec(),
        };
        screens
            .into_iter()
            .zip(0..)
            .map(|((role, rect), id)| ScreenInfo {
                id: OutputId(id),
                role,
                rect,
            })
            .collect()
    }

    /// The screens to start with, as the model and as clients see them.
    pub fn screens(&self) -> Vec<(ScreenInfo, Output)> {
        self.layout(self.backend.window_size())
            .into_iter()
            .map(|info| {
                let output = Output::new(
                    format!("nested-{}", info.id.0),
                    PhysicalProperties {
                        size: (0, 0).into(),
                        subpixel: Subpixel::Unknown,
                        make: "coquille".into(),
                        model: "nested".into(),
                        serial_number: "Unknown".into(),
                    },
                );
                place(&output, info.rect);
                (info, output)
            })
            .collect()
    }

    pub fn show_host_cursor(&self, image: &CursorImageStatus) {
        let window = self.backend.window();
        match image {
            CursorImageStatus::Named(icon) => {
                window.set_cursor(HostCursor::Icon(*icon));
                window.set_cursor_visible(true);
            }
            CursorImageStatus::Hidden | CursorImageStatus::Surface(_) => {
                window.set_cursor_visible(false);
            }
        }
    }
}

/// Gives a simulated screen its size and position.
fn place(output: &Output, rect: Rect) {
    let mode = Mode {
        size: (rect.w, rect.h).into(),
        refresh: REFRESH,
    };
    output.change_current_state(Some(mode), None, None, Some((rect.x, rect.y).into()));
    output.set_preferred(mode);
}

impl State {
    /// The nested backend, when it is the one in use.
    fn winit(&mut self) -> Option<&mut Winit> {
        match &mut self.backend {
            Backend::Winit(winit) => Some(winit),
            Backend::Udev(_) => None,
        }
    }

    /// The host window changed size: so do the simulated screens.
    pub fn resize_window(&mut self, size: Size<i32, Physical>) {
        let Some(winit) = self.winit() else {
            return;
        };
        let screens = winit.layout(size);
        for info in &screens {
            if let Some(output) = self.output(info.id) {
                place(output, info.rect);
            }
        }
        let ids: Vec<OutputId> = screens.iter().map(|screen| screen.id).collect();
        self.shell.move_screens(screens);
        // Panels follow their screen, and what they reserve with them.
        for id in ids {
            self.arrange_panels(id);
        }
        self.apply_arrangement();
    }

    /// Called at the refresh rate: draws if anything changed.
    ///
    /// Frames are paced by this tick rather than by the host's redraw
    /// requests, which stop coming once a frame brought no change.
    pub fn frame_tick(&mut self) {
        if std::mem::take(&mut self.redraw_needed) {
            self.render_window();
        }
    }

    /// Draws every simulated screen into the host window, then tells visible
    /// clients they may draw their next frame.
    pub fn render_window(&mut self) {
        self.shell.refresh_chrome(
            &mut self.painter,
            &self.theme,
            self.hover,
            self.capsule_hover,
        );
        let cursor = self.cursor(false);
        self.refresh_bar();
        let overlay = self.overlay();
        let panels = self.panels();
        let wallpapers = self.active_wallpapers();
        let freeze = self.active_freeze();
        let full_redraw = std::mem::take(&mut self.full_redraw);
        let Backend::Winit(winit) = &mut self.backend else {
            return;
        };
        let screenshot = winit.screenshot.take();

        // The buffer age can only be asked while the window is current, which
        // it is from one frame to the next unless the renderer was used in
        // between. Without an age, everything is redrawn.
        let was_current = std::mem::replace(&mut winit.window_is_current, true);
        let age = if full_redraw || screenshot.is_some() || !was_current {
            0
        } else {
            winit.backend.buffer_age().unwrap_or(0)
        };
        let size = winit.backend.window_size();
        if winit
            .damage_tracker
            .as_ref()
            .is_none_or(|(tracked, _)| *tracked != size)
        {
            // The host window is drawn upside down, hence the flip.
            let tracker = OutputDamageTracker::new(size, 1.0, Transform::Flipped180);
            winit.damage_tracker = Some((size, tracker));
        }
        let Some((_, damage_tracker)) = winit.damage_tracker.as_mut() else {
            return;
        };

        let damage = match winit.backend.bind() {
            Ok((renderer, mut framebuffer)) => {
                self.panel_glass
                    .retain(|(surface, _), _| surface.is_alive());
                let view = Rect::new(0, 0, size.w, size.h);
                let elements = scene::build(
                    renderer,
                    &winit.shaders,
                    &mut self.shell,
                    &self.theme,
                    Layers {
                        view,
                        overlay: &overlay,
                        panel: self.bar.as_ref().map(|bar| (&bar.buffer, bar.rect)),
                        panels: &panels,
                        cursor,
                        wallpapers: &wallpapers,
                        freeze: &freeze,
                        glass: &self.glass,
                        panel_glass: &mut self.panel_glass,
                        bar_glass: &mut self.bar_glass,
                        drop_glass: &mut self.drop_glass,
                        bar_shadow: &mut self.bar_shadow,
                        overlay_shapes: &mut self.overlay_shapes,
                    },
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

        self.send_frame_callbacks(None);
        if screenshot.is_some() {
            self.loop_signal.stop();
        }
    }

    /// Tells the clients shown on `screen` (or on any screen) that a frame
    /// went out: they may draw their next one.
    pub fn send_frame_callbacks(&mut self, screen: Option<OutputId>) {
        let time = self.start_time.elapsed();
        let throttle = Some(Duration::ZERO);
        for managed in &self.shell.windows {
            let on_screen = screen.is_none_or(|id| id == managed.geometry.output);
            let Some(output) = self.output(managed.geometry.output) else {
                continue;
            };
            if managed.geometry.visible && on_screen {
                managed
                    .window
                    .send_frame(output, time, throttle, |_, _| Some(output.clone()));
                managed.window.refresh();
            }
        }
        for panel in self.panels() {
            let on_screen = screen.is_none_or(|id| id == panel.output);
            if let Some(output) = self.output(panel.output).filter(|_| on_screen) {
                panel
                    .surface
                    .send_frame(output, time, throttle, |_, _| Some(output.clone()));
            }
        }
        // X11 menus draw again only once told that a frame went out.
        for window in self.shell.visible_overrides() {
            let rect = window.last_configure();
            let middle = rect.loc + rect.size.downscale(2).to_point();
            let on = self
                .shell
                .screen_at(coquille_model::Point::new(middle.x, middle.y))
                .or(self.shell.screens.first())
                .map(|info| info.id);
            let output = on
                .filter(|id| screen.is_none_or(|wanted| wanted == *id))
                .and_then(|id| self.output(id));
            if let (Some(output), Some(surface)) = (output, window.wl_surface()) {
                send_frames_surface_tree(&surface, output, time, throttle, |_, _| {
                    Some(output.clone())
                });
            }
        }
        if let CursorImageStatus::Surface(surface) = &self.cursor_status {
            let pointer_screen = self.pointer_screen().map(|screen| screen.id);
            let output = pointer_screen
                .filter(|id| screen.is_none_or(|wanted| wanted == *id))
                .and_then(|id| self.output(id));
            if let Some(output) = output {
                send_frames_surface_tree(surface, output, time, throttle, |_, _| {
                    Some(output.clone())
                });
            }
        }
        self.popups.cleanup();
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
