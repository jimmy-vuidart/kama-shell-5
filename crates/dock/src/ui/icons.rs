//! Icons: the ones of applications, found in the icon theme of the system,
//! and the few symbols the dock draws itself.

use std::collections::HashMap;
use std::path::Path;

use iced::widget::{container, image, svg, text};
use iced::{Color, ContentFit, Element, Length};

use super::style;
use crate::model::Identity;

/// A symbol of the dock, drawn as strokes on a 24 × 24 grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Launcher,
    Search,
    ChevronLeft,
    ChevronRight,
    Play,
    Volume,
    Muted,
    Sun,
    Network,
    Bluetooth,
    DoNotDisturb,
    Moon,
    Contrast,
    Sliders,
    Logout,
    Restart,
    Power,
    Plus,
    Bookmark,
    History,
    Close,
    Headphones,
    Clock,
}

impl Glyph {
    /// What is drawn inside the `<svg>` element.
    fn shapes(self) -> &'static str {
        match self {
            Self::Launcher => {
                r#"<g fill="black" stroke="none"><rect x="3" y="3" width="8" height="8" rx="2.2"/><rect x="13" y="3" width="8" height="8" rx="2.2" opacity=".75"/><rect x="3" y="13" width="8" height="8" rx="2.2" opacity=".75"/><rect x="13" y="13" width="8" height="8" rx="2.2" opacity=".55"/></g>"#
            }
            Self::Search => r#"<circle cx="11" cy="11" r="7"/><path d="M20.5 20.5 16 16"/>"#,
            Self::ChevronLeft => r#"<path d="m15 6-6 6 6 6"/>"#,
            Self::ChevronRight => r#"<path d="m9 6 6 6-6 6"/>"#,
            Self::Play => r#"<path d="M9 6.5v11l9-5.5z" fill="black"/>"#,
            Self::Volume => {
                r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z"/><path d="M15.5 9a4 4 0 0 1 0 6"/><path d="M18 6.5a7.5 7.5 0 0 1 0 11"/>"#
            }
            Self::Muted => {
                r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z"/><path d="m16 9.5 5 5"/><path d="m21 9.5-5 5"/>"#
            }
            Self::Sun => {
                r#"<circle cx="12" cy="12" r="4"/><path d="M12 2.5v2M12 19.5v2M2.5 12h2M19.5 12h2M5.3 5.3l1.4 1.4M17.3 17.3l1.4 1.4M5.3 18.7l1.4-1.4M17.3 6.7l1.4-1.4"/>"#
            }
            Self::Network => {
                r#"<rect x="9" y="3" width="6" height="5" rx="1"/><rect x="3" y="16" width="6" height="5" rx="1"/><rect x="15" y="16" width="6" height="5" rx="1"/><path d="M12 8v4M6 16v-2.5h12V16"/>"#
            }
            Self::Bluetooth => r#"<path d="m7 7.5 10 9-5 4.5V3l5 4.5-10 9"/>"#,
            Self::DoNotDisturb => r#"<circle cx="12" cy="12" r="9"/><path d="M8 12h8"/>"#,
            Self::Moon => r#"<path d="M20.5 13.5A8.5 8.5 0 1 1 10.5 3.5a6.8 6.8 0 0 0 10 10z"/>"#,
            Self::Contrast => {
                r#"<circle cx="12" cy="12" r="9"/><path d="M12 3a9 9 0 0 0 0 18z" fill="black"/>"#
            }
            Self::Sliders => {
                r#"<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>"#
            }
            Self::Logout => {
                r#"<path d="M10 4H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h4"/><path d="m15 8 4 4-4 4"/><path d="M19 12H9"/>"#
            }
            Self::Restart => r#"<path d="M20 12a8 8 0 1 1-2.6-5.9"/><path d="M20 4v5h-5"/>"#,
            Self::Power => r#"<path d="M12 3v9"/><path d="M17.7 6.8a8 8 0 1 1-11.4 0"/>"#,
            Self::Plus => r#"<path d="M12 5v14M5 12h14"/>"#,
            Self::Bookmark => r#"<path d="M7 4h10v16l-5-3.5L7 20z"/>"#,
            Self::History => {
                r#"<path d="M4 12a8 8 0 1 0 2.6-5.9"/><path d="M4 4v5h5"/><path d="M12 8v4.5l3 1.5"/>"#
            }
            Self::Close => r#"<path d="m6 6 12 12M18 6 6 18"/>"#,
            Self::Headphones => {
                r#"<path d="M4 15v-3a8 8 0 0 1 16 0v3"/><rect x="3" y="14" width="4" height="6" rx="1.5"/><rect x="17" y="14" width="4" height="6" rx="1.5"/>"#
            }
            Self::Clock => r#"<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>"#,
        }
    }

    fn handle(self) -> svg::Handle {
        let document = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
            self.shapes()
        );
        svg::Handle::from_memory(document.into_bytes())
    }
}

