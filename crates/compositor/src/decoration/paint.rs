//! Painting title bars and capsules to pixels, with tiny-skia for shapes and
//! cosmic-text for text.

use coquille_model::{Rect, Scale, Size};
use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics as TextMetrics, Shaping,
    SwashCache, Weight, Wrap,
};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, PixmapPaint,
    PremultipliedColorU8, Stroke, Transform,
};

use super::capsules::{CapsuleButton, CapsuleCell, GroupLayout};
use super::icons::IconCache;
use super::layout::{TitlebarButton, TitlebarLayout};
use crate::theme::{Rgba, Theme, ThemeKind};

const UI_FONT: &str = "Figtree";
const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 18.0;
/// Space between the title and the subtitle.
const SUBTITLE_GAP: f32 = 10.0;
/// A subtitle squeezed below this width is not worth showing.
const SUBTITLE_MIN_WIDTH: f32 = 24.0;
const BADGE_RADIUS: f32 = 5.0;
const ICON_STROKE: f32 = 1.4;
/// Width of the accent ring around the capsule of the active window.
const RING_WIDTH: f32 = 2.0;

/// Everything a title bar's look depends on: repaint only when it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitlebarSpec {
    pub size: Size,
    pub title: String,
    pub subtitle: String,
    /// The icon of the application, as its desktop entry names it.
    pub icon: Option<String>,
    /// The color that stands for the application when it has no icon.
    pub badge: [u8; 3],
    pub focused: bool,
    pub hovered: Option<TitlebarButton>,
    /// Whether the bar has its buttons.
    pub buttons: bool,
    pub theme: ThemeKind,
    pub accent: Rgba,
    pub corner_radius: i32,
    /// The scale of its screen: painted that many times larger.
    pub scale: Scale,
}

/// Everything the look of a group of capsules depends on: repaint only when
/// it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSpec {
    /// Size of the band the group sits in.
    pub size: Size,
    pub capsules: Vec<CapsuleSpec>,
    /// The capsule under the pointer, and its button there if any.
    pub hovered: Option<(usize, Option<CapsuleButton>)>,
    pub theme: ThemeKind,
    pub accent: Rgba,
    /// The group of the maximized windows of a screen: its buttons restore
    /// them instead of maximizing.
    pub maximized: bool,
    /// The box of the group of maximized windows in the same band, relative
    /// to it: this group keeps clear of it.
    pub avoid: Option<Rect>,
    /// The scale of its screen: painted that many times larger.
    pub scale: Scale,
}

/// One window of a stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsuleSpec {
    pub label: String,
    pub icon: Option<String>,
    pub badge: [u8; 3],
    /// Its slot shows it.
    pub shown: bool,
    pub focused: bool,
    /// Hidden by the user: shown dimmed, a click brings it back.
    pub minimized: bool,
}

impl CapsuleSpec {
    fn weight(&self) -> Weight {
        if self.shown {
            Weight::SEMIBOLD
        } else {
            Weight::NORMAL
        }
    }
}

/// A group painted to pixels, as large as its band (times the scale), with
/// where its capsules ended up.
pub struct PaintedGroup {
    pub pixmap: Pixmap,
    pub layout: GroupLayout,
}

pub struct Painter {
    fonts: FontSystem,
    glyphs: SwashCache,
    has_ui_font: bool,
    icons: IconCache,
}

impl Painter {
    /// Loads the system fonts, which takes a moment: create once.
    pub fn new() -> Self {
        let mut fonts = FontSystem::new();
        let has_ui_font = fonts
            .db_mut()
            .faces()
            .any(|face| face.families.iter().any(|(name, _)| name == UI_FONT));
        if !has_ui_font {
            tracing::info!("font {UI_FONT} not found, using the system sans-serif");
        }
        Self {
            fonts,
            glyphs: SwashCache::new(),
            has_ui_font,
            icons: IconCache::default(),
        }
    }

