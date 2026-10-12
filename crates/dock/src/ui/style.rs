//! Colors, fonts and the few shapes every view of the dock shares.

use coquille_ipc::ThemeName;
use iced::font::{Family, Weight};
use iced::widget::{button, container};
use iced::{Background, Border, Color, Font, Shadow, Vector};

pub const FIGTREE: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/Figtree-Regular.ttf"),
    include_bytes!("../../assets/fonts/Figtree-Medium.ttf"),
    include_bytes!("../../assets/fonts/Figtree-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/Figtree-Bold.ttf"),
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

/// The colors of a theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub accent: Color,
    /// The bar and its menu.
    pub bar: Color,
    pub bar_border: Color,
    /// The drawer and the mixer.
    pub panel: Color,
    /// The control center at the foot of the drawer.
    pub block: Color,
    /// Tiles and buttons that stand out of a block.
    pub card: Color,
    pub hover: Color,
    pub separator: Color,
    pub text: Color,
    pub muted: Color,
    /// What dims the rest of the screen behind the drawer.
    pub scrim: Color,
}

impl Palette {
    pub fn new(theme: ThemeName, accent: Option<Color>) -> Self {
        match theme {
            ThemeName::Dark => Self {
                dark: true,
                accent: accent.unwrap_or(Color::from_rgb8(0x7a, 0xa7, 0xff)),
                bar: Color::from_rgba8(30, 35, 45, 0.96),
                bar_border: Color::from_rgba8(255, 255, 255, 0.07),
                panel: Color::from_rgba8(0x1b, 0x1f, 0x27, 0.98),
                block: Color::from_rgb8(0x23, 0x28, 0x33),
                card: Color::from_rgb8(0x2c, 0x32, 0x3e),
                hover: Color::from_rgba8(255, 255, 255, 0.07),
                separator: Color::from_rgba8(255, 255, 255, 0.08),
                text: Color::from_rgb8(0xf1, 0xf3, 0xf6),
                muted: Color::from_rgb8(0x98, 0xa1, 0xb2),
                scrim: Color::from_rgba8(0, 0, 0, 0.45),
            },
            ThemeName::Light => Self {
                dark: false,
                accent: accent.unwrap_or(Color::from_rgb8(0x2f, 0x6f, 0xed)),
                bar: Color::from_rgba8(255, 255, 255, 0.92),
                bar_border: Color::from_rgba8(0, 0, 0, 0.06),
                panel: Color::from_rgba8(0xf6, 0xf7, 0xfa, 0.98),
                block: Color::from_rgb8(0xec, 0xee, 0xf3),
                card: Color::WHITE,
                hover: Color::from_rgba8(0, 0, 0, 0.05),
                separator: Color::from_rgba8(0, 0, 0, 0.08),
                text: Color::from_rgb8(0x1c, 0x22, 0x30),
                muted: Color::from_rgb8(0x6b, 0x73, 0x85),
                scrim: Color::from_rgba8(15, 18, 24, 0.35),
            },
        }
    }

    /// The soft shadow of what floats above the windows; the light theme
    /// needs it more than the dark one.
    pub fn shadow(&self) -> Shadow {
        Shadow {
            color: Color::from_rgba8(0, 0, 0, if self.dark { 0.35 } else { 0.16 }),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 18.0,
        }
    }

    /// A color the accent can be read on.
    pub fn on_accent(&self) -> Color {
        if self.dark {
            Color::from_rgb8(0x0f, 0x12, 0x18)
        } else {
            Color::WHITE
        }
    }
}

/// `#rrggbb` as a color.
pub fn parse_color(text: &str) -> Option<Color> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).ok();
    Some(Color::from_rgb8(channel(0)?, channel(2)?, channel(4)?))
}

/// A filled rounded rectangle.
pub fn fill(color: Color, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(color)),
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
    idle: Color,
    hover: Color,
    radius: f32,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let color = match status {
            button::Status::Hovered | button::Status::Pressed => hover,
            button::Status::Active | button::Status::Disabled => idle,
        };
        button::Style {
            background: Some(Background::Color(color)),
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

    #[test]
    fn accents_match_the_design() {
        let dark = Palette::new(ThemeName::Dark, None);
        let light = Palette::new(ThemeName::Light, None);

        assert_eq!(dark.accent, Color::from_rgb8(0x7a, 0xa7, 0xff));
        assert_eq!(light.accent, Color::from_rgb8(0x2f, 0x6f, 0xed));
    }

    #[test]
    fn the_accent_of_the_configuration_replaces_the_one_of_the_theme() {
        let accent = parse_color("#ff8800");

        let palette = Palette::new(ThemeName::Dark, accent);

        assert_eq!(palette.accent, Color::from_rgb8(0xff, 0x88, 0x00));
    }

    #[test]
    fn only_six_hexadecimal_digits_make_a_color() {
        assert_eq!(
            parse_color("#7aa7ff"),
            Some(Color::from_rgb8(0x7a, 0xa7, 0xff))
        );
        for text in ["7aa7ff", "#7aa7f", "#7aa7ffff", "#zzzzzz", ""] {
            assert_eq!(parse_color(text), None, "{text}");
        }
    }
}
