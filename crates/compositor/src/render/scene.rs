//! Turning the shell state into render elements, front to back.

use std::collections::HashMap;
use std::time::Instant;

use coquille_ipc::GlassShape;
use coquille_model::{
    Metrics, Point as ModelPoint, Rect, Scale, Size as ModelSize, WindowGeometry,
};
use smithay::backend::renderer::{
    Renderer, Texture,
    element::{
        Element, Id, Kind,
        memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement},
        render_elements,
        surface::{
            WaylandSurfaceRenderElement, WaylandSurfaceTexture, render_elements_from_surface_tree,
        },
        texture::TextureRenderElement,
    },
    gles::{
        GlesError, GlesPixelProgram, GlesRenderer, GlesTexture, Uniform,
        element::PixelShaderElement,
    },
};
use smithay::desktop::{PopupManager, Window};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Physical, Point, Rectangle, Size, Transform};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shell::wlr_layer::Layer;
use tracing::warn;

use super::clipped::ClippedSurface;
use super::glass::{Blur, GlassCache, GlassElement, Mask};
use super::per_view::PerView;
use super::shaders::Shaders;
use super::shadow::{Shadow, ShadowCache};
use crate::animation::{Area, Arrival, Curve, Entry, Exit, Look, Pace, durations::PANEL};
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
    Frozen=TextureRenderElement<GlesTexture>,
    FrozenClipped=ClippedSurface<TextureRenderElement<GlesTexture>>,
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
pub struct ShapeCache(Option<((Shape, f64, f32), PixelShaderElement)>);

impl ShapeCache {
    fn element(
        &mut self,
        program: &GlesPixelProgram,
        shape: Shape,
        scale: f64,
        alpha: f32,
    ) -> PixelShaderElement {
        if let Some((cached, element)) = &self.0 {
            if *cached == (shape, scale, alpha) {
                return element.clone();
            }
        }
        let element = shape_element(program, shape, scale, alpha);
        self.0 = Some(((shape, scale, alpha), element.clone()));
        element
    }
}