    /// Paints a title bar: rounded top corners, badge, title, subtitle,
    /// buttons. Returns `None` for an empty size.
    pub fn titlebar(&mut self, spec: &TitlebarSpec) -> Option<Pixmap> {
        let theme = Theme::new(spec.theme).with_accent(spec.accent);
        self.icons.follow(spec.theme, spec.accent);
        let (width, height) = (spec.size.w as f32, spec.size.h as f32);
        let s = spec.scale.get() as f32;
        let mut pixmap = canvas(spec.size, spec.scale)?;
        let layout = TitlebarLayout::new(spec.size, spec.buttons);

        let radius = spec.corner_radius as f32;
        let background = rounded_rect(0.0, 0.0, width, height, [radius, radius, 0.0, 0.0])?;
        fill(&mut pixmap, &background, theme.glass, s);
        if let Some(rim) = titlebar_rim(width, height, radius) {
            outline(&mut pixmap, &rim, theme.rim, 1.0, s);
        }
        if let Some(line) = rounded_rect(0.0, height - 1.0, width, 1.0, [0.0; 4]) {
            fill(&mut pixmap, &line, theme.separator, s);
        }

        let place = layout.icon;
        self.app_mark(
            &mut pixmap,
            spec.icon.as_deref(),
            spec.badge,
            place,
            BADGE_RADIUS,
            1.0,
            s,
        );
        self.texts(&mut pixmap, spec, &theme, layout.text, s);

        for (button, cell) in &layout.buttons {
            let hovered = spec.hovered == Some(*button);
            let highlight = match (button, hovered) {
                (TitlebarButton::Close, true) => Some(theme.close_hover),
                (_, true) => Some(theme.hover),
                (_, false) => None,
            };
            let pill = rounded_rect(
                cell.x as f32 + 4.0,
                5.0,
                cell.w as f32 - 8.0,
                height - 10.0,
                [7.0; 4],
            );
            if let (Some(highlight), Some(pill)) = (highlight, pill) {
                fill(&mut pixmap, &pill, highlight, s);
            }
            // White on the colored highlight, the text color otherwise.
            let on_color = hovered && *button == TitlebarButton::Close;
            let color = if on_color { Rgba::WHITE } else { theme.text };
            if let Some(icon) = button_icon(*button, *cell) {
                stroke(&mut pixmap, &icon, color, s);
            }
        }
        Some(pixmap)
    }

    /// Draws what stands for an application in `place`: its icon, or a
    /// rounded square of its color when it has none.
    #[allow(clippy::too_many_arguments)]
    fn app_mark(
        &mut self,
        pixmap: &mut Pixmap,
        icon: Option<&str>,
        badge: [u8; 3],
        place: Rect,
        radius: f32,
        opacity: f32,
        s: f32,
    ) {
        // Icons are drawn at the size of their pixels, never stretched.
        let size = (place.w.min(place.h) as f32 * s).round();
        let size = u32::try_from(size as i64).unwrap_or(0);
        if let Some(icon) = icon.and_then(|name| self.icons.get(name, size)) {
            let paint = PixmapPaint {
                opacity,
                ..PixmapPaint::default()
            };
            pixmap.draw_pixmap(
                (place.x as f32 * s).round() as i32,
                (place.y as f32 * s).round() as i32,
                icon.as_ref(),
                &paint,
                Transform::identity(),
                None,
            );
            return;
        }
        let (x, y, w, h) = (
            place.x as f32,
            place.y as f32,
            place.w as f32,
            place.h as f32,
        );
        if let Some(square) = rounded_rect(x, y, w, h, [radius; 4]) {
            fill(pixmap, &square, Rgba::from_array(badge), s);
        }
    }

