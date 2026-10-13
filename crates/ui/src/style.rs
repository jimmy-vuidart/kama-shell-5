//! Colors, fonts and the few shapes every view of the shell shares.

use coquille_ipc::ThemeName;
use iced::font::{Family, Weight};
use iced::widget::{button, container};
use iced::{Background, Border, Color, Font, Shadow, Vector};

pub const FIGTREE: &[&[u8]] = &[
    include_bytes!("../assets/fonts/Figtree-Regular.ttf"),
    include_bytes!("../assets/fonts/Figtree-Medium.ttf"),
    include_bytes!("../assets/fonts/Figtree-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Figtree-Bold.ttf"),
];

pub const REGULAR: Font = Font::with_name("Figtree");
pub const MEDIUM: Font = weighted(Weight::Medium);
pub const SEMIBOLD: Font = weighted(Weight::Semibold);
pub const BOLD: Font = weighted(Weight::Bold);

const fn weighted(weight: Weight) -> Font {
    Font {
        family: Family::Name("Figtree"),
        weight,
        ..Font::DEFAULT
    }
}

/// Unread counts, the close line of the menu, the power button.
pub const DANGER: Color = Color::from_rgb8(0xe5, 0x48, 0x4d);
/// The live level of a stream in the mixer.
pub const LEVEL: Color = Color::from_rgb8(0x3f, 0xb9, 0x50);

/// The accent of the shell: a gold, the same everywhere (see `docs/gold/`).
/// Not one color but a metal, lit from above: its tones from the brightest
/// to the darkest. The light theme has them a notch darker, or the
/// brightest would be lost on its ivory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accent {
    /// The very top of the ring of the active window.
    pub glint: Color,
    /// The top of every piece.
    pub light: Color,
    /// The body of the pieces, and the accent wherever it is flat.
    pub gold: Color,
    /// The lower part of the pieces.
    pub ochre: Color,
    /// The foot of ingots and bars.
    pub bronze: Color,
    /// The foot of thin pieces.
    pub brown: Color,
    /// The sharp break of the ring.
    pub shadow: Color,
    /// The lower part of the ring.
    pub mid: Color,
    /// The foot of the ring, where the light comes back.
    pub rebound: Color,
    /// Text in gold: the time, the headings.
    pub text: Color,
    /// Text laid on gold.
    pub ink: Color,
    /// The hairline around what floats (dock, drawer), and the brighter
    /// one along its top edge. Both are seen through.
    pub contour: Color,
    pub ridge: Color,
    /// A line that parts two things, seen through as well.
    pub hairline: Color,
}

impl Accent {
    pub fn new(theme: ThemeName) -> Self {
        let tone = |hex: u32| Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
        match theme {
            ThemeName::Dark => Self {
                glint: tone(0xff_fb_e8),
                light: tone(0xf6_e3_a1),
                gold: tone(0xdf_b8_5a),
                ochre: tone(0xb0_83_2b),
                bronze: tone(0x8a_5e_1c),
                brown: tone(0x6b_44_13),
                shadow: tone(0x2b_18_05),
                mid: tone(0xa8_7c_2a),
                rebound: tone(0xcf_a6_4c),
                text: tone(0xe6_c6_80),
                ink: tone(0x2a_1e_04),
                contour: tone(0x8c_62_1e),
                ridge: tone(0xf6_e3_a1),
                hairline: tone(0xe3_c1_5a),
            },
            ThemeName::Light => Self {
                glint: tone(0xf6_e7_b0),
                light: tone(0xe3_c8_78),
                gold: tone(0xc2_9a_38),
                ochre: tone(0x98_72_1f),
                // Not grouped by channel: clippy reads `_16` as a type.
                bronze: tone(0x7a5416),
                brown: tone(0x5e_3c_10),
                shadow: tone(0x2b_18_05),
                mid: tone(0x94_70_1f),
                rebound: tone(0xb8_90_2f),
                text: tone(0x77_56_0f),
                ink: tone(0x2a_1e_04),
                contour: tone(0xbe_a0_5a),
                ridge: tone(0x98_72_1f),
                hairline: tone(0x96_71_0f),
            },
        }
    }
}

