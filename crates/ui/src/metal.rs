//! The gold of the shell as iced draws it: not a flat color but a metal,
//! lit from above (see `docs/gold/`). Every ramp runs from the top of its
//! piece to its bottom, and from nowhere else.
//!
//! iced fills a background with a ramp, but neither a border nor an inner
//! shadow. So the bevel of an ingot is part of its ramp, with its two
//! flanks as lines of 1 px beside what it carries; and a frame is a box
//! filled with the ramp of its hairline, around a box 1 px smaller.

use std::f32::consts::PI;

use iced::gradient::{ColorStop, Linear};
use iced::widget::{Row, Stack, container, row, slider, space, stack};
use iced::{
    Background, Border, Color, Element, Gradient, Length, Padding, Radians, Shadow, border,
};

use crate::style::{Accent, Palette, alpha};

/// One color of a ramp, and how far down the piece it sits, from 0 to 1.
pub type Stop = (f32, Color);

/// Two colors this close to each other make a sharp break.
const BREAK: f32 = 0.0005;

/// The bevel of an ingot, laid over its ramp: a bright edge of 1 px along
/// the top, a dark foot of 2 px, and a flank of 1 px on each side, the left
/// one lit.
const INGOT_EDGE: Color = Color::from_rgba8(255, 251, 232, 0.9);
const INGOT_FOOT: Color = Color::from_rgba8(43, 24, 5, 0.5);
const INGOT_LIT_FLANK: Color = Color::from_rgba8(255, 240, 190, 0.22);
const INGOT_DARK_FLANK: Color = Color::from_rgba8(43, 24, 5, 0.25);
/// What rings the knob of a slider, in both themes.
const KNOB_RING: Color = Color::from_rgb8(0xb0, 0x83, 0x2b);

impl Accent {
    /// The ring of the active window, over its height. The jump from ochre
    /// to shadow is a sharp break, and meant: it is what makes a metal.
    pub fn ring(&self) -> [Stop; 8] {
        [
            (0.0, self.glint),
            (0.02, self.light),
            (0.14, self.gold),
            (0.45, self.ochre),
            (0.453, self.shadow),
            (0.62, self.brown),
            (0.84, self.mid),
            (1.0, self.rebound),
        ]
    }

    /// A thin piece: the mark of an open application, the squares of the
    /// launcher, a ring.
    pub fn thin_piece(&self) -> [Stop; 4] {
        [
            (0.0, self.light),
            (0.35, self.gold),
            (0.65, self.ochre),
            (1.0, self.brown),
        ]
    }

    /// A bar: the filled part of a slider, the line under the search field.
    pub fn bar(&self) -> [Stop; 3] {
        [(0.0, self.light), (0.4, self.gold), (1.0, self.bronze)]
    }

    /// An ingot: gold that carries text or a symbol, which are then in ink.
    pub fn ingot(&self) -> [Stop; 4] {
        [
            (0.0, self.light),
            (0.22, self.gold),
            (0.64, self.ochre),
            (1.0, self.bronze),
        ]
    }

    /// An ingot `height` pixels high with the bevel of its top and of its
    /// foot: the bright edge and the dark foot are part of the ramp.
    pub fn beveled_ingot(&self, height: f32) -> [Stop; 8] {
        let ingot = self.ingot();
        // A piece too low for its bevel keeps a little of each.
        let edge = (1.0 / height).min(0.2);
        let foot = 1.0 - (2.0 / height).min(0.3);
        [
            (0.0, laid(at(&ingot, 0.0), INGOT_EDGE)),
            (edge, laid(at(&ingot, edge), INGOT_EDGE)),
            (edge + BREAK, at(&ingot, edge)),
            ingot[1],
            ingot[2],
            (foot, at(&ingot, foot)),
            (foot + BREAK, laid(at(&ingot, foot), INGOT_FOOT)),
            (1.0, laid(at(&ingot, 1.0), INGOT_FOOT)),
        ]
    }