    /// Paints the capsules of a slot: a rounded box, and in it one capsule
    /// per window. Returns `None` when there is nothing to show.
    pub fn capsule_group(&mut self, spec: &GroupSpec) -> Option<PaintedGroup> {
        let theme = Theme::new(spec.theme).with_accent(spec.accent);
        self.icons.follow(spec.theme, spec.accent);
        let label_widths: Vec<i32> = spec
            .capsules
            .iter()
            .map(|capsule| self.shaped(&capsule.label, capsule.weight(), 1.0).1.ceil() as i32)
            .collect();
        let layout = match spec.avoid {
            Some(avoid) => GroupLayout::beside(spec.size, &label_widths, avoid),
            None => GroupLayout::new(spec.size, &label_widths),
        };
        if layout.capsules.is_empty() {
            return None;
        }
        let s = spec.scale.get() as f32;
        let mut pixmap = canvas(spec.size, spec.scale)?;

        if let Some(frame_path) = pill(layout.rect, 0.0) {
            fill(&mut pixmap, &frame_path, theme.glass, s);
        }
        if let Some(rim_path) = pill(layout.rect, 0.5) {
            outline(&mut pixmap, &rim_path, theme.rim, 1.0, s);
        }
        for (index, (capsule, cell)) in spec.capsules.iter().zip(&layout.capsules).enumerate() {
            let hover = spec.hovered.filter(|(under, _)| *under == index);
            self.capsule(
                &mut pixmap,
                capsule,
                cell,
                hover.map(|(_, button)| button),
                spec.maximized,
                &theme,
                s,
            );
        }
        Some(PaintedGroup { pixmap, layout })
    }

    /// Draws one capsule: its icon and its name, on a pill when its slot
    /// shows it, with its buttons when it is shown or under the pointer.
    #[allow(clippy::too_many_arguments)]
    fn capsule(
        &mut self,
        pixmap: &mut Pixmap,
        capsule: &CapsuleSpec,
        cell: &CapsuleCell,
        hover: Option<Option<CapsuleButton>>,
        maximized: bool,
        theme: &Theme,
        s: f32,
    ) {
        // The capsule is under the pointer, on this button of it if any.
        let (hovered, hovered_button) = (hover.is_some(), hover.flatten());
        let background = match (capsule.shown, hovered) {
            (true, _) => Some(theme.glass_strong),
            (false, true) => Some(theme.hover),
            (false, false) => None,
        };
        if let (Some(background), Some(path)) = (background, pill(cell.rect, 0.0)) {
            fill(pixmap, &path, background, s);
        }
        if capsule.shown && capsule.focused {
            // Inside the capsule, so that the box keeps its margin.
            if let Some(path) = pill(cell.rect, RING_WIDTH / 2.0) {
                outline(pixmap, &path, theme.accent, RING_WIDTH, s);
            }
        }

        // A hidden window shows dimmed, its icon with the rest.
        let opacity = if capsule.minimized { 0.45 } else { 1.0 };
        self.app_mark(
            pixmap,
            capsule.icon.as_deref(),
            capsule.badge,
            cell.icon,
            4.0,
            opacity,
            s,
        );

        let room = cell.label;
        let color = match (capsule.minimized, capsule.shown) {
            (true, _) => theme.disabled,
            (false, true) => theme.text,
            (false, false) => theme.text_muted,
        };
        self.text(
            pixmap,
            &capsule.label,
            room.x as f32,
            room.w as f32,
            room,
            capsule.weight(),
            color,
            s,
        );

        if !(capsule.shown || hovered) {
            return;
        }
        for (button, place) in &cell.buttons {
            let under = hovered_button == Some(*button);
            let highlight = match (button, under) {
                (CapsuleButton::Close, true) => Some(theme.close_hover),
                (_, true) => Some(theme.hover),
                (_, false) => None,
            };
            if let (Some(highlight), Some(path)) = (highlight, pill(*place, 0.0)) {
                fill(pixmap, &path, highlight, s);
            }
            // White on the colored highlight, the text color otherwise.
            let on_color = under && *button == CapsuleButton::Close;
            let color = if on_color { Rgba::WHITE } else { theme.text };
            let glyph = match button {
                CapsuleButton::Minimize => Glyph::Dash,
                CapsuleButton::Maximize if maximized => Glyph::Restore,
                CapsuleButton::Maximize => Glyph::Square,
                CapsuleButton::Close => Glyph::Cross,
            };
            if let Some(icon) = line_icon(glyph, *place, 3.5) {
                stroke(pixmap, &icon, color, s);
            }
        }
    }