fn shape_element(
    program: &GlesPixelProgram,
    shape: Shape,
    scale: f64,
    alpha: f32,
) -> PixelShaderElement {
    let (outline, area) = (shape.outline, shape.area);
    PixelShaderElement::new(
        program.clone(),
        rectangle(area),
        None,
        alpha,
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
    /// What the client showed at the last frame it stood still.
    pub snapshot: Vec<Piece>,
    /// The client draws its own shadow (X11 frame extents, client-side
    /// decorations): the shell adds none.
    pub own_shadow: bool,
}

/// What a client last showed of one of its surfaces: kept to draw it once
/// the client is gone.
#[derive(Debug, Clone)]
pub struct Piece {
    id: Id,
    texture: GlesTexture,
    src: Rectangle<f64, Logical>,
    transform: Transform,
    /// Where it was, from the corner of what it belongs to, in logical
    /// pixels.
    place: Area,
}

/// What `elements`, drawn with their corner at `corner`, show: the textures
/// and where they are.
fn pieces(
    elements: &[WaylandSurfaceRenderElement<GlesRenderer>],
    corner: Point<i32, Physical>,
    scale: f64,
) -> Vec<Piece> {
    elements
        .iter()
        .filter_map(|element| {
            let WaylandSurfaceTexture::Texture(texture) = element.texture() else {
                return None;
            };
            let at = element.geometry(scale.into());
            let size = texture.size().to_f64();
            Some(Piece {
                id: Id::new(),
                texture: texture.clone(),
                src: element.src().to_logical(1.0, element.transform(), &size),
                transform: element.transform(),
                place: Area {
                    x: f64::from(at.loc.x - corner.x) / scale,
                    y: f64::from(at.loc.y - corner.y) / scale,
                    w: f64::from(at.size.w) / scale,
                    h: f64::from(at.size.h) / scale,
                },
            })
        })
        .collect()
}

/// `pieces` drawn as `look` shows what has its corner at `corner`, in the
/// view.
fn frozen(
    renderer: &GlesRenderer,
    pieces: &[Piece],
    look: &Look,
    corner: (f64, f64),
    scale: f64,
) -> Vec<TextureRenderElement<GlesTexture>> {
    let (sx, sy) = look.scale();
    let (x, y) = look.point(corner.0, corner.1);
    pieces
        .iter()
        .map(|piece| {
            let location = Point::<f64, Physical>::from((
                ((x + piece.place.x * sx) * scale).round(),
                ((y + piece.place.y * sy) * scale).round(),
            ));
            let size = Size::<i32, Logical>::from((
                (piece.place.w * sx).round() as i32,
                (piece.place.h * sy).round() as i32,
            ));
            TextureRenderElement::from_static_texture(
                piece.id.clone(),
                renderer.context_id(),
                location,
                piece.texture.clone(),
                1,
                piece.transform,
                Some(look.alpha),
                Some(piece.src),
                Some(size),
                None,
                Kind::Unspecified,
            )
        })
        .collect()
}

/// `rect` with `margin` more on every side.
fn grown(rect: Rect, margin: i32) -> Rect {
    Rect::new(
        rect.x - margin,
        rect.y - margin,
        rect.w + 2 * margin,
        rect.h + 2 * margin,
    )
}

/// The physical pixel a logical point of the view falls on.
fn pixel(x: f64, y: f64, scale: f64) -> Point<i32, Physical> {
    (((x * scale).round()) as i32, ((y * scale).round()) as i32).into()
}

/// What a window draws with.
pub enum Content<'a> {
    /// Its client's surfaces, as they are now.
    Live(&'a Window),
    /// What they last showed: the client is gone.
    Frozen,
}

/// One window to draw, in the global space.
pub struct WindowDraw<'a> {
    pub geometry: WindowGeometry,
    pub frame: Rect,
    pub titlebar: Option<Rect>,
    pub client: Rect,
    pub content: Content<'a>,
    pub chrome: &'a mut Chrome,
    /// Where its frame shows, and how opaque, in the global space.
    pub look: Look,
    /// How lit its accent ring is.
    pub ring: f32,
    /// Its client draws see-through glass of its own, with blur under it.
    pub glassy: bool,
}

/// What every window of a frame is drawn with.
struct WindowCtx<'a> {
    shaders: &'a Shaders,
    theme: &'a Theme,
    glass: &'a GlassConfig,
    metrics: Metrics,
    view: Rect,
    scale: f64,
    output_size: Size<i32, Physical>,
}