    /// The hairline around what floats above the windows, `height` pixels
    /// high: 1 px all around, seen through, and a brighter one over it
    /// along the top edge.
    pub fn hairline(&self, contour: f32, ridge: f32, height: f32) -> [Stop; 4] {
        let contour = alpha(self.contour, contour);
        let top = laid(contour, alpha(self.ridge, ridge));
        let edge = (1.0 / height).min(0.5);
        [
            (0.0, top),
            (edge, top),
            (edge + BREAK, contour),
            (1.0, contour),
        ]
    }

    /// A card `height` pixels high that stands on a bar of gold of 2 px.
    pub fn underline(&self, card: Color, height: f32) -> [Stop; 5] {
        let [(_, light), (middle, gold), (_, bronze)] = self.bar();
        let line = (2.0 / height).min(0.5);
        let top = 1.0 - line;
        [
            (0.0, card),
            (top, card),
            (top + BREAK, light),
            (top + line * middle, gold),
            (1.0, bronze),
        ]
    }
}

/// The color of a ramp `offset` of the way down.
fn at(stops: &[Stop], offset: f32) -> Color {
    let Some(below) = stops.iter().position(|(place, _)| *place >= offset) else {
        return stops.last().map_or(Color::TRANSPARENT, |(_, color)| *color);
    };
    let (end, to) = stops[below];
    let Some((start, from)) = below.checked_sub(1).map(|above| stops[above]) else {
        return to;
    };
    let along = (offset - start) / (end - start).max(f32::EPSILON);
    let blend = |a: f32, b: f32| a + (b - a) * along;
    Color {
        r: blend(from.r, to.r),
        g: blend(from.g, to.g),
        b: blend(from.b, to.b),
        a: blend(from.a, to.a),
    }
}

/// `over` laid on `under`, each seen through as much as it says.
fn laid(under: Color, over: Color) -> Color {
    let kept = under.a * (1.0 - over.a);
    let opacity = over.a + kept;
    if opacity <= 0.0 {
        return Color::TRANSPARENT;
    }
    let blend = |under: f32, over_channel: f32| (over_channel * over.a + under * kept) / opacity;
    Color {
        r: blend(under.r, over.r),
        g: blend(under.g, over.g),
        b: blend(under.b, over.b),
        a: opacity,
    }
}

/// A background that goes down a ramp, from the top of what it fills to
/// its bottom. A ramp has at most eight colors.
pub fn ramp(stops: &[Stop]) -> Background {
    let mut colors = [None; 8];
    for (slot, (offset, color)) in colors.iter_mut().zip(stops) {
        *slot = Some(ColorStop {
            offset: *offset,
            color: *color,
        });
    }
    // Half a turn: down, as `180deg` in CSS.
    let linear = Linear {
        angle: Radians(PI),
        stops: colors,
    };
    Background::Gradient(Gradient::Linear(linear))
}

/// The same ramp, seen through.
pub fn faded(stops: &[Stop], opacity: f32) -> Background {
    let seen: Vec<Stop> = stops
        .iter()
        .map(|(offset, color)| (*offset, alpha(*color, color.a * opacity)))
        .collect();
    ramp(&seen)
}

/// An ingot `height` pixels high; a little brighter under the pointer.
pub fn ingot(accent: &Accent, height: f32, hovered: bool) -> Background {
    let mut stops = accent.beveled_ingot(height);
    if hovered {
        for (_, color) in &mut stops {
            *color = laid(*color, alpha(accent.glint, 0.14));
        }
    }
    ramp(&stops)
}

/// What an ingot carries, between the two flanks of its bevel. The flanks
/// stop where the corners of the ingot, of this `radius`, start to turn.
/// It takes all the height it is given.
pub fn flanked<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    radius: f32,
) -> Row<'a, Message> {
    let turn = (radius - radius.sqrt()).max(0.0);
    let flank = move |color: Color| {
        let line = container(space().width(1).height(Length::Fill))
            .style(move |_| crate::style::fill(color, 0.0));
        container(line)
            .height(Length::Fill)
            .padding(Padding::from([turn, 0.0]))
    };
    row![
        flank(INGOT_LIT_FLANK),
        content.into(),
        flank(INGOT_DARK_FLANK)
    ]
    .height(Length::Fill)
}