    fn texts(
        &mut self,
        pixmap: &mut Pixmap,
        spec: &TitlebarSpec,
        theme: &Theme,
        area: Rect,
        s: f32,
    ) {
        let title_color = if spec.focused {
            theme.text
        } else {
            theme.text_muted
        };
        let left = area.x as f32;
        let room = area.w as f32;
        let used = self.text(
            pixmap,
            &spec.title,
            left,
            room,
            area,
            Weight::SEMIBOLD,
            title_color,
            s,
        );

        let rest = room - used - SUBTITLE_GAP;
        if !spec.subtitle.is_empty() && rest >= SUBTITLE_MIN_WIDTH {
            let left = left + used + SUBTITLE_GAP;
            self.text(
                pixmap,
                &spec.subtitle,
                left,
                rest,
                area,
                Weight::NORMAL,
                theme.text_muted,
                s,
            );
        }
    }

    /// Lays out one line of text, `s` times larger, and measures it.
    fn shaped(&mut self, text: &str, weight: Weight, s: f32) -> (Buffer, f32) {
        let family = if self.has_ui_font {
            Family::Name(UI_FONT)
        } else {
            Family::SansSerif
        };
        let attrs = Attrs::new().family(family).weight(weight);

        let metrics = TextMetrics::new(FONT_SIZE * s, LINE_HEIGHT * s);
        let mut buffer = Buffer::new(&mut self.fonts, metrics);
        buffer.set_wrap(Wrap::None);
        buffer.set_size(None, Some(LINE_HEIGHT * s));
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fonts, false);
        let width = buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0, f32::max);
        (buffer, width)
    }

    /// Draws one line of text, cut at `room` pixels (logical, like every
    /// place given), on a pixmap `s` times larger. Returns the width used.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        left: f32,
        room: f32,
        area: Rect,
        weight: Weight,
        color: Rgba,
        s: f32,
    ) -> f32 {
        if text.is_empty() || room <= 0.0 {
            return 0.0;
        }
        let (mut buffer, width) = self.shaped(text, weight, s);
        let width = width / s;

        let origin_x = (left * s).round() as i32;
        let top = area.y as f32 + ((area.h as f32 - LINE_HEIGHT) / 2.0).round();
        let origin_y = (top * s).round() as i32;
        let clip_right = ((left + room) * s).floor() as i32;
        let pixmap_width = pixmap.width() as i32;
        let pixmap_height = pixmap.height() as i32;
        let ink = TextColor::rgba(color.r, color.g, color.b, color.a);
        buffer.draw(
            &mut self.fonts,
            &mut self.glyphs,
            ink,
            |x, y, w, h, color| {
                let [r, g, b, a] = color.as_rgba();
                if a == 0 {
                    return;
                }
                for dy in 0..h as i32 {
                    for dx in 0..w as i32 {
                        let (px, py) = (origin_x + x + dx, origin_y + y + dy);
                        let inside = px >= 0 && px < pixmap_width.min(clip_right);
                        if inside && py >= 0 && py < pixmap_height {
                            let index = (py * pixmap_width + px) as usize;
                            blend(&mut pixmap.pixels_mut()[index], [r, g, b, a]);
                        }
                    }
                }
            },
        );
        width.min(room)
    }
}

