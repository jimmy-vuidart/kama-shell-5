//! Light and dark colors of the shell chrome (see `docs/maquettes/`), and
//! its one accent, a gold (see `docs/gold/`).

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

    /// The same color, seen through: `percent` of it shows.
    pub const fn opacity(self, percent: u32) -> Self {
        Self {
            a: ((self.a as u32 * percent + 50) / 100) as u8,
            ..self
        }
    }

    /// Premultiplied components in `0.0..=1.0`, as the renderer wants them.
    pub fn premultiplied(self) -> [f32; 4] {
        let alpha = f32::from(self.a) / 255.0;
        let channel = |value: u8| f32::from(value) / 255.0 * alpha;
        [channel(self.r), channel(self.g), channel(self.b), alpha]
    }
}

/// The accent of the shell: a gold, the same everywhere. Not one color but
/// a metal, lit from above: its tones from the brightest to the darkest.
/// The light theme has them a notch darker, or the brightest would be lost
/// on its ivory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accent {
    /// The very top of the ring.
    pub glint: Rgba,
    /// The top of every piece.
    pub light: Rgba,
    /// The body of the pieces, and the accent wherever it is flat.
    pub gold: Rgba,
    /// The lower part of the pieces.
    pub ochre: Rgba,
    /// The foot of ingots and bars.
    pub bronze: Rgba,
    /// The foot of thin pieces.
    pub brown: Rgba,
    /// The sharp break of the ring.
    pub shadow: Rgba,
    /// The lower part of the ring.
    pub mid: Rgba,
    /// The foot of the ring, where the light comes back.
    pub rebound: Rgba,
    /// Text in gold.
    pub text: Rgba,
    /// Text laid on gold.
    pub ink: Rgba,
    /// The hairline around what floats (capsules, dock, drawer), and the
    /// brighter one along its top edge. Both show through.
    pub contour: Rgba,
    pub ridge: Rgba,
    /// A line that parts two things, seen through as well.
    pub hairline: Rgba,
}

/// One color of a ramp, and how far down the piece it sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stop {
    /// In thousandths of the height of the piece, from its top.
    pub at: u16,
    pub color: Rgba,
}

impl Stop {
    const fn new(at: u16, color: Rgba) -> Self {
        Self { at, color }
    }

    /// From 0 at the top of the piece to 1 at its bottom.
    pub fn fraction(self) -> f32 {
        f32::from(self.at) / 1000.0
    }
}

/// The bevel of an ingot, laid over its ramp: a bright edge of 1 px along
/// the top, a dark foot of 2 px, and a flank of 1 px on each side, the left
/// one lit.
pub const INGOT_EDGE: Rgba = Rgba::rgb(0xff_fb_e8).opacity(90);
pub const INGOT_FOOT: Rgba = Rgba::rgb(0x2b_18_05).opacity(50);
pub const INGOT_LIT_FLANK: Rgba = Rgba::rgb(0xff_f0_be).opacity(22);
pub const INGOT_DARK_FLANK: Rgba = Rgba::rgb(0x2b_18_05).opacity(25);