/// The frame of what floats above the windows: a box filled with the ramp
/// of its hairline. What it holds goes in a box 1 px smaller all around,
/// with [`within`] as corners.
pub fn frame(
    hairline: &[Stop],
    radius: impl Into<border::Radius>,
    shadow: Shadow,
) -> container::Style {
    container::Style {
        background: Some(ramp(hairline)),
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        shadow,
        ..container::Style::default()
    }
}

/// What floats above the windows, in its frame and on its `fill`. The
/// frame takes no room: what is inside is laid out as if it were not there.
/// The whole is as large as it is told to be, not as its content.
pub fn framed<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    fill: Color,
    hairline: [Stop; 4],
    radius: impl Into<border::Radius>,
    shadow: Shadow,
) -> Stack<'a, Message> {
    let radius = radius.into();
    let ground = container(space())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(fill.into()),
            border: Border {
                radius: within(radius),
                ..Border::default()
            },
            ..container::Style::default()
        });
    let frame = container(ground)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(1)
        .style(move |_| self::frame(&hairline, radius, shadow));
    stack![frame, content.into()]
}

/// The corners of what sits 1 px inside a frame.
pub fn within(radius: impl Into<border::Radius>) -> border::Radius {
    let radius = radius.into();
    let inside = |corner: f32| (corner - 1.0).max(0.0);
    border::Radius {
        top_left: inside(radius.top_left),
        top_right: inside(radius.top_right),
        bottom_right: inside(radius.bottom_right),
        bottom_left: inside(radius.bottom_left),
    }
}

/// A slider: what is filled is a bar of gold, and its knob, light in both
/// themes, is ringed with ochre.
pub fn slider_style(palette: &Palette) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (ramp(&palette.accent.bar()), palette.track.into()),
            width: 4.0,
            border: Border {
                radius: 2.0.into(),
                ..Border::default()
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 7.0 },
            background: Color::WHITE.into(),
            border_width: 1.5,
            border_color: KNOB_RING,
        },
    }
}

#[cfg(test)]
mod tests {
    use coquille_ipc::ThemeName;

    use super::*;

    const THEMES: [ThemeName; 2] = [ThemeName::Dark, ThemeName::Light];

    /// How bright a color looks, from 0 to 1.
    fn brightness(color: Color) -> f32 {
        0.299 * color.r + 0.587 * color.g + 0.114 * color.b
    }

    fn goes_down(stops: &[Stop]) -> bool {
        let ends = stops.first().unwrap().0 == 0.0 && stops.last().unwrap().0 == 1.0;
        ends && stops.is_sorted_by(|above, below| above.0 < below.0)
    }

    #[test]
    fn every_ramp_goes_down_its_piece_from_top_to_bottom() {
        for theme in THEMES {
            let gold = Accent::new(theme);
            assert!(goes_down(&gold.ring()));
            assert!(goes_down(&gold.thin_piece()));
            assert!(goes_down(&gold.bar()));
            assert!(goes_down(&gold.ingot()));
            // Whatever the height, even one too low for the bevel.
            for height in [4.0, 22.0, 28.0, 54.0, 1440.0] {
                assert!(goes_down(&gold.beveled_ingot(height)), "{height}");
                assert!(goes_down(&gold.hairline(0.5, 0.7, height)), "{height}");
                assert!(goes_down(&gold.underline(Color::WHITE, height)), "{height}");
            }
        }
    }

    #[test]
    fn the_ring_breaks_sharply_below_its_middle() {
        let gold = Accent::new(ThemeName::Dark);
        let ring = gold.ring();

        assert_eq!(
            (ring[3], ring[4]),
            ((0.45, gold.ochre), (0.453, gold.shadow))
        );
        assert_eq!(ring[0].1, gold.glint);
        assert_eq!(ring[7].1, gold.rebound);
    }