/// Source-over of a straight color onto a premultiplied pixel.
fn blend(pixel: &mut PremultipliedColorU8, [r, g, b, a]: [u8; 4]) {
    let alpha = u32::from(a);
    let over = |source: u8, under: u8| {
        ((u32::from(source) * alpha + u32::from(under) * (255 - alpha) + 127) / 255) as u8
    };
    let blended = PremultipliedColorU8::from_rgba(
        over(r, pixel.red()),
        over(g, pixel.green()),
        over(b, pixel.blue()),
        over(255, pixel.alpha()),
    );
    // Each channel is at most the new alpha, so the color is always valid.
    if let Some(blended) = blended {
        *pixel = blended;
    }
}

fn paint(color: Rgba) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(color.r, color.g, color.b, color.a));
    paint.anti_alias = true;
    paint
}

/// A transparent pixmap for something of logical `size`: as many pixels as
/// its screen shows it with. Everything is then drawn in logical units,
/// `s` times larger (`fill`, `stroke`, `outline`, `text`).
fn canvas(size: Size, scale: Scale) -> Option<Pixmap> {
    let pixels = scale.physical(size);
    Pixmap::new(pixels.w.try_into().ok()?, pixels.h.try_into().ok()?)
}

fn fill(pixmap: &mut Pixmap, path: &Path, color: Rgba, s: f32) {
    pixmap.fill_path(
        path,
        &paint(color),
        FillRule::Winding,
        Transform::from_scale(s, s),
        None,
    );
}

fn stroke(pixmap: &mut Pixmap, path: &Path, color: Rgba, s: f32) {
    outline(pixmap, path, color, ICON_STROKE, s);
}

fn outline(pixmap: &mut Pixmap, path: &Path, color: Rgba, width: f32, s: f32) {
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(
        path,
        &paint(color),
        &stroke,
        Transform::from_scale(s, s),
        None,
    );
}

/// A rectangle with fully rounded ends, drawn `inset` pixels inside `rect`.
fn pill(rect: Rect, inset: f32) -> Option<Path> {
    let (w, h) = (rect.w as f32 - 2.0 * inset, rect.h as f32 - 2.0 * inset);
    let (x, y) = (rect.x as f32 + inset, rect.y as f32 + inset);
    rounded_rect(x, y, w, h, [h / 2.0; 4])
}

/// A rectangle with one radius per corner: top-left, top-right, bottom-right,
/// bottom-left.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) -> Option<Path> {
    // Control-point distance that makes a cubic Bézier look like a quarter circle.
    const KAPPA: f32 = 0.552_284_8;
    let limit = (w.min(h) / 2.0).max(0.0);
    let [tl, tr, br, bl] = radii.map(|radius| radius.clamp(0.0, limit));
    let (right, bottom) = (x + w, y + h);

    let mut path = PathBuilder::new();
    path.move_to(x + tl, y);
    path.line_to(right - tr, y);
    path.cubic_to(
        right - tr + tr * KAPPA,
        y,
        right,
        y + tr - tr * KAPPA,
        right,
        y + tr,
    );
    path.line_to(right, bottom - br);
    path.cubic_to(
        right,
        bottom - br + br * KAPPA,
        right - br + br * KAPPA,
        bottom,
        right - br,
        bottom,
    );
    path.line_to(x + bl, bottom);
    path.cubic_to(
        x + bl - bl * KAPPA,
        bottom,
        x,
        bottom - bl + bl * KAPPA,
        x,
        bottom - bl,
    );
    path.line_to(x, y + tl);
    path.cubic_to(x, y + tl - tl * KAPPA, x + tl - tl * KAPPA, y, x + tl, y);
    path.close();
    path.finish()
}

/// An open path tracing the top edge of a titlebar and its rounded corners.
fn titlebar_rim(w: f32, _h: f32, radius: f32) -> Option<Path> {
    const KAPPA: f32 = 0.552_284_8;
    let r = radius.clamp(0.0, w / 2.0);
    let mut builder = PathBuilder::new();
    let y = 0.5;
    builder.move_to(0.5, r);
    builder.cubic_to(0.5, r - r * KAPPA, r - r * KAPPA, y, r, y);
    builder.line_to(w - 0.5 - r, y);
    builder.cubic_to(
        w - 0.5 - r + r * KAPPA,
        y,
        w - 0.5,
        r - r * KAPPA,
        w - 0.5,
        r,
    );
    builder.finish()
}