/// A symbol, of a size and a color.
pub fn glyph<'a, Message: 'a>(glyph: Glyph, size: f32, color: Color) -> Element<'a, Message> {
    svg(glyph.handle())
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
        .into()
}

/// The image of an icon, ready to be drawn.
#[derive(Debug, Clone)]
pub enum Icon {
    Vector(svg::Handle),
    Raster(image::Handle),
}

impl Icon {
    fn from_path(path: &Path) -> Self {
        let is_svg = path.extension().is_some_and(|extension| extension == "svg");
        if is_svg {
            Self::Vector(svg::Handle::from_path(path))
        } else {
            Self::Raster(image::Handle::from_path(path))
        }
    }
}

/// The icons of applications, found once and kept.
#[derive(Debug, Clone, Default)]
pub struct Icons {
    found: HashMap<String, Icon>,
}

impl Icons {
    /// Looks every name up in the icon theme. This reads many directories:
    /// to do away from the interface.
    pub fn load(names: Vec<String>) -> Self {
        let theme = coquille_apps::icon_theme();
        let mut found = HashMap::new();
        for name in names {
            if let Some(path) = coquille_apps::find_icon(&name, &theme) {
                found.insert(name, Icon::from_path(&path));
            }
        }
        Self { found }
    }

    pub fn get(&self, name: &str) -> Option<&Icon> {
        self.found.get(name)
    }
}

/// The icon of an application at `size`: the picture of a game, the icon of
/// the theme, or a colored tile with its initial when it has none.
pub fn app<'a, Message: 'a>(icons: &Icons, identity: &Identity, size: f32) -> Element<'a, Message> {
    if let Some(cover) = &identity.cover {
        return image(image::Handle::from_path(cover))
            .width(size)
            .height(size)
            .content_fit(ContentFit::Cover)
            .border_radius(size * 0.22)
            .into();
    }
    match identity.icon.as_deref().and_then(|name| icons.get(name)) {
        Some(Icon::Vector(handle)) => svg(handle.clone()).width(size).height(size).into(),
        Some(Icon::Raster(handle)) => image(handle.clone())
            .width(size)
            .height(size)
            .content_fit(ContentFit::Contain)
            .into(),
        None => initial(&identity.name, &identity.key, size),
    }
}

/// A rounded tile of a color that depends on the application, with the
/// first letter of its name.
fn initial<'a, Message: 'a>(name: &str, key: &str, size: f32) -> Element<'a, Message> {
    let letter = name
        .chars()
        .next()
        .map_or('?', |letter| letter.to_ascii_uppercase());
    let label = text(letter.to_string())
        .size(size * 0.5)
        .font(style::BOLD)
        .color(Color::WHITE);
    let color = tile_color(key);
    container(label)
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size))
        .style(move |_| style::fill(color, size * 0.26))
        .into()
}

/// One of a few pleasant colors, always the same for an application.
pub fn tile_color(key: &str) -> Color {
    const COLORS: [(u8, u8, u8); 8] = [
        (0x25, 0x63, 0xeb),
        (0x63, 0x66, 0xf1),
        (0xea, 0x58, 0x0c),
        (0x7c, 0x3a, 0xed),
        (0xca, 0x8a, 0x04),
        (0x0d, 0x94, 0x88),
        (0xdb, 0x27, 0x77),
        (0x47, 0x55, 0x69),
    ];
    let sum = key.bytes().fold(0_usize, |sum, byte| {
        sum.wrapping_mul(31).wrapping_add(byte.into())
    });
    let (r, g, b) = COLORS[sum % COLORS.len()];
    Color::from_rgb8(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_application_always_gets_the_same_color() {
        assert_eq!(tile_color("discord"), tile_color("discord"));
    }

    #[test]
    fn every_symbol_is_a_well_formed_drawing() {
        let glyphs = [
            Glyph::Launcher,
            Glyph::Search,
            Glyph::ChevronLeft,
            Glyph::ChevronRight,
            Glyph::Play,
            Glyph::Volume,
            Glyph::Muted,
            Glyph::Sun,
            Glyph::Network,
            Glyph::Bluetooth,
            Glyph::DoNotDisturb,
            Glyph::Moon,
            Glyph::Contrast,
            Glyph::Sliders,
            Glyph::Logout,
            Glyph::Restart,
            Glyph::Power,
            Glyph::Plus,
            Glyph::Bookmark,
            Glyph::History,
            Glyph::Close,
            Glyph::Headphones,
            Glyph::Clock,
        ];
        for glyph in glyphs {
            let shapes = glyph.shapes();
            assert_eq!(
                shapes.matches('<').count(),
                shapes.matches('>').count(),
                "{glyph:?}"
            );
            assert_eq!(shapes.matches('"').count() % 2, 0, "{glyph:?}");
        }
    }
}