impl Accent {
    pub const fn new(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Light => Self {
                glint: Rgba::rgb(0xf6_e7_b0),
                light: Rgba::rgb(0xe3_c8_78),
                gold: Rgba::rgb(0xc2_9a_38),
                ochre: Rgba::rgb(0x98_72_1f),
                // Not grouped by channel: clippy reads `_16` as a type.
                bronze: Rgba::rgb(0x7a5416),
                brown: Rgba::rgb(0x5e_3c_10),
                shadow: Rgba::rgb(0x2b_18_05),
                mid: Rgba::rgb(0x94_70_1f),
                rebound: Rgba::rgb(0xb8_90_2f),
                text: Rgba::rgb(0x77_56_0f),
                ink: Rgba::rgb(0x2a_1e_04),
                contour: Rgba::rgb(0xbe_a0_5a),
                ridge: Rgba::rgb(0x98_72_1f),
                hairline: Rgba::rgb(0x96_71_0f),
            },
            ThemeKind::Dark => Self {
                glint: Rgba::rgb(0xff_fb_e8),
                light: Rgba::rgb(0xf6_e3_a1),
                gold: Rgba::rgb(0xdf_b8_5a),
                ochre: Rgba::rgb(0xb0_83_2b),
                bronze: Rgba::rgb(0x8a_5e_1c),
                brown: Rgba::rgb(0x6b_44_13),
                shadow: Rgba::rgb(0x2b_18_05),
                mid: Rgba::rgb(0xa8_7c_2a),
                rebound: Rgba::rgb(0xcf_a6_4c),
                text: Rgba::rgb(0xe6_c6_80),
                ink: Rgba::rgb(0x2a_1e_04),
                contour: Rgba::rgb(0x8c_62_1e),
                ridge: Rgba::rgb(0xf6_e3_a1),
                hairline: Rgba::rgb(0xe3_c1_5a),
            },
        }
    }

    // The recipes of the metal. Every ramp runs from the top of its piece
    // to its bottom: the light comes from above, and from nowhere else.

    /// The ring of the active window, over the height of the window. The
    /// jump from ochre to shadow, in less than a hundredth of the height,
    /// is a sharp break, and meant: it is what makes the ring a metal. The
    /// top edge is then bright from end to end, both sides break at the
    /// same height, and the bottom edge is a middling gold.
    pub const fn ring(&self) -> [Stop; 8] {
        [
            Stop::new(0, self.glint),
            Stop::new(20, self.light),
            Stop::new(140, self.gold),
            Stop::new(450, self.ochre),
            Stop::new(453, self.shadow),
            Stop::new(620, self.brown),
            Stop::new(840, self.mid),
            Stop::new(1000, self.rebound),
        ]
    }

    /// A thin piece: the ring of the capsule of the active window.
    pub const fn thin_piece(&self) -> [Stop; 4] {
        [
            Stop::new(0, self.light),
            Stop::new(350, self.gold),
            Stop::new(650, self.ochre),
            Stop::new(1000, self.brown),
        ]
    }

    /// An ingot: gold that carries text, which is then written in ink.
    pub const fn ingot(&self) -> [Stop; 4] {
        [
            Stop::new(0, self.light),
            Stop::new(220, self.gold),
            Stop::new(640, self.ochre),
            Stop::new(1000, self.bronze),
        ]
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
    /// The box the capsules of a slot sit in; the backdrop shows through.
    pub capsule_box: Rgba,
    /// Icons of buttons that do nothing yet.
    pub disabled: Rgba,
    pub accent: Accent,
    /// Under the pointer, and the capsule of the window a slot shows.
    pub button_hover: Rgba,
    pub close_hover: Rgba,
}

