//! Light and dark colors of the shell chrome (see `docs/maquettes/`).

use clap::ValueEnum;

pub use coquille_theme::Color as Rgba;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ValueEnum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeKind {
    Light,
    Dark,
}

impl From<ThemeKind> for coquille_theme::ThemeKind {
    fn from(kind: ThemeKind) -> Self {
        match kind {
            ThemeKind::Light => coquille_theme::ThemeKind::Light,
            ThemeKind::Dark => coquille_theme::ThemeKind::Dark,
        }
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
    /// Translucent glass for chrome surfaces.
    pub glass: Rgba,
    /// Strong glass for menus, tooltips, mixer.
    pub glass_strong: Rgba,
    /// 1px edge rim.
    pub rim: Rgba,
    /// Card on top of glass.
    pub card: Rgba,
    /// Soft shadows.
    pub shadow: Rgba,
}

impl Theme {
    pub const fn new(kind: ThemeKind) -> Self {
        let theme_kind = match kind {
            ThemeKind::Light => coquille_theme::ThemeKind::Light,
            ThemeKind::Dark => coquille_theme::ThemeKind::Dark,
        };
        let tokens = coquille_theme::Tokens::new(theme_kind);
        Self {
            kind,
            backdrop: tokens.backdrop,
            surface: tokens.surface,
            titlebar: tokens.titlebar,
            separator: tokens.separator,
            text: tokens.text,
            text_muted: tokens.muted,
            disabled: tokens.disabled,
            accent: tokens.accent,
            button_hover: tokens.button_hover,
            close_hover: tokens.close_hover,
            glass: tokens.glass,
            glass_strong: tokens.glass_strong,
            rim: tokens.rim,
            card: tokens.card,
            shadow: tokens.shadow,
        }
    }

    /// The same theme with the accent the user chose.
    pub const fn with_accent(mut self, accent: Rgba) -> Self {
        self.accent = accent;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_color_is_read_from_its_hexadecimal_form() {
        assert_eq!(Rgba::parse("#7aa7ff"), Some(Rgba::rgb(0x7a_a7_ff)));
        assert_eq!(Rgba::parse("#7AA7FF"), Some(Rgba::rgb(0x7a_a7_ff)));
    }

    #[test]
    fn anything_else_is_not_a_color() {
        for text in ["7aa7ff", "#7aa7f", "#7aa7ff00", "#gggggg", "", "#+12345"] {
            assert_eq!(Rgba::parse(text), None, "{text}");
        }
    }

    #[test]
    fn a_color_is_written_back_as_it_was_read() {
        assert_eq!(Rgba::rgb(0x2f_6f_ed).hex(), "#2f6fed");
    }

    #[test]
    fn the_accent_of_a_theme_can_be_replaced() {
        let theme = Theme::new(ThemeKind::Dark).with_accent(Rgba::rgb(0xff_00_00));

        assert_eq!(theme.accent, Rgba::rgb(0xff_00_00));
        assert_eq!(theme.text, Theme::new(ThemeKind::Dark).text);
    }

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
        assert_eq!(Theme::new(ThemeKind::Light).accent, Rgba::rgb(0x5b_63_e6));
        assert_eq!(Theme::new(ThemeKind::Dark).accent, Rgba::rgb(0x8f_97_ff));
    }
}
