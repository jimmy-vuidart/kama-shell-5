//! Icons: the ones of applications, found in the icon theme of the system.
//! The few symbols the shell draws itself come from `coquille-ui`.

use std::collections::HashMap;
use std::path::Path;

pub use coquille_ui::glyph::{Glyph, glyph};
use iced::widget::{container, image, svg, text};
use iced::{Color, ContentFit, Element, Length};

use super::style;
use crate::model::Identity;

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
}
