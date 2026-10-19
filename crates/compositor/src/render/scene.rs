//! Turning the shell state into render elements, front to back.

use std::collections::HashMap;

use coquille_model::{Point as ModelPoint, Rect, Scale};
use smithay::backend::renderer::{
    element::{
        Element, Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
        render_elements,
        surface::{
            WaylandSurfaceRenderElement, WaylandSurfaceTexture, render_elements_from_surface_tree,
        },
    },
    gles::{GlesError, GlesPixelProgram, GlesRenderer, Uniform, element::PixelShaderElement},
};
use smithay::desktop::PopupManager;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::wlr_layer::Layer;
use tracing::warn;

use super::clipped::ClippedSurface;
use super::glass::{Blur, GlassCache, GlassElement, Mask};
use super::per_view::PerView;
use super::shaders::Shaders;
use super::shadow::{Shadow, ShadowCache};
use crate::config::GlassConfig;
use crate::decoration::paint::TitlebarSpec;
use crate::panels::Panel;
use crate::shell::Shell;
use crate::theme::{Rgba, Theme};

/// How far from a view the pointer may be and still show on it: more than
/// any pointer image is large.
const CURSOR_REACH: i32 = 256;

render_elements! {
    pub SceneElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Clipped=ClippedSurface,
    Texture=MemoryRenderBufferRenderElement<GlesRenderer>,
    Shape=PixelShaderElement,
    Glass=GlassElement,
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
pub struct ShapeCache(Option<((Shape, f64), PixelShaderElement)>);

impl ShapeCache {
    fn element(
        &mut self,
        program: &GlesPixelProgram,
        shape: Shape,
        scale: f64,
    ) -> PixelShaderElement {
        if let Some((cached, element)) = &self.0 {
            if *cached == (shape, scale) {
                return element.clone();
            }
        }
        let element = shape_element(program, shape, scale);
        self.0 = Some(((shape, scale), element.clone()));
        element
    }
}

fn shape_element(program: &GlesPixelProgram, shape: Shape, scale: f64) -> PixelShaderElement {
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
            Uniform::new("pixel", (1.0 / scale) as f32),
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
    ring: PerView<ShapeCache>,
    /// Window background where the client is narrower than its area.
    beside: PerView<ShapeCache>,
    /// Window background where the client is shorter than its area.
    below: PerView<ShapeCache>,
    /// Glass under a client that draws see-through glass of its own.
    pub glass: PerView<GlassCache>,
    pub titlebar_glass: PerView<GlassCache>,
    pub shadow: PerView<ShadowCache>,
}

/// What is drawn at the pointer.
pub enum Cursor {
    /// A surface provided by the client under the pointer.
    Surface {
        surface: WlSurface,
        location: Point<i32, Logical>,
    },
    /// An image from the cursor theme, of `pixels` drawn over `size`.
    Image {
        buffer: MemoryRenderBuffer,
        location: Point<f64, Logical>,
        pixels: Size<i32, Logical>,
        size: Size<i32, Logical>,
    },
}

/// What a frame shows besides the windows.
pub struct Layers<'a> {
    /// The part of the global space the frame shows: one screen, or all of
    /// them nested.
    pub view: Rect,
    /// How many pixels of the frame a logical pixel of the view takes.
    pub scale: f64,
    /// Shell hints above the windows (drop zones), in the global space.
    pub overlay: &'a [Shape],
    /// A panel of the shell above everything but the cursor (the placement
    /// bar), with its place in the global space.
    pub panel: Option<(&'a MemoryRenderBuffer, Rect)>,
    /// Panels of other programs (the dock, its drawer), topmost first.
    pub panels: &'a [Panel],
    pub cursor: Option<Cursor>,
    /// Screen wallpapers in the global space, shown under every window and panel.
    pub wallpapers: &'a [(MemoryRenderBuffer, Rect)],
    /// Pre-captured frozen screens for interactive region screenshot.
    pub freeze: &'a [(MemoryRenderBuffer, Rect)],
    pub glass: &'a GlassConfig,
    pub panel_glass: &'a mut PanelGlass,
    pub bar_glass: &'a mut PerView<GlassCache>,
    /// The glass of each drop zone, by its place in the global space.
    pub drop_glass: &'a mut HashMap<Rect, GlassCache>,
    pub bar_shadow: &'a mut PerView<ShadowCache>,
    /// The elements of the shell hints, by place and kind (fill or ring).
    pub overlay_shapes: &'a mut OverlayShapes,
}

pub fn rectangle(rect: Rect) -> Rectangle<i32, Logical> {
    Rectangle::new((rect.x, rect.y).into(), (rect.w, rect.h).into())
}

fn physical(point: Point<i32, Logical>, scale: f64) -> Point<i32, Physical> {
    point.to_physical_precise_round(scale)
}

/// Where in the frame a logical rectangle of the view lands.
fn physical_rect(rect: Rect, scale: f64) -> Rectangle<i32, Physical> {
    rectangle(rect).to_physical_precise_round(scale)
}

/// An image the compositor painted for a screen of scale `at`, with as many
/// pixels as `rect` takes there: shown over `rect`, in a view of `scale`.
fn painted(
    renderer: &mut GlesRenderer,
    buffer: &MemoryRenderBuffer,
    rect: Rect,
    scale: f64,
    at: Scale,
    kind: Kind,
) -> Result<MemoryRenderBufferRenderElement<GlesRenderer>, GlesError> {
    let pixels = at.physical(rect.size());
    image(
        renderer,
        buffer,
        physical((rect.x, rect.y).into(), scale).to_f64(),
        (pixels.w, pixels.h).into(),
        (rect.w, rect.h).into(),
        kind,
    )
}

/// An image of `pixels` shown over `size`, logical, at `location`.
fn image(
    renderer: &mut GlesRenderer,
    buffer: &MemoryRenderBuffer,
    location: Point<f64, Physical>,
    pixels: Size<i32, Logical>,
    size: Size<i32, Logical>,
    kind: Kind,
) -> Result<MemoryRenderBufferRenderElement<GlesRenderer>, GlesError> {
    // The buffer has scale 1: its source, in logical units, is its pixels.
    let src = Rectangle::from_size(pixels.to_f64());
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        location,
        buffer,
        None,
        Some(src),
        Some(size),
        kind,
    )
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// The glass caches of the shell's panels: one per element of each surface
/// tree, menus included, and one per piece of liquid glass (from
/// [`LIQUID_CACHES`] on).
pub type PanelGlass = HashMap<(WlSurface, usize), GlassCache>;

/// Where the caches of the liquid glass of a panel start, past those of the
/// elements of its tree.
const LIQUID_CACHES: usize = 1 << 16;

/// The elements of the shell hints: by outline in the global space and ring
/// width (0 for a fill).
pub type OverlayShapes = HashMap<(Rect, i32), PerView<ShapeCache>>;

/// The glass under a client surface, shaped by its alpha, when it has a
/// texture.
#[allow(clippy::too_many_arguments)]
fn alpha_glass(
    element: &WaylandSurfaceRenderElement<GlesRenderer>,
    cache: &mut GlassCache,
    shaders: &Shaders,
    glass: &GlassConfig,
    output_size: Size<i32, Physical>,
    shape: Rectangle<i32, Physical>,
    radii: [f32; 4],
    scale: f64,
) -> Option<SceneElement> {
    let WaylandSurfaceTexture::Texture(texture) = element.texture() else {
        return None;
    };
    let mask = Mask::Alpha {
        texture: texture.clone(),
        src: element.src(),
        dst: element.geometry(scale.into()),
        transform: element.transform(),
        commit: element.current_commit(),
        radii,
    };
    Some(SceneElement::Glass(cache.element(
        shaders,
        shape,
        output_size,
        mask,
        Blur::new(glass.strength, scale),
    )))
}

/// Pushes the elements of a surface tree, each with its glass under it when
/// `glass_of` names the tree's surface and the caches of its glass.
fn push_with_glass(
    elements: &mut Vec<SceneElement>,
    rendered: Vec<WaylandSurfaceRenderElement<GlesRenderer>>,
    glass_of: Option<(&WlSurface, &mut PanelGlass)>,
    shaders: &Shaders,
    glass: &GlassConfig,
    output_size: Size<i32, Physical>,
    scale: f64,
) {
    let Some((surface, caches)) = glass_of else {
        elements.extend(rendered.into_iter().map(SceneElement::Surface));
        return;
    };
    for (index, element) in rendered.into_iter().enumerate() {
        // One cache per element of the tree: two elements sharing an Id
        // would be captured again at every frame.
        let cache = caches.entry((surface.clone(), index)).or_default();
        let shape = element.geometry(scale.into());
        let under = alpha_glass(
            &element,
            cache,
            shaders,
            glass,
            output_size,
            shape,
            [0.0; 4],
            scale,
        );
        elements.push(SceneElement::Surface(element));
        elements.extend(under);
    }
}

/// The surfaces of the panels on some layers, with their menus, topmost
/// first.
#[allow(clippy::too_many_arguments)]
fn panel_elements(
    renderer: &mut GlesRenderer,
    shaders: &Shaders,
    glass: &GlassConfig,
    panel_glass: &mut PanelGlass,
    output_size: Size<i32, Physical>,
    panels: &[Panel],
    layers: &[Layer],
    view: Rect,
    scale: f64,
    specular: f32,
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
        let is_coquille = panel.surface.namespace().starts_with("coquille-");

        for (popup, offset) in PopupManager::popups_for_surface(surface) {
            let popup_surface = popup.wl_surface();
            let location = physical(origin + offset - popup.geometry().loc, scale);
            let rendered =
                render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    popup_surface,
                    location,
                    scale,
                    1.0,
                    Kind::Unspecified,
                );
            push_with_glass(
                &mut elements,
                rendered,
                (is_coquille && glass.blur).then_some((popup_surface, &mut *panel_glass)),
                shaders,
                glass,
                output_size,
                scale,
            );
        }

        let rendered =
            render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                renderer,
                surface,
                physical(origin, scale),
                scale,
                1.0,
                Kind::Unspecified,
            );
        // The program said where its liquid glass is: under the surface,
        // exactly there, rather than wherever it is opaque enough.
        if is_coquille && glass.blur && !panel.liquid.is_empty() {
            elements.extend(rendered.into_iter().map(SceneElement::Surface));
            for (index, piece) in panel.liquid.iter().enumerate() {
                let place = Rect::new(
                    origin.x + piece.x,
                    origin.y + piece.y,
                    piece.width,
                    piece.height,
                );
                let shape = physical_rect(place, scale);
                let radius = (f64::from(piece.radius) * scale) as f32;
                let cache = panel_glass
                    .entry((surface.clone(), LIQUID_CACHES + index))
                    .or_default();
                elements.push(SceneElement::Glass(cache.element(
                    shaders,
                    shape,
                    output_size,
                    Mask::Liquid {
                        radii: [radius; 4],
                        specular,
                    },
                    Blur::new(glass.strength, scale),
                )));
            }
            continue;
        }
        push_with_glass(
            &mut elements,
            rendered,
            (is_coquille && glass.blur).then_some((surface, &mut *panel_glass)),
            shaders,
            glass,
            output_size,
            scale,
        );
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
        scale,
        overlay,
        panel,
        panels,
        cursor,
        wallpapers,
        freeze,
        glass,
        panel_glass,
        bar_glass,
        drop_glass,
        bar_shadow,
        overlay_shapes,
    } = layers;
    let output_size = physical_rect(Rect::from_size(view.size()), scale).size;
    // The edge of liquid glass shows more on a light theme.
    let specular = match theme.kind {
        crate::theme::ThemeKind::Light => 0.75,
        crate::theme::ThemeKind::Dark => 0.4,
    };

    if !freeze.is_empty() {
        let shift = ModelPoint::new(-view.x, -view.y);
        let to_view: Point<i32, Logical> = (shift.x, shift.y).into();
        let near_view = Rect::new(
            view.x - CURSOR_REACH,
            view.y - CURSOR_REACH,
            view.w + 2 * CURSOR_REACH,
            view.h + 2 * CURSOR_REACH,
        );
        let cursor = cursor.filter(|cursor| near_view.contains(cursor.point()));

        let mut elements = Vec::new();
        push_cursor(&mut elements, renderer, cursor, to_view, scale);

        for shape in overlay {
            let shape = Shape {
                outline: shape.outline.translated(shift),
                area: shape.area.translated(shift),
                ..*shape
            };
            if overlaps(shape.area, Rect::new(0, 0, view.w, view.h)) {
                elements.push(SceneElement::Shape(shape_element(
                    &shaders.rounded,
                    shape,
                    scale,
                )));
            }
        }

        for (buffer, rect) in freeze {
            if overlaps(*rect, view) {
                let place = rect.translated(shift);
                let at = Scale::new(scale);
                match painted(renderer, buffer, place, scale, at, Kind::Unspecified) {
                    Ok(element) => elements.push(SceneElement::Texture(element)),
                    Err(error) => warn!(?error, "cannot upload the frozen screenshot buffer"),
                }
            }
        }

        return elements;
    }
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
            let location = physical(origin + offset - popup.geometry().loc, scale);
            popups.extend(render_elements_from_surface_tree::<_, SceneElement>(
                renderer,
                popup.wl_surface(),
                location,
                scale,
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
                color: theme.accent,
                radii: [radius + ring; 4],
                border: ring,
            };
            windows.push(SceneElement::Shape(managed.chrome.ring.get(view).element(
                &shaders.rounded,
                shape,
                scale,
            )));
        }

        let bar = managed.titlebar().map(|bar| bar.translated(shift));
        // The glass of the title bar goes under the client, not over it:
        // what it blurs is what is behind the window, never the window's own
        // content next to the bar.
        let mut titlebar_glass = None;
        if let (Some(bar), Some(buffer)) = (bar, &managed.chrome.titlebar) {
            let at = managed
                .chrome
                .spec
                .as_ref()
                .map_or(Scale::ONE, |spec| spec.scale);
            match painted(renderer, buffer, bar, scale, at, Kind::Unspecified) {
                Ok(element) => {
                    windows.push(SceneElement::Texture(element));
                    if glass.blur {
                        let bar_rect = physical_rect(bar, scale);
                        let r = (f64::from(radius) * scale) as f32;
                        let radii = [r, r, 0.0, 0.0];
                        titlebar_glass = Some(SceneElement::Glass(
                            managed.chrome.titlebar_glass.get(view).element(
                                shaders,
                                bar_rect,
                                output_size,
                                Mask::Rounded { radii },
                                Blur::new(glass.strength, scale),
                            ),
                        ));
                    }
                }
                Err(error) => warn!(?error, "cannot upload a title bar"),
            }
        }

        let location = physical(origin - managed.window.geometry().loc, scale);
        let surfaces = render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<_>>(
            renderer,
            surface,
            location,
            scale,
            1.0,
            Kind::Unspecified,
        );
        let is_settings = managed.app_id == "coquille-settings";
        let client_rect = physical_rect(client, scale);
        let (top_px, radius_px) = (
            (f64::from(top) * scale) as f32,
            (f64::from(radius) * scale) as f32,
        );
        let radii = [top_px, top_px, radius_px, radius_px];

        let client_glass = if is_settings && glass.blur {
            surfaces.first().and_then(|element| {
                alpha_glass(
                    element,
                    managed.chrome.glass.get(view),
                    shaders,
                    glass,
                    output_size,
                    client_rect,
                    radii,
                    scale,
                )
            })
        } else {
            None
        };

        if decorated {
            windows.extend(surfaces.into_iter().map(|inner| {
                SceneElement::Clipped(ClippedSurface::new(
                    inner,
                    shaders.clip.clone(),
                    client_rect,
                    radii,
                ))
            }));
        } else {
            windows.extend(surfaces.into_iter().map(SceneElement::Surface));
        }
        windows.extend(client_glass);
        windows.extend(titlebar_glass);

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
                managed
                    .chrome
                    .beside
                    .get(view)
                    .element(&shaders.rounded, shape, scale),
            ));
        }
        if drawn_h < client.h {
            let area = Rect::new(client.x, client.y + drawn_h, client.w, client.h - drawn_h);
            let shape = Shape { area, ..background };
            windows.push(SceneElement::Shape(managed.chrome.below.get(view).element(
                &shaders.rounded,
                shape,
                scale,
            )));
        }

        let has_x11_shadows = managed.window.x11_surface().is_some_and(|s| {
            let e = s.frame_extents();
            e.left > 0 || e.right > 0 || e.top > 0 || e.bottom > 0
        });
        let has_csd_shadows =
            geometry.titlebar.is_none() && managed.window.geometry().loc != (0, 0).into();
        let draws_own_shadow = has_x11_shadows || has_csd_shadows;

        // Only a floating window casts a shadow: a tidied one would darken
        // its neighbour and the capsules above it, 8 px away.
        if decorated && geometry.floating && !draws_own_shadow {
            let (blur, dy) = if geometry.focused {
                (28.0, 8)
            } else {
                (22.0, 6)
            };
            let frame = managed.frame().translated(shift);
            let shadow = Shadow {
                shape: frame,
                offset: ModelPoint::new(0, dy),
                blur,
                color: theme.shadow,
                radii: [radius as f32; 4],
            };
            if overlaps(shadow.area(), Rect::new(0, 0, view.w, view.h)) {
                windows.push(SceneElement::Shape(
                    managed
                        .chrome
                        .shadow
                        .get(view)
                        .element(&shaders.shadow, shadow),
                ));
            }
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
    let cursor = cursor.filter(|cursor| near_view.contains(cursor.point()));
    push_cursor(&mut elements, renderer, cursor, to_view, scale);
    elements.extend(panel_elements(
        renderer,
        shaders,
        glass,
        panel_glass,
        output_size,
        panels,
        &[Layer::Overlay],
        view,
        scale,
        specular,
    ));
    if let Some((buffer, place)) = panel.filter(|(_, place)| overlaps(*place, view)) {
        let place_rect = place.translated(shift);
        let at = Scale::new(scale);
        match painted(renderer, buffer, place_rect, scale, at, Kind::Unspecified) {
            Ok(element) => {
                elements.push(SceneElement::Texture(element));
                if glass.blur {
                    let shape = physical_rect(place_rect, scale);
                    let radii = [(14.0 * scale) as f32; 4];
                    elements.push(SceneElement::Glass(bar_glass.get(view).element(
                        shaders,
                        shape,
                        output_size,
                        Mask::Rounded { radii },
                        Blur::new(glass.strength, scale),
                    )));
                }
                let shadow = Shadow {
                    shape: place_rect,
                    offset: ModelPoint::new(0, 4),
                    blur: 18.0,
                    color: theme.shadow,
                    radii: [14.0; 4],
                };
                if overlaps(shadow.area(), Rect::new(0, 0, view.w, view.h)) {
                    elements.push(SceneElement::Shape(
                        bar_shadow.get(view).element(&shaders.shadow, shadow),
                    ));
                }
            }
            Err(error) => warn!(?error, "cannot upload the placement bar"),
        }
    }
    // Shell hints (drop zones) sit above every window, in the global space.
    // Each keeps its element while it stays where it is: a drag that only
    // moves the window under them damages nothing else.
    overlay_shapes.retain(|(outline, border), _| {
        overlay
            .iter()
            .any(|shape| shape.outline == *outline && shape.border == *border)
    });
    drop_glass.retain(|outline, _| overlay.iter().any(|shape| shape.outline == *outline));
    for global in overlay {
        let shape = Shape {
            outline: global.outline.translated(shift),
            area: global.area.translated(shift),
            ..*global
        };
        if overlaps(shape.area, Rect::new(0, 0, view.w, view.h)) {
            let cache = overlay_shapes
                .entry((global.outline, global.border))
                .or_default()
                .get(view);
            elements.push(SceneElement::Shape(cache.element(
                &shaders.rounded,
                shape,
                scale,
            )));
            if shape.border == 0 && glass.blur {
                let cache = drop_glass.entry(global.outline).or_default();
                let rect = physical_rect(shape.outline, scale);
                let radii = shape.radii.map(|r| (f64::from(r) * scale) as f32);
                elements.push(SceneElement::Glass(cache.element(
                    shaders,
                    rect,
                    output_size,
                    Mask::Rounded { radii },
                    Blur::new(glass.strength, scale),
                )));
            }
        }
    }
    // X11 menus, tooltips and dropdowns: where they ask to be, above every
    // window, the one shown last on top.
    for window in shell.visible_overrides().rev() {
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
            physical(location, scale),
            scale,
            1.0,
            Kind::Unspecified,
        ));
    }
    elements.extend(popups);
    elements.extend(panel_elements(
        renderer,
        shaders,
        glass,
        panel_glass,
        output_size,
        panels,
        &[Layer::Top],
        view,
        scale,
        specular,
    ));
    elements.extend(windows);
    // The capsules never cover a tidied window: under all of them is under
    // the floating ones too.
    for group in &mut shell.capsules {
        let band = group.band();
        let Some(buffer) = group.buffer.as_ref().filter(|_| overlaps(band, view)) else {
            continue;
        };
        let corner = band.translated(shift);
        let at = group.painted_scale();
        match painted(renderer, buffer, corner, scale, at, Kind::Unspecified) {
            Ok(element) => {
                elements.push(SceneElement::Texture(element));
                if glass.blur {
                    let box_rect = group
                        .layout
                        .rect
                        .translated(ModelPoint::new(corner.x, corner.y));
                    let shape = physical_rect(box_rect, scale);
                    let radius = (f64::from(box_rect.h) * scale / 2.0) as f32;
                    elements.push(SceneElement::Glass(group.glass.get(view).element(
                        shaders,
                        shape,
                        output_size,
                        Mask::Rounded { radii: [radius; 4] },
                        Blur::new(glass.strength, scale),
                    )));
                }
            }
            Err(error) => warn!(?error, "cannot upload the capsules of a slot"),
        }
    }
    let under = [Layer::Bottom, Layer::Background];
    elements.extend(panel_elements(
        renderer,
        shaders,
        glass,
        panel_glass,
        output_size,
        panels,
        &under,
        view,
        scale,
        specular,
    ));
    for (buffer, rect) in wallpapers {
        if overlaps(*rect, view) {
            let place = rect.translated(shift);
            match painted(
                renderer,
                buffer,
                place,
                scale,
                Scale::new(scale),
                Kind::Unspecified,
            ) {
                Ok(element) => elements.push(SceneElement::Texture(element)),
                Err(error) => warn!(?error, "cannot upload a wallpaper"),
            }
        }
    }
    if tracing::enabled!(tracing::Level::TRACE) {
        let glass_count = elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Glass(_)))
            .count();
        tracing::trace!(?view, glass_count, "built scene elements");
    }
    elements
}