    #[test]
    fn pieces_bars_and_ingots_darken_all_the_way_down() {
        for theme in THEMES {
            let gold = Accent::new(theme);
            let ramps = [
                gold.thin_piece().to_vec(),
                gold.bar().to_vec(),
                gold.ingot().to_vec(),
            ];
            for ramp in ramps {
                assert!(
                    ramp.is_sorted_by(|above, below| brightness(above.1) > brightness(below.1)),
                    "{theme:?}"
                );
            }
        }
    }

    #[test]
    fn the_bevel_of_an_ingot_is_a_bright_pixel_on_top_and_two_dark_ones_at_the_foot() {
        let gold = Accent::new(ThemeName::Dark);

        let ingot = gold.beveled_ingot(50.0);

        // One pixel of fifty, then the ramp again.
        assert_eq!(ingot[1].0, 0.02);
        assert!(brightness(ingot[0].1) > brightness(gold.light));
        assert!(brightness(ingot[2].1) < brightness(ingot[1].1));
        // Two pixels of fifty, darker than the bronze they lie on.
        assert_eq!(ingot[5].0, 0.96);
        assert!(brightness(ingot[6].1) < brightness(ingot[5].1));
        assert!(brightness(ingot[7].1) < brightness(gold.bronze));
        // Between them, the ingot of the recipe.
        assert_eq!((ingot[3], ingot[4]), (gold.ingot()[1], gold.ingot()[2]));
    }

    #[test]
    fn a_hairline_is_brighter_along_the_top_edge_only() {
        let gold = Accent::new(ThemeName::Dark);

        let hairline = gold.hairline(0.5, 0.7, 100.0);

        assert_eq!(hairline[1].0, 0.01);
        assert_eq!(hairline[3].1, alpha(gold.contour, 0.5));
        assert!(brightness(hairline[0].1) > brightness(hairline[3].1));
        // The ridge lies on the contour: more of it hides what is behind.
        assert!(hairline[0].1.a > 0.7);
    }

    #[test]
    fn the_color_of_a_ramp_is_read_between_its_stops() {
        let stops = [
            (0.0, Color::BLACK),
            (0.5, Color::WHITE),
            (1.0, Color::WHITE),
        ];

        assert_eq!(at(&stops, 0.0), Color::BLACK);
        assert_eq!(at(&stops, 0.25), Color::from_rgb(0.5, 0.5, 0.5));
        assert_eq!(at(&stops, 0.75), Color::WHITE);
        assert_eq!(at(&stops, 2.0), Color::WHITE);
    }

    #[test]
    fn laying_a_color_on_another_mixes_them_by_opacity() {
        let half_white = alpha(Color::WHITE, 0.5);

        assert_eq!(
            laid(Color::BLACK, half_white),
            Color::from_rgb(0.5, 0.5, 0.5)
        );
        assert_eq!(laid(Color::BLACK, Color::WHITE), Color::WHITE);
        assert_eq!(laid(Color::BLACK, Color::TRANSPARENT), Color::BLACK);
        // Two colors seen through hide more together than each alone.
        assert_eq!(laid(half_white, half_white).a, 0.75);
    }

    #[test]
    fn a_ramp_keeps_its_colors_in_order() {
        let gold = Accent::new(ThemeName::Light);

        let Background::Gradient(Gradient::Linear(linear)) = ramp(&gold.bar()) else {
            panic!("not a ramp");
        };

        let offsets: Vec<f32> = linear
            .stops
            .iter()
            .flatten()
            .map(|stop| stop.offset)
            .collect();
        assert_eq!(offsets, [0.0, 0.4, 1.0]);
        assert_eq!(linear.stops[2].unwrap().color, gold.bronze);
    }

    #[test]
    fn what_sits_inside_a_frame_has_corners_one_pixel_tighter() {
        let inside = within(border::Radius::default().top_right(18).bottom_right(18));

        assert_eq!((inside.top_left, inside.top_right), (0.0, 17.0));
        assert_eq!(within(14.0).bottom_left, 13.0);
    }
}
