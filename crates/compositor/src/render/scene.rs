//! Turning the shell state into render elements, front to back.

use coquille_model::{Point as ModelPoint, Rect};
use smithay::backend::renderer::{
    element::{
        Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
        render_elements,
        surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
    },
    gles::{GlesPixelProgram, GlesRenderer, Uniform, element::PixelShaderElement},
};
use smithay::desktop::PopupManager;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle};
use tracing::warn;

use super::clipped::ClippedSurface;
use super::shaders::Shaders;
use crate::decoration::paint::TitlebarSpec;
use crate::shell::Shell;
use crate::theme::{Rgba, Theme};

/// The compositor draws at scale 1 for now.
const SCALE: f64 = 1.0;

render_elements! {
    pub SceneElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Clipped=ClippedSurface,
    Texture=MemoryRenderBufferRenderElement<GlesRenderer>,
    Shape=PixelShaderElement,
}

/// A rounded rectangle drawn by the `rounded` shader, or a part of one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    /// The rounded rectangle.
    pub outline: Rect,
    /// The part of it that is drawn.
    pub area: Rect,
    pub color: Rgba,
    /// Top-left, top-right, bottom-right, bottom-left.
    pub radii: [i32; 4],
    /// 0 fills the shape; otherwise the thickness of the ring.
    pub border: i32,
}

/// Keeps a shape's element between frames, so that an unchanged shape
/// causes no damage.
#[derive(Default)]
pub struct ShapeCache(Option<(Shape, PixelShaderElement)>);

impl ShapeCache {
    fn element(&mut self, program: &GlesPixelProgram, shape: Shape) -> PixelShaderElement {
        if let Some((cached, element)) = &self.0 {
            if *cached == shape {
                return element.clone();
            }
        }
        let (outline, area) = (shape.outline, shape.area);
        let element = PixelShaderElement::new(
            program.clone(),
            rectangle(area),
            None,
            1.0,
            vec![
                Uniform::new("shape_size", [outline.w as f32, outline.h as f32]),
                Uniform::new(
                    "shape_offset",
                    [(area.x - outline.x) as f32, (area.y - outline.y) as f32],
                ),
                Uniform::new("color", shape.color.premultiplied()),
                Uniform::new("radius", shape.radii.map(|radius| radius as f32)),
                Uniform::new("border", shape.border as f32),
            ],
            Kind::Unspecified,
        );
        self.0 = Some((shape, element.clone()));
        element
    }
}

/// What the compositor draws around one window.
#[derive(Default)]
pub struct Chrome {
    /// What `titlebar` was painted from.
    pub spec: Option<TitlebarSpec>,
    pub titlebar: Option<MemoryRenderBuffer>,
    ring: ShapeCache,
    /// Window background where the client is narrower than its area.
    beside: ShapeCache,
    /// Window background where the client is shorter than its area.
    below: ShapeCache,
}

/// What is drawn at the pointer.
pub enum Cursor {
    /// A surface provided by the client under the pointer.
    Surface {
        surface: WlSurface,
        location: Point<i32, Logical>,
    },
    /// An image from the cursor theme.
    Image {
        buffer: MemoryRenderBuffer,
        location: Point<i32, Logical>,
    },
}

pub fn rectangle(rect: Rect) -> Rectangle<i32, Logical> {
    Rectangle::new((rect.x, rect.y).into(), (rect.w, rect.h).into())
}

