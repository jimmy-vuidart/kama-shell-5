//! Appearance design tokens, palette colors and application fallback badges.
//!
//! Pure data crate without any graphical dependencies (no iced, no Wayland).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThemeKind {
    Light,
    Dark,
}

/// A straight (non-premultiplied) sRGB color with an alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Self = Self::rgb(0xff_ff_ff);
    pub const BLACK: Self = Self::rgb(0x00_00_00);

    /// From `0xRRGGBB`, fully opaque.
    pub const fn rgb(hex: u32) -> Self {
        Self {
            r: (hex >> 16) as u8,
            g: (hex >> 8) as u8,
            b: hex as u8,
            a: 0xff,
        }
    }

    /// From discrete channels, with an alpha.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// From discrete channels, fully opaque.
    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 0xff }
    }

    /// From `[r, g, b]`, fully opaque.
    pub const fn from_array([r, g, b]: [u8; 3]) -> Self {
        Self { r, g, b, a: 0xff }
    }

    /// From `[r, g, b, a]`.
    pub const fn from_rgba_array([r, g, b, a]: [u8; 4]) -> Self {
        Self { r, g, b, a }
    }

    /// From `#rrggbb`, as written in the configuration.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let hex = u32::from_str_radix(digits, 16).ok()?;
        Some(Self::rgb(hex))
    }

    /// As `#rrggbb`, without the alpha channel.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Same color with a different alpha value.
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Premultiplied components in `0.0..=1.0`, for GL shaders.
    pub fn premultiplied(self) -> [f32; 4] {
        let alpha = f32::from(self.a) / 255.0;
        let channel = |value: u8| f32::from(value) / 255.0 * alpha;
        [channel(self.r), channel(self.g), channel(self.b), alpha]
    }
}

/// Fixed danger color (power button, unread counts, close button).
pub const DANGER: [u8; 3] = [0xe5, 0x48, 0x4d];

/// Fixed audio level color.
pub const LEVEL: [u8; 3] = [0x3f, 0xb9, 0x50];

/// Eight pleasant colors for application badges without an icon.
pub const BADGE_PALETTE: [[u8; 3]; 8] = [
    [0x7c, 0x3a, 0xed],
    [0x25, 0x63, 0xeb],
    [0x0d, 0x94, 0x88],
    [0xea, 0x58, 0x0c],
    [0xca, 0x8a, 0x04],
    [0x4f, 0x46, 0xe5],
    [0xdc, 0x26, 0x26],
    [0x47, 0x55, 0x69],
];

/// The RGB badge color for an application, stable across runs (FNV-1a hash).
pub fn badge_rgb(key: &str) -> [u8; 3] {
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    BADGE_PALETTE[(hash % BADGE_PALETTE.len() as u64) as usize]
}

/// The badge color for an application, as a `Color`.
pub fn badge_color(key: &str) -> Color {
    Color::from_array(badge_rgb(key))
}

/// Liquid glass appearance tokens for a theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tokens {
    pub kind: ThemeKind,
    /// Background behind the wallpaper.
    pub backdrop: Color,
    /// Bar, drawer, titlebars, sidebar.
    pub glass: Color,
    /// Menus, tooltips, mixer, active capsule.
    pub glass_strong: Color,
    /// 1px edge rim.
    pub rim: Color,
    /// Tile or block on top of glass.
    pub card: Color,
    /// Hover overlay.
    pub hover: Color,
    /// Separator lines and hairpins.
    pub separator: Color,
    /// Primary text.
    pub text: Color,
    /// Muted secondary text.
    pub muted: Color,
    /// Soft shadows.
    pub shadow: Color,
    /// Scrim veil behind drawer.
    pub scrim: Color,
    /// Default periwinkle accent.
    pub accent: Color,
    /// Window background behind client surfaces.
    pub surface: Color,
    /// Titlebar fill.
    pub titlebar: Color,
    /// Disabled button icons.
    pub disabled: Color,
    /// Button hover background.
    pub button_hover: Color,
    /// Close button hover background.
    pub close_hover: Color,
}