impl Theme {
    pub const fn new(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Light => Self {
                kind,
                backdrop: Rgba::rgb(0xee_ea_df),
                surface: Rgba::rgb(0xff_fe_fb),
                titlebar: Rgba::rgb(0xff_fd_f8),
                separator: Rgba::rgb(0xeb_e5_d6),
                text: Rgba::rgb(0x24_1f_14),
                text_muted: Rgba::rgb(0x7a_70_5c),
                capsule_box: Rgba::rgb(0xff_fd_f7).opacity(90),
                disabled: Rgba::rgb(0xbd_b6_a4),
                accent: Accent::new(kind),
                button_hover: Rgba::rgb(0xf1_eb_dc),
                close_hover: Rgba::rgb(0xe5_48_4d),
            },
            ThemeKind::Dark => Self {
                kind,
                backdrop: Rgba::rgb(0x13_11_0d),
                surface: Rgba::rgb(0x17_15_12),
                titlebar: Rgba::rgb(0x1d_1a_15),
                separator: Rgba::rgb(0x2b_27_1f),
                text: Rgba::rgb(0xf4_f1_ea),
                text_muted: Rgba::rgb(0xa8_9e_88),
                capsule_box: Rgba::rgb(0x1c_19_14).opacity(88),
                disabled: Rgba::rgb(0x57_50_42),
                accent: Accent::new(kind),
                button_hover: Rgba::rgb(0x33_2e_24),
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

    /// How bright a color looks, from 0 to 255.
    fn brightness(color: Rgba) -> u32 {
        (299 * u32::from(color.r) + 587 * u32::from(color.g) + 114 * u32::from(color.b)) / 1000
    }

    #[test]
    fn the_gold_matches_the_design() {
        let light = Theme::new(ThemeKind::Light).accent;
        let dark = Theme::new(ThemeKind::Dark).accent;

        assert_eq!(dark.gold, Rgba::rgb(0xdf_b8_5a));
        assert_eq!(light.gold, Rgba::rgb(0xc2_9a_38));
        assert_eq!(dark.glint, Rgba::rgb(0xff_fb_e8));
        assert_eq!(light.glint, Rgba::rgb(0xf6_e7_b0));
        assert_eq!(dark.text, Rgba::rgb(0xe6_c6_80));
        assert_eq!(light.text, Rgba::rgb(0x77_56_0f));
        // What is written on gold, and the break of the ring, are the same
        // in both themes.
        assert_eq!(dark.ink, light.ink);
        assert_eq!(dark.shadow, light.shadow);
    }

    #[test]
    fn the_tones_of_a_piece_go_from_light_to_dark() {
        for kind in [ThemeKind::Light, ThemeKind::Dark] {
            let gold = Theme::new(kind).accent;
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
                "{kind:?}"
            );
        }
    }

    #[test]
    fn every_ramp_goes_down_the_piece_from_its_top_to_its_bottom() {
        for kind in [ThemeKind::Light, ThemeKind::Dark] {
            let gold = Theme::new(kind).accent;
            let ramps = [
                gold.ring().to_vec(),
                gold.thin_piece().to_vec(),
                gold.ingot().to_vec(),
            ];
            for ramp in ramps {
                assert_eq!(ramp.first().unwrap().at, 0);
                assert_eq!(ramp.last().unwrap().at, 1000);
                assert!(ramp.is_sorted_by(|above, below| above.at < below.at));
            }
        }
    }

    #[test]
    fn the_ring_breaks_sharply_below_its_middle() {
        let gold = Theme::new(ThemeKind::Dark).accent;
        let ring = gold.ring();

        let (before, after) = (ring[3], ring[4]);
        assert_eq!((before.color, after.color), (gold.ochre, gold.shadow));
        assert_eq!((before.at, after.at), (450, 453));
        // Bright at the very top, a middling gold at the foot.
        assert_eq!(ring[0].color, gold.glint);
        assert_eq!(ring[7].color, gold.rebound);
        assert!(brightness(ring[7].color) < brightness(ring[2].color));
    }

    #[test]
    fn thin_pieces_and_ingots_darken_all_the_way_down() {
        for kind in [ThemeKind::Light, ThemeKind::Dark] {
            let gold = Theme::new(kind).accent;
            for ramp in [gold.thin_piece(), gold.ingot()] {
                assert!(
                    ramp.is_sorted_by(|above, below| {
                        brightness(above.color) > brightness(below.color)
                    }),
                    "{kind:?}"
                );
            }
        }
    }

    #[test]
    fn a_color_seen_through_keeps_its_hue() {
        let seen = Rgba::rgb(0x2b_18_05).opacity(50);

        assert_eq!((seen.r, seen.g, seen.b, seen.a), (0x2b, 0x18, 0x05, 128));
        assert_eq!(Rgba::WHITE.opacity(100), Rgba::WHITE);
        assert_eq!(INGOT_EDGE.a, 230);
    }

    #[test]
    fn the_light_theme_has_its_gold_a_notch_darker() {
        let light = Theme::new(ThemeKind::Light).accent;
        let dark = Theme::new(ThemeKind::Dark).accent;

        for (light, dark) in [
            (light.glint, dark.glint),
            (light.light, dark.light),
            (light.gold, dark.gold),
            (light.ochre, dark.ochre),
            (light.bronze, dark.bronze),
            (light.brown, dark.brown),
            (light.mid, dark.mid),
            (light.rebound, dark.rebound),
        ] {
            assert!(brightness(light) < brightness(dark));
        }
    }

    #[test]
    fn the_grays_are_warm() {
        // Nothing of the chrome leans to blue any more.
        for kind in [ThemeKind::Light, ThemeKind::Dark] {
            let theme = Theme::new(kind);
            for gray in [
                theme.backdrop,
                theme.surface,
                theme.titlebar,
                theme.separator,
                theme.text,
                theme.text_muted,
                theme.disabled,
                theme.button_hover,
            ] {
                assert!(gray.r >= gray.g && gray.g >= gray.b, "{kind:?} {gray:?}");
            }
        }
    }

    #[test]
    fn the_chrome_matches_the_design() {
        let light = Theme::new(ThemeKind::Light);
        let dark = Theme::new(ThemeKind::Dark);

        assert_eq!(dark.surface, Rgba::rgb(0x17_15_12));
        assert_eq!(light.surface, Rgba::rgb(0xff_fe_fb));
        assert_eq!(dark.titlebar, Rgba::rgb(0x1d_1a_15));
        assert_eq!(light.titlebar, Rgba::rgb(0xff_fd_f8));
        assert_eq!(dark.text, Rgba::rgb(0xf4_f1_ea));
        assert_eq!(light.text, Rgba::rgb(0x24_1f_14));
    }
}