/// The line icon of a title bar button, centered in its cell.
fn button_icon(button: TitlebarButton, cell: Rect) -> Option<Path> {
    let glyph = match button {
        TitlebarButton::Minimize => Glyph::Dash,
        TitlebarButton::Maximize => Glyph::Square,
        TitlebarButton::Close => Glyph::Cross,
    };
    line_icon(glyph, cell, 5.0)
}

/// What a button shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Glyph {
    /// Minimize.
    Dash,
    /// Maximize.
    Square,
    /// Put a maximized window back: two squares, one behind the other.
    Restore,
    /// Close.
    Cross,
}

/// The line drawing of `glyph`, `reach` pixels around the center of `cell`.
fn line_icon(glyph: Glyph, cell: Rect, reach: f32) -> Option<Path> {
    let cx = cell.x as f32 + cell.w as f32 / 2.0;
    let cy = cell.y as f32 + cell.h as f32 / 2.0;
    let mut path = PathBuilder::new();
    let mut lines = |points: &[(f32, f32)]| {
        for (index, (x, y)) in points.iter().enumerate() {
            if index == 0 {
                path.move_to(cx + x, cy + y);
            } else {
                path.line_to(cx + x, cy + y);
            }
        }
    };
    let r = reach;
    match glyph {
        Glyph::Dash => lines(&[(-r - 0.5, 0.0), (r + 0.5, 0.0)]),
        Glyph::Cross => {
            lines(&[(-r, -r), (r, r)]);
            lines(&[(r, -r), (-r, r)]);
        }
        Glyph::Square => lines(&[(-r, -r), (r, -r), (r, r), (-r, r), (-r, -r)]),
        Glyph::Restore => {
            // The front square, low on the left, and what shows of the one
            // behind it, high on the right.
            let s = (r * 0.5).round();
            lines(&[
                (-r, -r + s),
                (r - s, -r + s),
                (r - s, r),
                (-r, r),
                (-r, -r + s),
            ]);
            lines(&[
                (-r + s, -r + s),
                (-r + s, -r),
                (r, -r),
                (r, r - s),
                (r - s, r - s),
            ]);
        }
    }
    path.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixmap: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let color = pixmap.pixel(x, y).unwrap();
        [color.red(), color.green(), color.blue(), color.alpha()]
    }

    #[test]
    fn the_top_corners_are_cut_and_the_bottom_ones_are_square() {
        let path = rounded_rect(0.0, 0.0, 100.0, 38.0, [12.0, 12.0, 0.0, 0.0]).unwrap();
        let mut pixmap = Pixmap::new(100, 38).unwrap();

        fill(&mut pixmap, &path, Rgba::WHITE, 1.0);

        assert_eq!(pixel(&pixmap, 0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&pixmap, 99, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&pixmap, 0, 37), [255, 255, 255, 255]);
        assert_eq!(pixel(&pixmap, 99, 37), [255, 255, 255, 255]);
        assert_eq!(pixel(&pixmap, 50, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn a_radius_larger_than_the_shape_is_reduced_to_fit() {
        let path = rounded_rect(0.0, 0.0, 10.0, 10.0, [50.0; 4]).unwrap();

        let bounds = path.bounds();
        assert_eq!((bounds.width(), bounds.height()), (10.0, 10.0));
    }

    #[test]
    fn blending_an_opaque_color_replaces_the_pixel() {
        let mut pixel = PremultipliedColorU8::from_rgba(10, 20, 30, 255).unwrap();

        blend(&mut pixel, [200, 100, 50, 255]);

        assert_eq!(
            [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()],
            [200, 100, 50, 255]
        );
    }

    #[test]
    fn blending_a_transparent_color_keeps_the_pixel() {
        let mut pixel = PremultipliedColorU8::from_rgba(10, 20, 30, 255).unwrap();

        blend(&mut pixel, [200, 100, 50, 0]);

        assert_eq!(
            [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()],
            [10, 20, 30, 255]
        );
    }

    #[test]
    fn blending_half_way_mixes_both_colors() {
        let mut pixel = PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();

        blend(&mut pixel, [255, 255, 255, 128]);

        assert_eq!(
            [pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()],
            [128, 128, 128, 255]
        );
    }

    #[test]
    fn every_button_has_an_icon_inside_its_cell() {
        let cell = Rect::new(100, 0, 38, 38);
        for button in [
            TitlebarButton::Minimize,
            TitlebarButton::Maximize,
            TitlebarButton::Close,
        ] {
            let bounds = button_icon(button, cell).unwrap().bounds();

            assert!(
                bounds.left() >= 100.0 && bounds.right() <= 138.0,
                "{button:?}"
            );
            assert!(bounds.top() >= 0.0 && bounds.bottom() <= 38.0, "{button:?}");
        }
    }

    fn group(capsules: Vec<CapsuleSpec>) -> GroupSpec {
        GroupSpec {
            size: Size::new(600, 32),
            capsules,
            hovered: None,
            theme: ThemeKind::Light,
            accent: Theme::new(ThemeKind::Light).accent,
            maximized: false,
            avoid: None,
            scale: Scale::ONE,
        }
    }

    fn capsule(label: &str, shown: bool) -> CapsuleSpec {
        CapsuleSpec {
            label: label.to_owned(),
            icon: None,
            badge: [0x2f, 0x6f, 0xed],
            shown,
            focused: false,
            minimized: false,
        }
    }

    #[test]
    fn a_group_is_painted_as_large_as_its_band_and_clear_around_its_box() {
        let spec = group(vec![capsule("kitty", true), capsule("Discord", false)]);

        let painted = Painter::new().capsule_group(&spec).unwrap();

        assert_eq!((painted.pixmap.width(), painted.pixmap.height()), (600, 32));
        assert_eq!(painted.layout.capsules.len(), 2);
        assert_eq!(pixel(&painted.pixmap, 2, 16), [0, 0, 0, 0]);
        let middle = painted.layout.rect.x + painted.layout.rect.w / 2;
        assert_ne!(pixel(&painted.pixmap, middle as u32, 1)[3], 0);
    }

    #[test]
    fn the_capsule_of_the_active_window_is_ringed_with_the_accent() {
        let mut spec = group(vec![capsule("kitty", true)]);
        spec.capsules[0].focused = true;

        let painted = Painter::new().capsule_group(&spec).unwrap();

        let cell = painted.layout.capsules[0].rect;
        let top = pixel(&painted.pixmap, (cell.x + cell.w / 2) as u32, cell.y as u32);
        assert_eq!(&top[..3], &[0x5b, 0x63, 0xe6]);
    }

    #[test]
    fn at_twice_the_scale_a_group_has_twice_the_pixels_and_the_same_layout() {
        let spec = group(vec![capsule("kitty", true), capsule("Discord", false)]);
        let doubled = GroupSpec {
            scale: Scale::new(2.0),
            ..spec.clone()
        };
        let mut painter = Painter::new();

        let single = painter.capsule_group(&spec).unwrap();
        let double = painter.capsule_group(&doubled).unwrap();

        assert_eq!((double.pixmap.width(), double.pixmap.height()), (1200, 64));
        assert_eq!(double.layout, single.layout);
        let middle = single.layout.rect.x + single.layout.rect.w / 2;
        assert_ne!(pixel(&double.pixmap, middle as u32 * 2, 2)[3], 0);
        assert_eq!(pixel(&double.pixmap, 4, 32), [0, 0, 0, 0]);
    }

    #[test]
    fn a_group_without_capsule_is_not_painted() {
        assert!(Painter::new().capsule_group(&group(Vec::new())).is_none());
    }
}