fn physical(point: Point<i32, Logical>) -> Point<i32, Physical> {
    point.to_physical_precise_round(SCALE)
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// Every element seen through `view`, a part of the global space (one screen,
/// or all of them nested), topmost first: cursor, popups, then windows from
/// the top one down. Positions are relative to the view's corner.
pub fn build(
    renderer: &mut GlesRenderer,
    shaders: &Shaders,
    shell: &mut Shell,
    theme: &Theme,
    view: Rect,
    cursor: Option<Cursor>,
) -> Vec<SceneElement> {
    let metrics = *shell.desk.metrics();
    // From the global space to the view.
    let shift = ModelPoint::new(-view.x, -view.y);
    let mut popups = Vec::new();
    let mut windows = Vec::new();

    for managed in shell.windows.iter_mut().rev() {
        let geometry = managed.geometry;
        let ring = metrics.ring_width;
        let global = managed.frame();
        let reach = Rect::new(
            global.x - ring,
            global.y - ring,
            global.w + 2 * ring,
            global.h + 2 * ring,
        );
        if !geometry.visible || !overlaps(reach, view) {
            continue;
        }
        let Some(toplevel) = managed.window.toplevel() else {
            continue;
        };
        let surface = toplevel.wl_surface();
        let client = managed.client().translated(shift);
        let origin: Point<i32, Logical> = (client.x, client.y).into();

        for (popup, offset) in PopupManager::popups_for_surface(surface) {
            let location = physical(origin + offset - popup.geometry().loc);
            popups.extend(render_elements_from_surface_tree::<_, SceneElement>(
                renderer,
                popup.wl_surface(),
                location,
                SCALE,
                1.0,
                Kind::Unspecified,
            ));
        }

        let decorated = !geometry.anchored;
        let radius = if decorated { metrics.corner_radius } else { 0 };

        if geometry.focused && decorated {
            let outline = reach.translated(shift);
            let shape = Shape {
                outline,
                area: outline,
                color: theme.accent,
                radii: [radius + ring; 4],
                border: ring,
            };
            windows.push(SceneElement::Shape(
                managed.chrome.ring.element(&shaders.rounded, shape),
            ));
        }

        let bar = managed.titlebar().map(|bar| bar.translated(shift));
        if let (Some(bar), Some(buffer)) = (bar, &managed.chrome.titlebar) {
            let location = Point::<i32, Physical>::from((bar.x, bar.y)).to_f64();
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                location,
                buffer,
                None,
                None,
                None,
                Kind::Unspecified,
            ) {
                Ok(element) => windows.push(SceneElement::Texture(element)),
                Err(error) => warn!(?error, "cannot upload a title bar"),
            }
        }

        let location = physical(origin - managed.window.geometry().loc);
        let surfaces = render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<_>>(
            renderer,
            surface,
            location,
            SCALE,
            1.0,
            Kind::Unspecified,
        );
        if decorated {
            let clip = rectangle(client).to_physical_precise_round(SCALE);
            let radii = [0.0, 0.0, radius as f32, radius as f32];
            windows.extend(surfaces.into_iter().map(|inner| {
                SceneElement::Clipped(ClippedSurface::new(
                    inner,
                    shaders.clip.clone(),
                    clip,
                    radii,
                ))
            }));
        } else {
            windows.extend(surfaces.into_iter().map(SceneElement::Surface));
        }

        // The client may draw less than its area (fixed-size dialogs, or no
        // buffer yet): the rest shows the window background. Nothing is put
        // behind the client itself, so see-through clients stay see-through.
        let drawn = managed.window.geometry().size;
        let (drawn_w, drawn_h) = (drawn.w.clamp(0, client.w), drawn.h.clamp(0, client.h));
        let background = Shape {
            outline: client,
            area: client,
            color: if decorated {
                theme.surface
            } else {
                Rgba::rgb(0)
            },
            radii: [0, 0, radius, radius],
            border: 0,
        };
        if drawn_w < client.w && drawn_h > 0 {
            let area = Rect::new(client.x + drawn_w, client.y, client.w - drawn_w, drawn_h);
            let shape = Shape { area, ..background };
            windows.push(SceneElement::Shape(
                managed.chrome.beside.element(&shaders.rounded, shape),
            ));
        }
        if drawn_h < client.h {
            let area = Rect::new(client.x, client.y + drawn_h, client.w, client.h - drawn_h);
            let shape = Shape { area, ..background };
            windows.push(SceneElement::Shape(
                managed.chrome.below.element(&shaders.rounded, shape),
            ));
        }
    }

    let mut elements = Vec::new();
    let to_view: Point<i32, Logical> = (shift.x, shift.y).into();
    match cursor {
        Some(Cursor::Surface { surface, location }) => {
            elements.extend(render_elements_from_surface_tree::<_, SceneElement>(
                renderer,
                &surface,
                physical(location + to_view),
                SCALE,
                1.0,
                Kind::Cursor,
            ));
        }
        Some(Cursor::Image { buffer, location }) => {
            match MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                physical(location + to_view).to_f64(),
                &buffer,
                None,
                None,
                None,
                Kind::Cursor,
            ) {
                Ok(element) => elements.push(SceneElement::Texture(element)),
                Err(error) => warn!(?error, "cannot upload the cursor"),
            }
        }
        None => {}
    }
    elements.extend(popups);
    elements.extend(windows);
    elements
}