/// The colors of a theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub accent: Accent,
    /// The bar and its menu.
    pub bar: Color,
    pub bar_border: Color,
    /// The lines that part the bar.
    pub bar_separator: Color,
    /// The drawer and the mixer.
    pub panel: Color,
    /// The control center at the foot of the drawer.
    pub block: Color,
    /// Tiles and buttons that stand out of a block.
    pub card: Color,
    pub hover: Color,
    pub separator: Color,
    /// The part of a slider that is not filled.
    pub track: Color,
    pub text: Color,
    pub muted: Color,
    /// What dims the rest of the screen behind the drawer.
    pub scrim: Color,
}

impl Palette {
    /// The grays are warm, drawn to brown: a black next to the gold in the
    /// dark theme, an ivory in the light one.
    pub fn new(theme: ThemeName) -> Self {
        let accent = Accent::new(theme);
        match theme {
            ThemeName::Dark => Self {
                dark: true,
                accent,
                bar: Color::from_rgba8(28, 25, 20, 0.96),
                bar_border: alpha(accent.contour, 0.5),
                bar_separator: alpha(accent.hairline, 0.3),
                panel: Color::from_rgba8(28, 25, 20, 0.98),
                block: Color::from_rgb8(0x25, 0x21, 0x1a),
                card: Color::from_rgb8(0x2f, 0x2a, 0x21),
                hover: Color::from_rgba8(255, 244, 220, 0.07),
                separator: Color::from_rgb8(0x2b, 0x27, 0x1f),
                track: Color::from_rgb8(0x2f, 0x2a, 0x21),
                text: Color::from_rgb8(0xf4, 0xf1, 0xea),
                muted: Color::from_rgb8(0xa8, 0x9e, 0x88),
                scrim: Color::from_rgba8(0, 0, 0, 0.45),
            },
            ThemeName::Light => Self {
                dark: false,
                accent,
                bar: Color::from_rgba8(255, 253, 247, 0.94),
                bar_border: alpha(accent.contour, 0.5),
                bar_separator: alpha(accent.hairline, 0.3),
                panel: Color::from_rgba8(250, 247, 240, 0.98),
                block: Color::from_rgb8(0xf1, 0xeb, 0xdc),
                card: Color::WHITE,
                hover: Color::from_rgba8(60, 45, 10, 0.06),
                separator: Color::from_rgb8(0xeb, 0xe5, 0xd6),
                track: Color::from_rgb8(0xe6, 0xdf, 0xcd),
                text: Color::from_rgb8(0x24, 0x1f, 0x14),
                muted: Color::from_rgb8(0x7a, 0x70, 0x5c),
                scrim: Color::from_rgba8(24, 18, 8, 0.35),
            },
        }
    }

    /// The soft shadow of what floats above the windows, straight down; the
    /// light theme needs it more than the dark one.
    pub fn shadow(&self) -> Shadow {
        let color = if self.dark {
            Color::from_rgba8(0, 0, 0, 0.4)
        } else {
            Color::from_rgba8(60, 45, 10, 0.16)
        };
        Shadow {
            color,
            offset: Vector::new(0.0, 4.0),
            blur_radius: 18.0,
        }
    }

    /// What is written or drawn on gold: an ink, in both themes.
    pub fn on_accent(&self) -> Color {
        self.accent.ink
    }
}