/// The elements of one window, topmost first, into `windows`; its menus
/// into `popups`.
fn window_elements(
    renderer: &mut GlesRenderer,
    ctx: &WindowCtx<'_>,
    draw: WindowDraw<'_>,
    windows: &mut Vec<SceneElement>,
    popups: &mut Vec<SceneElement>,
) {
    let WindowDraw {
        geometry,
        frame,
        titlebar,
        client,
        content,
        chrome,
        look,
        ring: lit,
        glassy,
    } = draw;
    let (view, scale, theme, shaders, glass) =
        (ctx.view, ctx.scale, ctx.theme, ctx.shaders, ctx.glass);
    let shift = ModelPoint::new(-view.x, -view.y);
    let look = look.translated(shift);
    let (frame, client) = (frame.translated(shift), client.translated(shift));
    let bar = titlebar.map(|bar| bar.translated(shift));
    let (sx, sy) = look.scale();
    let alpha = look.alpha;
    // Corners shrink with the window, rings and edges do not.
    let shrink = sx.min(sy).min(1.0);
    let scaled = |radius: i32| (f64::from(radius) * shrink).round() as i32;
    let ring = ctx.metrics.ring_width;

    let decorated = !geometry.anchored;
    let radius = if decorated {
        ctx.metrics.corner_radius
    } else {
        0
    };
    // Under the shell's title bar the client's top corners are square;
    // a client that draws its own bar is rounded all around.
    let top = if geometry.titlebar.is_some() {
        0
    } else {
        radius
    };
    let shown_client = look.rect(client);
    let corner = look.point(f64::from(client.x), f64::from(client.y));
    let surface_scale = (scale * sx, scale * sy);

    let live = match content {
        Content::Live(window) => window.wl_surface().map(|surface| (window, surface)),
        Content::Frozen => None,
    };
    if let Some((_, surface)) = &live {
        for (popup, offset) in PopupManager::popups_for_surface(surface) {
            let delta = offset - popup.geometry().loc;
            let location = pixel(
                corner.0 + f64::from(delta.x) * sx,
                corner.1 + f64::from(delta.y) * sy,
                scale,
            );
            popups.extend(render_elements_from_surface_tree::<_, SceneElement>(
                renderer,
                popup.wl_surface(),
                location,
                surface_scale,
                alpha,
                Kind::Unspecified,
            ));
        }
    }

    let lit = lit * alpha;
    if decorated && lit > 0.0 {
        let outline = grown(look.rect(frame), ring);
        let shape = Shape {
            outline,
            area: outline,
            color: theme.accent,
            radii: [scaled(radius) + ring; 4],
            border: ring,
        };
        windows.push(SceneElement::Shape(chrome.ring.get(view).element(
            &shaders.rounded,
            shape,
            scale,
            lit,
        )));
    }

    // The glass of the title bar goes under the client, not over it:
    // what it blurs is what is behind the window, never the window's own
    // content next to the bar.
    let mut titlebar_glass = None;
    if let (Some(bar), Some(buffer)) = (bar, &chrome.titlebar) {
        let at = chrome.spec.as_ref().map_or(Scale::ONE, |spec| spec.scale);
        let shown = look.rect(bar);
        match painted_as(renderer, buffer, bar.size(), shown, scale, at, alpha) {
            Ok(element) => {
                windows.push(SceneElement::Texture(element));
                if glass.blur {
                    let r = (f64::from(scaled(radius)) * scale) as f32;
                    titlebar_glass =
                        Some(SceneElement::Glass(chrome.titlebar_glass.get(view).faded(
                            shaders,
                            physical_rect(shown, scale),
                            ctx.output_size,
                            Mask::Rounded {
                                radii: [r, r, 0.0, 0.0],
                            },
                            Blur::new(glass.strength, scale),
                            alpha,
                        )));
                }
            }
            Err(error) => warn!(?error, "cannot upload a title bar"),
        }
    }

    let client_rect = physical_rect(shown_client, scale);
    let (top_px, radius_px) = (
        (f64::from(scaled(top)) * scale) as f32,
        (f64::from(scaled(radius)) * scale) as f32,
    );
    let radii = [top_px, top_px, radius_px, radius_px];
    let mut drawn = (client.w, client.h);

    if let Some((window, surface)) = &live {
        let loc = window.geometry().loc;
        let location = pixel(
            corner.0 - f64::from(loc.x) * sx,
            corner.1 - f64::from(loc.y) * sy,
            scale,
        );
        let surfaces = render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<_>>(
            renderer,
            surface,
            location,
            surface_scale,
            alpha,
            Kind::Unspecified,
        );
        // What it shows at rest is what it leaves when it closes.
        if look.is_still() {
            chrome.snapshot = pieces(&surfaces, pixel(corner.0, corner.1, scale), scale);
            let own_x11 = window.x11_surface().is_some_and(|s| {
                let e = s.frame_extents();
                e.left > 0 || e.right > 0 || e.top > 0 || e.bottom > 0
            });
            let own_csd = geometry.titlebar.is_none() && loc != (0, 0).into();
            chrome.own_shadow = own_x11 || own_csd;
        }

        let client_glass = if glassy && glass.blur {
            surfaces.first().and_then(|element| {
                alpha_glass(
                    element,
                    chrome.glass.get(view),
                    shaders,
                    glass,
                    ctx.output_size,
                    client_rect,
                    radii,
                    scale,
                    alpha,
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
        let size = window.geometry().size;
        drawn = (size.w.clamp(0, client.w), size.h.clamp(0, client.h));
    } else {
        let corner = (f64::from(client.x), f64::from(client.y));
        let elements = frozen(renderer, &chrome.snapshot, &look, corner, scale);
        if decorated {
            windows.extend(elements.into_iter().map(|inner| {
                SceneElement::FrozenClipped(ClippedSurface::new(
                    inner,
                    shaders.clip.clone(),
                    client_rect,
                    radii,
                ))
            }));
        } else {
            windows.extend(elements.into_iter().map(SceneElement::Frozen));
        }
    }
    windows.extend(titlebar_glass);

    // The client may draw less than its area (fixed-size dialogs, or no
    // buffer yet): the rest shows the window background. Nothing is put
    // behind the client itself, so see-through clients stay see-through.
    let (drawn_w, drawn_h) = drawn;
    let background = Shape {
        outline: shown_client,
        area: shown_client,
        color: if decorated {
            theme.surface
        } else {
            Rgba::rgb(0)
        },
        radii: [scaled(top), scaled(top), scaled(radius), scaled(radius)],
        border: 0,
    };
    if drawn_w < client.w && drawn_h > 0 {
        let area = Rect::new(client.x + drawn_w, client.y, client.w - drawn_w, drawn_h);
        let shape = Shape {
            area: look.rect(area),
            ..background
        };
        windows.push(SceneElement::Shape(chrome.beside.get(view).element(
            &shaders.rounded,
            shape,
            scale,
            alpha,
        )));
    }
    if drawn_h < client.h {
        let area = Rect::new(client.x, client.y + drawn_h, client.w, client.h - drawn_h);
        let shape = Shape {
            area: look.rect(area),
            ..background
        };
        windows.push(SceneElement::Shape(chrome.below.get(view).element(
            &shaders.rounded,
            shape,
            scale,
            alpha,
        )));
    }

    // Only a floating window casts a shadow: a tidied one would darken
    // its neighbour and the capsules above it, 8 px away.
    if decorated && geometry.floating && !chrome.own_shadow {
        let (blur, dy) = if geometry.focused {
            (28.0, 8)
        } else {
            (22.0, 6)
        };
        let shadow = Shadow {
            shape: look.rect(frame),
            offset: ModelPoint::new(0, dy),
            blur,
            color: theme.shadow,
            radii: [scaled(radius) as f32; 4],
            alpha,
        };
        if overlaps(shadow.area(), Rect::new(0, 0, view.w, view.h)) {
            windows.push(SceneElement::Shape(
                chrome.shadow.get(view).element(&shaders.shadow, shadow),
            ));
        }
    }
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
    /// How opaque the hints are: they fade in.
    pub overlay_alpha: f32,
    /// A panel of the compositor above everything but the cursor, with its
    /// place in the global space and how it shows there.
    pub panel: Option<(&'a MemoryRenderBuffer, Rect, Look)>,
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
    /// How the panels of the shell come and go.
    pub panel_motion: PanelAnimation<'a>,
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
        None,
    )
}

/// An image painted for `size` at the scale `at`, stretched over `rect`
/// and `alpha` opaque: on its way somewhere.
fn painted_as(
    renderer: &mut GlesRenderer,
    buffer: &MemoryRenderBuffer,
    size: ModelSize,
    rect: Rect,
    scale: f64,
    at: Scale,
    alpha: f32,
) -> Result<MemoryRenderBufferRenderElement<GlesRenderer>, GlesError> {
    let pixels = at.physical(size);
    image(
        renderer,
        buffer,
        physical((rect.x, rect.y).into(), scale).to_f64(),
        (pixels.w, pixels.h).into(),
        (rect.w, rect.h).into(),
        Kind::Unspecified,
        (alpha < 1.0).then_some(alpha),
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
    alpha: Option<f32>,
) -> Result<MemoryRenderBufferRenderElement<GlesRenderer>, GlesError> {
    // The buffer has scale 1: its source, in logical units, is its pixels.
    let src = Rectangle::from_size(pixels.to_f64());
    MemoryRenderBufferRenderElement::from_buffer(
        renderer,
        location,
        buffer,
        alpha,
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
    alpha: f32,
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
    Some(SceneElement::Glass(cache.faded(
        shaders,
        shape,
        output_size,
        mask,
        Blur::new(glass.strength, scale),
        alpha,
    )))
}

/// Pushes the elements of a surface tree, each with its glass under it when
/// `glass_of` names the tree's surface and the caches of its glass.
#[allow(clippy::too_many_arguments)]
fn push_with_glass(
    elements: &mut Vec<SceneElement>,
    rendered: Vec<WaylandSurfaceRenderElement<GlesRenderer>>,
    glass_of: Option<(&WlSurface, &mut PanelGlass)>,
    shaders: &Shaders,
    glass: &GlassConfig,
    output_size: Size<i32, Physical>,
    scale: f64,
    alpha: f32,
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
            alpha,
        );
        elements.push(SceneElement::Surface(element));
        elements.extend(under);
    }
}

/// How a panel of the shell comes in, and what it leaves when it goes.
#[derive(Debug, Default)]
pub struct PanelMotion {
    /// It drew already: it came in.
    seen: bool,
    entry: Option<Entry>,
    /// What it showed at the last frame it stood still.
    pub snapshot: Vec<Piece>,
}

impl PanelMotion {
    /// Still coming in.
    pub fn busy(&self, now: Instant) -> bool {
        self.entry.is_some_and(|entry| !entry.done(now))
    }
}

/// The motion of each panel of the shell, by its surface.
pub type PanelMotions = HashMap<WlSurface, PanelMotion>;

/// A panel of the shell that is gone, fading out where it was.
#[derive(Debug)]
pub struct PanelGhost {
    pub layer: Layer,
    /// Where it was, in the global space.
    pub place: Rect,
    pub snapshot: Vec<Piece>,
    /// Its liquid glass, from its corner.
    pub liquid: Vec<GlassShape>,
    pub exit: Exit,
    glass: Vec<GlassCache>,
}

impl PanelGhost {
    pub fn new(
        layer: Layer,
        place: Rect,
        snapshot: Vec<Piece>,
        liquid: Vec<GlassShape>,
        exit: Exit,
    ) -> Self {
        Self {
            layer,
            place,
            snapshot,
            liquid,
            exit,
            glass: Vec::new(),
        }
    }
}

/// The surfaces of the panels on some layers, with their menus, topmost
/// first; the panels gone from these layers above them, fading out.
#[allow(clippy::too_many_arguments)]
fn panel_elements(
    renderer: &mut GlesRenderer,
    shaders: &Shaders,
    glass: &GlassConfig,
    panel_glass: &mut PanelGlass,
    output_size: Size<i32, Physical>,
    panels: &[Panel],
    motion: &mut PanelAnimation<'_>,
    layers: &[Layer],
    view: Rect,
    scale: f64,
    specular: f32,
) -> Vec<SceneElement> {
    let to_view = Point::<i32, Logical>::from((-view.x, -view.y));
    let shift = ModelPoint::new(-view.x, -view.y);
    let now = motion.now;
    let mut elements = Vec::new();

    for ghost in motion.ghosts.iter_mut() {
        if !layers.contains(&ghost.layer) || !overlaps(ghost.place, view) {
            continue;
        }
        let place = ghost.place.translated(shift);
        let look = ghost.exit.look(place, now);
        let corner = (f64::from(place.x), f64::from(place.y));
        let pieces = frozen(renderer, &ghost.snapshot, &look, corner, scale);
        elements.extend(pieces.into_iter().map(SceneElement::Frozen));
        if glass.blur {
            ghost
                .glass
                .resize_with(ghost.liquid.len(), GlassCache::default);
            for (shape, cache) in ghost.liquid.iter().zip(&mut ghost.glass) {
                let rect = Rect::new(
                    place.x + shape.x,
                    place.y + shape.y,
                    shape.width,
                    shape.height,
                );
                let (sx, sy) = look.scale();
                let radius = (f64::from(shape.radius) * scale * sx.min(sy).min(1.0)) as f32;
                elements.push(SceneElement::Glass(cache.faded(
                    shaders,
                    physical_rect(look.rect(rect), scale),
                    output_size,
                    Mask::Liquid {
                        radii: [radius; 4],
                        specular,
                    },
                    Blur::new(glass.strength, scale),
                    look.alpha,
                )));
            }
        }
    }

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
        let bbox = panel.surface.bbox();
        let place = Rect::new(
            origin.x + bbox.loc.x,
            origin.y + bbox.loc.y,
            bbox.size.w,
            bbox.size.h,
        );

        // The panels of the shell come in once they have something to show.
        let mut state = is_coquille.then(|| motion.motions.entry(surface.clone()).or_default());
        if let Some(state) = state.as_deref_mut() {
            if !state.seen && !place.is_empty() {
                state.seen = true;
                state.entry = motion
                    .pace
                    .tween(now, PANEL, Curve::Settle)
                    .filter(|_| panel.arrival != Arrival::Own)
                    .map(|tween| Entry {
                        tween,
                        slide: panel.arrival.offset(place),
                    });
            }
        }
        let look = state
            .as_deref()
            .and_then(|state| state.entry)
            .map_or(Look::still(place), |entry| entry.look(place, now));
        let (sx, sy) = look.scale();
        let surface_scale = (scale * sx, scale * sy);
        let at = |point: Point<i32, Logical>| {
            let (x, y) = look.point(f64::from(point.x), f64::from(point.y));
            pixel(x, y, scale)
        };

        for (popup, offset) in PopupManager::popups_for_surface(surface) {
            let popup_surface = popup.wl_surface();
            let location = at(origin + offset - popup.geometry().loc);
            let rendered =
                render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    popup_surface,
                    location,
                    surface_scale,
                    look.alpha,
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
                look.alpha,
            );
        }

        let rendered =
            render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                renderer,
                surface,
                at(origin),
                surface_scale,
                look.alpha,
                Kind::Unspecified,
            );
        // What it shows at rest is what it leaves when it goes.
        if let Some(state) = state.filter(|_| look.is_still()) {
            state.snapshot = pieces(&rendered, physical((place.x, place.y).into(), scale), scale);
        }
        // The program said where its liquid glass is: under the surface,
        // exactly there, rather than wherever it is opaque enough.
        if is_coquille && glass.blur && !panel.liquid.is_empty() {
            elements.extend(rendered.into_iter().map(SceneElement::Surface));
            for (index, piece) in panel.liquid.iter().enumerate() {
                let rect = Rect::new(
                    origin.x + piece.x,
                    origin.y + piece.y,
                    piece.width,
                    piece.height,
                );
                let shape = physical_rect(look.rect(rect), scale);
                let radius = (f64::from(piece.radius) * scale * sx.min(sy).min(1.0)) as f32;
                let cache = panel_glass
                    .entry((surface.clone(), LIQUID_CACHES + index))
                    .or_default();
                elements.push(SceneElement::Glass(cache.faded(
                    shaders,
                    shape,
                    output_size,
                    Mask::Liquid {
                        radii: [radius; 4],
                        specular,
                    },
                    Blur::new(glass.strength, scale),
                    look.alpha,
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
            look.alpha,
        );
    }
    elements
}

/// How the panels of the shell come and go, for one frame.
pub struct PanelAnimation<'a> {
    pub motions: &'a mut PanelMotions,
    pub ghosts: &'a mut [PanelGhost],
    pub pace: Pace,
    pub now: Instant,
}

/// Every element of a frame, topmost first: cursor, panels above everything,
/// the shell's own panel, shell hints, menus, panels above the windows, windows from
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
        overlay_alpha,
        mut panel_motion,
    } = layers;
    let now = panel_motion.now;
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
                    1.0,
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
    let ring = metrics.ring_width;
    let ctx = WindowCtx {
        shaders,
        theme,
        glass,
        metrics,
        view,
        scale,
        output_size,
    };

    // Closed windows fade out over the others.
    for ghost in &mut shell.ghosts {
        let look = ghost.exit.look(ghost.frame, now);
        if !overlaps(grown(look.rect(ghost.frame), ring), view) {
            continue;
        }
        let lit = f32::from(u8::from(ghost.geometry.focused));
        window_elements(
            renderer,
            &ctx,
            WindowDraw {
                geometry: ghost.geometry,
                frame: ghost.frame,
                titlebar: ghost.titlebar,
                client: ghost.client,
                content: Content::Frozen,
                chrome: &mut ghost.chrome,
                look,
                ring: lit,
                glassy: false,
            },
            &mut windows,
            &mut popups,
        );
    }

    for managed in shell.windows.iter_mut().rev() {
        let geometry = managed.geometry;
        if !geometry.visible && !managed.motion.drawn_hidden(now) {
            continue;
        }
        let (frame, titlebar, client) = (managed.frame(), managed.titlebar(), managed.client());
        // A game is never animated: its buffer may go straight to the screen.
        let look = if geometry.anchored {
            Look::still(frame)
        } else {
            managed.motion.look(frame, now)
        };
        if !overlaps(grown(look.rect(frame), ring), view) {
            continue;
        }
        let lit = managed.motion.ring_alpha(geometry.focused, now);
        let glassy = managed.app_id == "coquille-settings";
        window_elements(
            renderer,
            &ctx,
            WindowDraw {
                geometry,
                frame,
                titlebar,
                client,
                content: Content::Live(&managed.window),
                chrome: &mut managed.chrome,
                look,
                ring: lit,
                glassy,
            },
            &mut windows,
            &mut popups,
        );
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
        &mut panel_motion,
        &[Layer::Overlay],
        view,
        scale,
        specular,
    ));
    if let Some((buffer, place, look)) = panel.filter(|(_, place, _)| overlaps(*place, view)) {
        let look = look.translated(shift);
        let place_rect = place.translated(shift);
        let shown = look.rect(place_rect);
        let at = Scale::new(scale);
        let alpha = look.alpha;
        let (sx, sy) = look.scale();
        let radius = 14.0 * sx.min(sy).min(1.0);
        match painted_as(renderer, buffer, place_rect.size(), shown, scale, at, alpha) {
            Ok(element) => {
                elements.push(SceneElement::Texture(element));
                if glass.blur {
                    let shape = physical_rect(shown, scale);
                    let radii = [(radius * scale) as f32; 4];
                    elements.push(SceneElement::Glass(bar_glass.get(view).faded(
                        shaders,
                        shape,
                        output_size,
                        Mask::Rounded { radii },
                        Blur::new(glass.strength, scale),
                        alpha,
                    )));
                }
                let shadow = Shadow {
                    shape: shown,
                    offset: ModelPoint::new(0, 4),
                    blur: 18.0,
                    color: theme.shadow,
                    radii: [radius as f32; 4],
                    alpha,
                };
                if overlaps(shadow.area(), Rect::new(0, 0, view.w, view.h)) {
                    elements.push(SceneElement::Shape(
                        bar_shadow.get(view).element(&shaders.shadow, shadow),
                    ));
                }
            }
            Err(error) => warn!(?error, "cannot upload the panel of the compositor"),
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
                overlay_alpha,
            )));
            if shape.border == 0 && glass.blur {
                let cache = drop_glass.entry(global.outline).or_default();
                let rect = physical_rect(shape.outline, scale);
                let radii = shape.radii.map(|r| (f64::from(r) * scale) as f32);
                elements.push(SceneElement::Glass(cache.faded(
                    shaders,
                    rect,
                    output_size,
                    Mask::Rounded { radii },
                    Blur::new(glass.strength, scale),
                    overlay_alpha,
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
        &mut panel_motion,
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
        &mut panel_motion,
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
            match image(
                renderer,
                &buffer,
                location,
                pixels,
                size,
                Kind::Cursor,
                None,
            ) {
                Ok(element) => elements.push(SceneElement::Texture(element)),
                Err(error) => warn!(?error, "cannot upload the cursor"),
            }
        }
        None => {}
    }
}
