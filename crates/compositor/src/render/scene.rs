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
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::wlr_layer::Layer;
use tracing::warn;

use super::clipped::ClippedSurface;
use super::shaders::Shaders;
use crate::decoration::paint::TitlebarSpec;
use crate::panels::Panel;
use crate::shell::Shell;
use crate::theme::{Rgba, Theme};

/// The compositor draws at scale 1 for now.
const SCALE: f64 = 1.0;
/// How far from a view the pointer may be and still show on it: more than
/// any pointer image is large.
const CURSOR_REACH: i32 = 256;

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
        let element = shape_element(program, shape);
        self.0 = Some((shape, element.clone()));
        element
    }
}

fn shape_element(program: &GlesPixelProgram, shape: Shape) -> PixelShaderElement {
    let (outline, area) = (shape.outline, shape.area);
    PixelShaderElement::new(
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
    )
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

/// What a frame shows besides the windows.
pub struct Layers<'a> {
    /// The part of the global space the frame shows: one screen, or all of
    /// them nested.
    pub view: Rect,
    /// Shell hints above the windows (drop zones), in the global space.
    pub overlay: &'a [Shape],
    /// A panel of the shell above everything but the cursor (the placement
    /// bar), with its place in the global space.
    pub panel: Option<(&'a MemoryRenderBuffer, Rect)>,
    /// Panels of other programs (the dock, its drawer), topmost first.
    pub panels: &'a [Panel],
    pub cursor: Option<Cursor>,
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

/// The surfaces of the panels on some layers, with their menus, topmost
/// first.
fn panel_elements(
    renderer: &mut GlesRenderer,
    panels: &[Panel],
    layers: &[Layer],
    view: Rect,
) -> Vec<SceneElement> {
    let to_view = Point::<i32, Logical>::from((-view.x, -view.y));
    let mut elements = Vec::new();
    for panel in panels.iter().filter(|panel| layers.contains(&panel.layer)) {
        let covered = panel.surface.bbox_with_popups();
        let corner = panel.origin + covered.loc;
        let covered = Rect::new(corner.x, corner.y, covered.size.w, covered.size.h);
        if !overlaps(covered, view) {
            continue;
        }
        let surface = panel.surface.wl_surface();
        let origin = panel.origin + to_view;
        for (popup, offset) in PopupManager::popups_for_surface(surface) {
            let location = physical(origin + offset - popup.geometry().loc);
            elements.extend(render_elements_from_surface_tree::<_, SceneElement>(
                renderer,
                popup.wl_surface(),
                location,
                SCALE,
                1.0,
                Kind::Unspecified,
            ));
        }
        elements.extend(render_elements_from_surface_tree::<_, SceneElement>(
            renderer,
            surface,
            physical(origin),
            SCALE,
            1.0,
            Kind::Unspecified,
        ));
    }
    elements
}

/// Every element of a frame, topmost first: cursor, panels above everything,
/// placement bar, shell hints, menus, panels above the windows, windows from
/// the top one down, capsules, panels under them. Positions are relative to the corner
/// of the view.
pub fn build(
    renderer: &mut GlesRenderer,
    shaders: &Shaders,
    shell: &mut Shell,
    theme: &Theme,
    layers: Layers<'_>,
) -> Vec<SceneElement> {
    let Layers {
        view,
        overlay,
        panel,
        panels,
        cursor,
    } = layers;
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
        // An X11 window has no surface until XWayland pairs them.
        let Some(surface) = managed.window.wl_surface() else {
            continue;
        };
        let surface = &*surface;
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
        // Under the shell's title bar the client's top corners are square;
        // a client that draws its own bar is rounded all around.
        let top = if geometry.titlebar.is_some() {
            0
        } else {
            radius
        };

        if geometry.focused && decorated {
            let outline = reach.translated(shift);
            let shape = Shape {
                outline,
                area: outline,
                color: theme.accent.gold,
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
            let radii = [top as f32, top as f32, radius as f32, radius as f32];
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
            radii: [top, top, radius, radius],
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

    // What is not on this view is left out altogether: a screen that shows
    // a single window can then hand its buffer straight to the hardware.
    let mut elements = Vec::new();
    let to_view: Point<i32, Logical> = (shift.x, shift.y).into();
    let near_view = Rect::new(
        view.x - CURSOR_REACH,
        view.y - CURSOR_REACH,
        view.w + 2 * CURSOR_REACH,
        view.h + 2 * CURSOR_REACH,
    );
    let cursor = cursor.filter(|cursor| {
        let (Cursor::Surface { location, .. } | Cursor::Image { location, .. }) = cursor;
        near_view.contains(ModelPoint::new(location.x, location.y))
    });
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
    elements.extend(panel_elements(renderer, panels, &[Layer::Overlay], view));
    if let Some((buffer, place)) = panel.filter(|(_, place)| overlaps(*place, view)) {
        let corner = ModelPoint::new(place.x + shift.x, place.y + shift.y);
        let location = Point::<i32, Physical>::from((corner.x, corner.y)).to_f64();
        match MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location,
            buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        ) {
            Ok(element) => elements.push(SceneElement::Texture(element)),
            Err(error) => warn!(?error, "cannot upload the placement bar"),
        }
    }
    // Shell hints (drop zones) sit above every window, in the global space.
    for shape in overlay {
        let shape = Shape {
            outline: shape.outline.translated(shift),
            area: shape.area.translated(shift),
            ..*shape
        };
        if overlaps(shape.area, Rect::new(0, 0, view.w, view.h)) {
            elements.push(SceneElement::Shape(shape_element(&shaders.rounded, shape)));
        }
    }
    // X11 menus, tooltips and dropdowns: where they ask to be, above every
    // window, the one shown last on top.
    for window in shell.overrides.iter().rev() {
        let Some(surface) = window.wl_surface() else {
            continue;
        };
        let place = window.last_configure();
        let covered = Rect::new(place.loc.x, place.loc.y, place.size.w, place.size.h);
        if !overlaps(covered, view) {
            continue;
        }
        let location = place.loc + to_view;
        elements.extend(render_elements_from_surface_tree::<_, SceneElement>(
            renderer,
            &surface,
            physical(location),
            SCALE,
            1.0,
            Kind::Unspecified,
        ));
    }
    elements.extend(popups);
    elements.extend(panel_elements(renderer, panels, &[Layer::Top], view));
    elements.extend(windows);
    // The capsules never cover a tidied window: under all of them is under
    // the floating ones too.
    for group in &shell.capsules {
        let band = group.band();
        let Some(buffer) = group.buffer.as_ref().filter(|_| overlaps(band, view)) else {
            continue;
        };
        let corner = band.translated(shift);
        let location = Point::<i32, Physical>::from((corner.x, corner.y)).to_f64();
        match MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location,
            buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        ) {
            Ok(element) => elements.push(SceneElement::Texture(element)),
            Err(error) => warn!(?error, "cannot upload the capsules of a slot"),
        }
    }
    let under = [Layer::Bottom, Layer::Background];
    elements.extend(panel_elements(renderer, panels, &under, view));
    elements
}