/// A filled rounded rectangle; with a color, or with a ramp of gold.
pub fn fill(background: impl Into<Background>, radius: f32) -> container::Style {
    container::Style {
        background: Some(background.into()),
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// A filled rounded rectangle with a hairline around it.
pub fn outlined(color: Color, line: Color, radius: f32) -> container::Style {
    container::Style {
        border: Border {
            color: line,
            width: 1.0,
            radius: radius.into(),
        },
        ..fill(color, radius)
    }
}

/// A button without a look of its own: its content decides, and its
/// background shows only under the pointer.
pub fn flat(hover: Color, radius: f32) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    tinted(Color::TRANSPARENT, hover, radius)
}

/// A button filled with `idle`, and with `hover` under the pointer.
pub fn tinted(
    idle: impl Into<Background>,
    hover: impl Into<Background>,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    let (idle, hover) = (idle.into(), hover.into());
    move |_, status| {
        let background = match status {
            button::Status::Hovered | button::Status::Pressed => hover,
            button::Status::Active | button::Status::Disabled => idle,
        };
        button::Style {
            background: Some(background),
            border: Border {
                radius: radius.into(),
                ..Border::default()
            },
            ..button::Style::default()
        }
    }
}

/// `color` with another opacity.
pub fn alpha(color: Color, opacity: f32) -> Color {
    Color {
        a: opacity,
        ..color
    }
}

/// `over` laid on `under` with some opacity: a hover on a colored tile.
pub fn mix(under: Color, over: Color, opacity: f32) -> Color {
    let blend = |a: f32, b: f32| a + (b - a) * opacity;
    Color {
        r: blend(under.r, over.r),
        g: blend(under.g, over.g),
        b: blend(under.b, over.b),
        a: under.a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How bright a color looks, from 0 to 1.
    fn brightness(color: Color) -> f32 {
        0.299 * color.r + 0.587 * color.g + 0.114 * color.b
    }

    #[test]
    fn accents_match_the_design() {
        let dark = Palette::new(ThemeName::Dark).accent;
        let light = Palette::new(ThemeName::Light).accent;

        assert_eq!(dark.gold, Color::from_rgb8(0xdf, 0xb8, 0x5a));
        assert_eq!(light.gold, Color::from_rgb8(0xc2, 0x9a, 0x38));
        assert_eq!(dark.light, Color::from_rgb8(0xf6, 0xe3, 0xa1));
        assert_eq!(light.light, Color::from_rgb8(0xe3, 0xc8, 0x78));
        assert_eq!(dark.text, Color::from_rgb8(0xe6, 0xc6, 0x80));
        assert_eq!(light.text, Color::from_rgb8(0x77, 0x56, 0x0f));
    }

    #[test]
    fn the_tones_of_a_piece_go_from_light_to_dark() {
        for theme in [ThemeName::Dark, ThemeName::Light] {
            let gold = Palette::new(theme).accent;
            let piece = [
                gold.glint,
                gold.light,
                gold.gold,
                gold.ochre,
                gold.bronze,
                gold.brown,
                gold.shadow,
            ];

            assert!(
                piece.is_sorted_by(|above, below| brightness(*above) > brightness(*below)),
                "{theme:?}"
            );
        }
    }

    #[test]
    fn what_is_written_on_gold_is_the_same_ink_in_both_themes() {
        let dark = Palette::new(ThemeName::Dark);
        let light = Palette::new(ThemeName::Light);

        assert_eq!(dark.on_accent(), Color::from_rgb8(0x2a, 0x1e, 0x04));
        assert_eq!(light.on_accent(), dark.on_accent());
    }

    #[test]
    fn the_grays_are_warm() {
        // Nothing leans to blue any more.
        for theme in [ThemeName::Dark, ThemeName::Light] {
            let palette = Palette::new(theme);
            for gray in [
                palette.bar,
                palette.panel,
                palette.block,
                palette.card,
                palette.hover,
                palette.separator,
                palette.track,
                palette.text,
                palette.muted,
                palette.scrim,
            ] {
                assert!(gray.r >= gray.g && gray.g >= gray.b, "{theme:?} {gray:?}");
            }
        }
    }

    #[test]
    fn the_grays_match_the_design() {
        let dark = Palette::new(ThemeName::Dark);
        let light = Palette::new(ThemeName::Light);

        assert_eq!(dark.bar, Color::from_rgba8(28, 25, 20, 0.96));
        assert_eq!(light.bar, Color::from_rgba8(255, 253, 247, 0.94));
        assert_eq!(dark.card, Color::from_rgb8(0x2f, 0x2a, 0x21));
        assert_eq!(dark.text, Color::from_rgb8(0xf4, 0xf1, 0xea));
        assert_eq!(light.text, Color::from_rgb8(0x24, 0x1f, 0x14));
        assert_eq!(dark.muted, Color::from_rgb8(0xa8, 0x9e, 0x88));
        assert_eq!(light.muted, Color::from_rgb8(0x7a, 0x70, 0x5c));
    }
}