impl Cursor {
    /// Where the pointer image starts, in the global space.
    fn point(&self) -> ModelPoint {
        match self {
            Cursor::Surface { location, .. } => ModelPoint::new(location.x, location.y),
            Cursor::Image { location, .. } => {
                ModelPoint::new(location.x.floor() as i32, location.y.floor() as i32)
            }
        }
    }
}

/// The pointer image, `to_view` moving it from the global space to the view.
fn push_cursor(
    elements: &mut Vec<SceneElement>,
    renderer: &mut GlesRenderer,
    cursor: Option<Cursor>,
    to_view: Point<i32, Logical>,
    scale: f64,
) {
    match cursor {
        Some(Cursor::Surface { surface, location }) => {
            elements.extend(render_elements_from_surface_tree::<_, SceneElement>(
                renderer,
                &surface,
                physical(location + to_view, scale),
                scale,
                1.0,
                Kind::Cursor,
            ));
        }
        Some(Cursor::Image {
            buffer,
            location,
            pixels,
            size,
        }) => {
            let location = (location + to_view.to_f64()).to_physical(scale);
            match image(renderer, &buffer, location, pixels, size, Kind::Cursor) {
                Ok(element) => elements.push(SceneElement::Texture(element)),
                Err(error) => warn!(?error, "cannot upload the cursor"),
            }
        }
        None => {}
    }
}