impl Tokens {
    pub const fn new(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Light => {
                let glass = Color::rgba(255, 255, 255, 140);
                let hover = Color::rgba(255, 255, 255, 89);
                Self {
                    kind,
                    backdrop: Color::rgb(0xe9_e6_f4),
                    glass,
                    glass_strong: Color::rgba(255, 255, 255, 204),
                    rim: Color::rgba(255, 255, 255, 166),
                    card: Color::rgba(255, 255, 255, 153),
                    hover,
                    separator: Color::rgba(30, 34, 60, 20),
                    text: Color::rgb(0x1d_20_33),
                    muted: Color::rgb(0x6b_6f_85),
                    shadow: Color::rgba(60, 50, 120, 36),
                    scrim: Color::rgba(40, 30, 80, 26),
                    accent: Color::rgb(0x5b_63_e6),
                    surface: Color::rgba(255, 255, 255, 153),
                    titlebar: glass,
                    disabled: Color::rgb(0xb3_b9_c5),
                    button_hover: hover,
                    close_hover: Color::rgb(0xe5_48_4d),
                }
            }
            ThemeKind::Dark => {
                let glass = Color::rgba(22, 22, 26, 140);
                let hover = Color::rgba(255, 255, 255, 24);
                Self {
                    kind,
                    backdrop: Color::rgb(0x12_12_14),
                    glass,
                    glass_strong: Color::rgba(28, 28, 33, 210),
                    rim: Color::rgba(255, 255, 255, 28),
                    card: Color::rgba(255, 255, 255, 18),
                    hover,
                    separator: Color::rgba(255, 255, 255, 18),
                    text: Color::rgb(0xf1_f2_f4),
                    muted: Color::rgb(0x94_98_a4),
                    shadow: Color::rgba(0, 0, 0, 45),
                    scrim: Color::rgba(0, 0, 0, 45),
                    accent: Color::rgb(0x8f_97_ff),
                    surface: glass,
                    titlebar: glass,
                    disabled: Color::rgb(0x50_54_60),
                    button_hover: hover,
                    close_hover: Color::rgb(0xe5_48_4d),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_accents_match_periwinkle_design() {
        assert_eq!(Tokens::new(ThemeKind::Light).accent, Color::rgb(0x5b_63_e6));
        assert_eq!(Tokens::new(ThemeKind::Dark).accent, Color::rgb(0x8f_97_ff));
    }

    #[test]
    fn alpha_rule_for_translucent_surfaces() {
        for kind in [ThemeKind::Light, ThemeKind::Dark] {
            let tokens = Tokens::new(kind);
            // Scrim and shadows must stay under 0.18 alpha
            assert!(
                f32::from(tokens.scrim.a) / 255.0 <= 0.18,
                "{kind:?} scrim alpha > 0.18: {}",
                tokens.scrim.a
            );
            assert!(
                f32::from(tokens.shadow.a) / 255.0 <= 0.18,
                "{kind:?} shadow alpha > 0.18: {}",
                tokens.shadow.a
            );
            // Glass surfaces must be at or above 0.45 alpha
            assert!(
                f32::from(tokens.glass.a) / 255.0 >= 0.45,
                "{kind:?} glass alpha < 0.45: {}",
                tokens.glass.a
            );
            assert!(
                f32::from(tokens.glass_strong.a) / 255.0 >= 0.45,
                "{kind:?} glass_strong alpha < 0.45: {}",
                tokens.glass_strong.a
            );
        }
    }

    #[test]
    fn hex_color_parsing_and_formatting() {
        assert_eq!(Color::parse("#5b63e6"), Some(Color::rgb(0x5b_63_e6)));
        assert_eq!(Color::parse("#8F97FF"), Some(Color::rgb(0x8f_97_ff)));
        assert_eq!(Color::rgb(0x5b_63_e6).hex(), "#5b63e6");

        for invalid in ["5b63e6", "#5b63e", "#5b63e6ff", "#xyz123", "", "#"] {
            assert_eq!(Color::parse(invalid), None, "expected {invalid} to fail");
        }
    }

    #[test]
    fn fallback_badge_color_is_deterministic() {
        for key in ["discord", "steam", "kitty", "org.gnome.Nautilus"] {
            assert_eq!(badge_rgb(key), badge_rgb(key));
            assert_eq!(badge_color(key).a, 0xff);
        }
    }

    #[test]
    fn premultiplying_scales_channels_by_alpha() {
        let half_red = Color::rgba(255, 0, 0, 51);
        assert_eq!(half_red.premultiplied(), [0.2, 0.0, 0.0, 0.2]);
    }
}
