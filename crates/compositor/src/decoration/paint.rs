//! Painting title bars to pixels, with tiny-skia for shapes and cosmic-text
//! for text.

use coquille_model::{Rect, Size};
use cosmic_text::{
    Attrs, Buffer, Color as TextColor, Family, FontSystem, Metrics as TextMetrics, Shaping,
    SwashCache, Weight, Wrap,
};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, PremultipliedColorU8,
    Stroke, Transform,
};

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

/// Everything a title bar's look depends on: repaint only when it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitlebarSpec {
    pub size: Size,
    pub title: String,
    pub subtitle: String,
    pub badge: [u8; 3],
    pub focused: bool,
    pub hovered: Option<TitlebarButton>,
    pub show_swap: bool,
    pub theme: ThemeKind,
    pub corner_radius: i32,
    /// The windows stacked in the slot, when they are shown as tabs; they
    /// then replace the badge, title and subtitle.
    pub tabs: Vec<TabSpec>,
}

/// One tab of a stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSpec {
    pub label: String,
    pub badge: [u8; 3],
    pub active: bool,
}

/// What the placement bar's look depends on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarSpec {
    /// One per button; the first one is the default choice and stands out.
    pub labels: Vec<String>,
    pub hovered: Option<usize>,
    pub theme: ThemeKind,
}

/// A bar painted to pixels, with where its buttons ended up.
pub struct PaintedBar {
    pub pixmap: Pixmap,
    /// Relative to the bar's top-left corner, in the order of the labels.
    pub buttons: Vec<Rect>,
}

