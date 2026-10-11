//! Light and dark colors of the shell chrome (see `docs/maquettes/`).

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    Light,
    Dark,
}

/// A straight (non-premultiplied) sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const WHITE: Self = Self::rgb(0xff_ff_ff);

    /// From `0xRRGGBB`, opaque.
    pub const fn rgb(hex: u32) -> Self {
        Self {
            r: (hex >> 16) as u8,
            g: (hex >> 8) as u8,
            b: hex as u8,
            a: 0xff,
        }
    }

    pub const fn from_array([r, g, b]: [u8; 3]) -> Self {
        Self { r, g, b, a: 0xff }
    }

    /// Premultiplied components in `0.0..=1.0`, as the renderer wants them.
    pub fn premultiplied(self) -> [f32; 4] {
        let alpha = f32::from(self.a) / 255.0;
        let channel = |value: u8| f32::from(value) / 255.0 * alpha;
        [channel(self.r), channel(self.g), channel(self.b), alpha]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub kind: ThemeKind,
    /// Behind every window.
    pub backdrop: Rgba,
    /// Window background, visible where a client does not fill its slot.
    pub surface: Rgba,
    pub titlebar: Rgba,
    /// Hairline under the title bar.
    pub separator: Rgba,
    pub text: Rgba,
    pub text_muted: Rgba,
    /// Icons of buttons that do nothing yet.
    pub disabled: Rgba,
    pub accent: Rgba,
    pub button_hover: Rgba,
    pub close_hover: Rgba,
}

impl Theme {
    pub const fn new(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Light => Self {
                kind,
                backdrop: Rgba::rgb(0xe8_eb_f0),
                surface: Rgba::rgb(0xff_ff_ff),
                titlebar: Rgba::rgb(0xff_ff_ff),
                separator: Rgba::rgb(0xe6_e8_ec),
                text: Rgba::rgb(0x1c_22_30),
                text_muted: Rgba::rgb(0x6b_73_85),
                disabled: Rgba::rgb(0xb3_b9_c5),
                accent: Rgba::rgb(0x2f_6f_ed),
                button_hover: Rgba::rgb(0xec_ee_f2),
                close_hover: Rgba::rgb(0xe5_48_4d),
            },
            ThemeKind::Dark => Self {
                kind,
                backdrop: Rgba::rgb(0x0f_12_18),
                surface: Rgba::rgb(0x17_1b_22),
                titlebar: Rgba::rgb(0x1b_1f_27),
                separator: Rgba::rgb(0x26_2b_35),
                text: Rgba::rgb(0xf1_f3_f6),
                text_muted: Rgba::rgb(0x98_a1_b2),
                disabled: Rgba::rgb(0x4b_52_62),
                accent: Rgba::rgb(0x7a_a7_ff),
                button_hover: Rgba::rgb(0x2a_2f_3a),
                close_hover: Rgba::rgb(0xe5_48_4d),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors_are_split_into_channels() {
        assert_eq!(
            Rgba::rgb(0x2f_6f_ed),
            Rgba {
                r: 0x2f,
                g: 0x6f,
                b: 0xed,
                a: 0xff
            }
        );
    }

    #[test]
    fn premultiplying_scales_channels_by_alpha() {
        let half_red = Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 51,
        };

        assert_eq!(half_red.premultiplied(), [0.2, 0.0, 0.0, 0.2]);
    }

    #[test]
    fn accents_match_the_design() {
        assert_eq!(Theme::new(ThemeKind::Light).accent, Rgba::rgb(0x2f_6f_ed));
        assert_eq!(Theme::new(ThemeKind::Dark).accent, Rgba::rgb(0x7a_a7_ff));
    }
}