pub struct Painter {
    fonts: FontSystem,
    glyphs: SwashCache,
    has_ui_font: bool,
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
        }
    }

    /// Paints a title bar: rounded top corners, badge, title, subtitle,
    /// buttons. Returns `None` for an empty size.
    pub fn titlebar(&mut self, spec: &TitlebarSpec) -> Option<Pixmap> {
        let theme = Theme::new(spec.theme);
        let (width, height) = (spec.size.w as f32, spec.size.h as f32);
        let mut pixmap = Pixmap::new(spec.size.w.try_into().ok()?, spec.size.h.try_into().ok()?)?;
        let layout = TitlebarLayout::new(spec.size, spec.show_swap);

        let radius = spec.corner_radius as f32;
        let background = rounded_rect(0.0, 0.0, width, height, [radius, radius, 0.0, 0.0])?;
        fill(&mut pixmap, &background, theme.titlebar);
        if let Some(line) = rounded_rect(0.0, height - 1.0, width, 1.0, [0.0; 4]) {
            fill(&mut pixmap, &line, theme.separator);
        }

        if spec.tabs.is_empty() {
            let badge = layout.icon;
            if let Some(path) = rounded_rect(
                badge.x as f32,
                badge.y as f32,
                badge.w as f32,
                badge.h as f32,
                [BADGE_RADIUS; 4],
            ) {
                fill(&mut pixmap, &path, Rgba::from_array(spec.badge));
            }
            self.texts(&mut pixmap, spec, &theme, layout.text);
        } else {
            let cells = layout.tabs(spec.tabs.len());
            for (tab, cell) in spec.tabs.iter().zip(cells) {
                self.tab(&mut pixmap, tab, cell, spec.focused, &theme);
            }
        }

        for (button, cell) in &layout.buttons {
            let hovered = spec.hovered == Some(*button);
            let mut color = match button {
                TitlebarButton::Close | TitlebarButton::Swap => theme.text,
                // Not wired yet: shown, but visibly unavailable.
                TitlebarButton::AddTab | TitlebarButton::Minimize => theme.disabled,
            };
            if hovered && *button == TitlebarButton::Close {
                let highlight = rounded_rect(
                    cell.x as f32 + 4.0,
                    5.0,
                    cell.w as f32 - 8.0,
                    height - 10.0,
                    [7.0; 4],
                );
                if let Some(highlight) = highlight {
                    fill(&mut pixmap, &highlight, theme.close_hover);
                }
                color = Rgba::WHITE;
            } else if hovered {
                let highlight = rounded_rect(
                    cell.x as f32 + 4.0,
                    5.0,
                    cell.w as f32 - 8.0,
                    height - 10.0,
                    [7.0; 4],
                );
                if let Some(highlight) = highlight {
                    fill(&mut pixmap, &highlight, theme.button_hover);
                }
            }
            if let Some(icon) = button_icon(*button, *cell) {
                stroke(&mut pixmap, &icon, color);
            }
        }
        Some(pixmap)
    }

    /// Draws one tab: a small badge and a label, on a pill when it is the
    /// shown one.
    fn tab(
        &mut self,
        pixmap: &mut Pixmap,
        tab: &TabSpec,
        cell: Rect,
        focused: bool,
        theme: &Theme,
    ) {
        const PADDING: i32 = 8;
        const BADGE: i32 = 14;
        const LABEL_LEFT: i32 = PADDING + BADGE + 6;
        if cell.w <= LABEL_LEFT {
            return;
        }
        let (x, y) = (cell.x as f32, cell.y as f32);
        if tab.active {
            let pill = rounded_rect(x, y, cell.w as f32, cell.h as f32, [8.0; 4]);
            if let Some(pill) = pill {
                fill(pixmap, &pill, theme.button_hover);
            }
        }
        let badge = rounded_rect(
            x + PADDING as f32,
            y + (cell.h - BADGE) as f32 / 2.0,
            BADGE as f32,
            BADGE as f32,
            [4.0; 4],
        );
        if let Some(badge) = badge {
            fill(pixmap, &badge, Rgba::from_array(tab.badge));
        }

        let (weight, color) = match (tab.active, focused) {
            (true, true) => (Weight::SEMIBOLD, theme.text),
            (true, false) => (Weight::SEMIBOLD, theme.text_muted),
            (false, _) => (Weight::NORMAL, theme.text_muted),
        };
        let left = (cell.x + LABEL_LEFT) as f32;
        let room = (cell.w - LABEL_LEFT - PADDING) as f32;
        self.text(pixmap, &tab.label, left, room, cell, weight, color);
    }

    /// Paints the placement bar. Returns `None` when it has no button.
    pub fn placement_bar(&mut self, spec: &BarSpec) -> Option<PaintedBar> {
        const HEIGHT: i32 = 40;
        const PADDING: i32 = 6;
        const BUTTON_PADDING: i32 = 12;
        const GAP: i32 = 4;
        if spec.labels.is_empty() {
            return None;
        }
        let theme = Theme::new(spec.theme);

        let widths: Vec<i32> = spec
            .labels
            .iter()
            .map(|label| self.shaped(label, Weight::MEDIUM).1.ceil() as i32 + 2 * BUTTON_PADDING)
            .collect();
        let gaps = GAP * (widths.len() as i32 - 1);
        let width = 2 * PADDING + widths.iter().sum::<i32>() + gaps;
        let mut pixmap = Pixmap::new(width.try_into().ok()?, HEIGHT as u32)?;

        // A hairline around the bar keeps it apart from what is behind.
        let (w, h) = (width as f32, HEIGHT as f32);
        fill(
            &mut pixmap,
            &rounded_rect(0.0, 0.0, w, h, [14.0; 4])?,
            theme.separator,
        );
        let inside = rounded_rect(1.0, 1.0, w - 2.0, h - 2.0, [13.0; 4])?;
        fill(&mut pixmap, &inside, theme.titlebar);

        let mut buttons = Vec::new();
        let mut x = PADDING;
        for (index, (label, width)) in spec.labels.iter().zip(widths).enumerate() {
            let cell = Rect::new(x, PADDING, width, HEIGHT - 2 * PADDING);
            // The first choice is the default one: it stands out.
            let (background, ink) = match (index, spec.hovered == Some(index)) {
                (0, _) => (Some(theme.accent), Rgba::WHITE),
                (_, true) => (Some(theme.button_hover), theme.text),
                (_, false) => (None, theme.text),
            };
            let pill = rounded_rect(
                x as f32,
                PADDING as f32,
                width as f32,
                cell.h as f32,
                [8.0; 4],
            );
            if let (Some(background), Some(pill)) = (background, pill) {
                fill(&mut pixmap, &pill, background);
            }
            let left = (x + BUTTON_PADDING) as f32;
            let room = (width - BUTTON_PADDING) as f32;
            self.text(&mut pixmap, label, left, room, cell, Weight::MEDIUM, ink);
            buttons.push(cell);
            x += width + GAP;
        }
        Some(PaintedBar { pixmap, buttons })
    }

    fn texts(&mut self, pixmap: &mut Pixmap, spec: &TitlebarSpec, theme: &Theme, area: Rect) {
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
            );
        }
    }

    /// Lays out one line of text and measures it.
    fn shaped(&mut self, text: &str, weight: Weight) -> (Buffer, f32) {
        let family = if self.has_ui_font {
            Family::Name(UI_FONT)
        } else {
            Family::SansSerif
        };
        let attrs = Attrs::new().family(family).weight(weight);

        let mut buffer = Buffer::new(&mut self.fonts, TextMetrics::new(FONT_SIZE, LINE_HEIGHT));
        buffer.set_wrap(Wrap::None);
        buffer.set_size(None, Some(LINE_HEIGHT));
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.fonts, false);
        let width = buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0, f32::max);
        (buffer, width)
    }

    /// Draws one line of text, cut at `room` pixels. Returns the width used.
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
    ) -> f32 {
        if text.is_empty() || room <= 0.0 {
            return 0.0;
        }
        let (mut buffer, width) = self.shaped(text, weight);

        let origin_x = left.round() as i32;
        let origin_y = area.y + ((area.h as f32 - LINE_HEIGHT) / 2.0).round() as i32;
        let clip_right = (left + room).floor() as i32;
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

fn fill(pixmap: &mut Pixmap, path: &Path, color: Rgba) {
    pixmap.fill_path(
        path,
        &paint(color),
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

fn stroke(pixmap: &mut Pixmap, path: &Path, color: Rgba) {
    let stroke = Stroke {
        width: ICON_STROKE,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    pixmap.stroke_path(path, &paint(color), &stroke, Transform::identity(), None);
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

/// The line icon of a button, centered in its cell.
fn button_icon(button: TitlebarButton, cell: Rect) -> Option<Path> {
    let cx = cell.x as f32 + cell.w as f32 / 2.0;
    let cy = cell.y as f32 + cell.h as f32 / 2.0;
    let mut path = PathBuilder::new();
    let mut line = |from: (f32, f32), to: (f32, f32)| {
        path.move_to(cx + from.0, cy + from.1);
        path.line_to(cx + to.0, cy + to.1);
    };
    match button {
        TitlebarButton::Close => {
            line((-5.0, -5.0), (5.0, 5.0));
            line((5.0, -5.0), (-5.0, 5.0));
        }
        TitlebarButton::Minimize => line((-5.5, 0.0), (5.5, 0.0)),
        TitlebarButton::Swap => {
            // Two arrows: the upper one points right, the lower one left.
            line((-6.0, -3.0), (6.0, -3.0));
            line((3.0, -6.0), (6.0, -3.0));
            line((6.0, -3.0), (3.0, 0.0));
            line((6.0, 3.0), (-6.0, 3.0));
            line((-3.0, 0.0), (-6.0, 3.0));
            line((-6.0, 3.0), (-3.0, 6.0));
        }
        TitlebarButton::AddTab => {
            // A tab outline with a plus sign inside.
            line((-6.5, -5.5), (-1.5, -5.5));
            line((-1.5, -5.5), (0.5, -3.5));
            line((0.5, -3.5), (6.5, -3.5));
            line((6.5, -3.5), (6.5, 5.5));
            line((6.5, 5.5), (-6.5, 5.5));
            line((-6.5, 5.5), (-6.5, -5.5));
            line((0.0, -1.0), (0.0, 3.5));
            line((-2.25, 1.25), (2.25, 1.25));
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

        fill(&mut pixmap, &path, Rgba::WHITE);

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
    fn the_bar_has_one_button_per_label_side_by_side_inside_it() {
        let spec = BarSpec {
            labels: vec![
                "Laisser flottante".into(),
                "Ranger ici".into(),
                "Au centre".into(),
            ],
            hovered: Some(1),
            theme: ThemeKind::Light,
        };

        let bar = Painter::new().placement_bar(&spec).unwrap();

        let (width, height) = (bar.pixmap.width() as i32, bar.pixmap.height() as i32);
        assert_eq!(bar.buttons.len(), 3);
        let mut previous_right = 0;
        for button in &bar.buttons {
            assert!(
                button.x >= previous_right,
                "buttons overlap: {:?}",
                bar.buttons
            );
            assert!(button.w > 0 && button.right() <= width);
            assert!(button.y >= 0 && button.bottom() <= height);
            previous_right = button.right();
        }
        // A longer label makes a wider button.
        assert!(bar.buttons[0].w > bar.buttons[2].w);
    }

    #[test]
    fn a_bar_without_label_is_not_painted() {
        let spec = BarSpec {
            labels: Vec::new(),
            hovered: None,
            theme: ThemeKind::Dark,
        };

        assert!(Painter::new().placement_bar(&spec).is_none());
    }

    #[test]
    fn every_button_has_an_icon_inside_its_cell() {
        let cell = Rect::new(100, 0, 38, 38);
        for button in [
            TitlebarButton::AddTab,
            TitlebarButton::Swap,
            TitlebarButton::Minimize,
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
}
