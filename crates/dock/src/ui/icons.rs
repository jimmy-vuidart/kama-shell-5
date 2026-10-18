//! Icons: the ones of applications, found in the icon theme of the shell
//! (Reversal, see `coquille_apps::icon_theme_for`). The few symbols the shell
//! draws itself come from `coquille-ui`.

use std::collections::HashMap;
use std::path::Path;

use coquille_ipc::ThemeName;
use coquille_theme::{Color as Rgb, ThemeKind, Tokens};
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
    pub fn from_path(path: &Path) -> Self {
        let is_svg = path.extension().is_some_and(|extension| extension == "svg");
        if is_svg {
            Self::Vector(svg::Handle::from_path(path))
        } else {
            Self::Raster(image::Handle::from_path(path))
        }
    }
}

/// What the icon theme is chosen from: the theme and the accent.
pub type IconStyle = (ThemeKind, Rgb);

/// The style of a theme and an accent as the compositor tells them; an
/// accent that is not a color is the one of the theme.
pub fn style(theme: ThemeName, accent: &str) -> IconStyle {
    let kind = match theme {
        ThemeName::Light => ThemeKind::Light,
        ThemeName::Dark => ThemeKind::Dark,
    };
    let accent = Rgb::parse(accent).unwrap_or(Tokens::new(kind).accent);
    (kind, accent)
}

/// The icons of applications, found once per theme and accent, and kept.
#[derive(Debug, Clone, Default)]
pub struct Icons {
    /// What they were found for, and the name of the icon theme.
    chosen: Option<(IconStyle, String)>,
    found: HashMap<String, Icon>,
}

impl Icons {
    /// Looks every name up in the icon theme that goes with `style`. This
    /// reads many directories: to do away from the interface.
    pub fn load(names: Vec<String>, style: IconStyle) -> Self {
        let theme = coquille_apps::icon_theme_for(style.0, style.1);
        tracing::info!(theme, "icon theme");
        let mut found = HashMap::new();
        for name in names {
            if let Some(path) = coquille_apps::find_icon(&name, &theme) {
                found.insert(name, Icon::from_path(&path));
            }
        }
        Self {
            chosen: Some((style, theme)),
            found,
        }
    }

    pub fn get(&self, name: &str) -> Option<&Icon> {
        self.found.get(name)
    }

    pub fn style(&self) -> Option<IconStyle> {
        self.chosen.as_ref().map(|(style, _)| *style)
    }

    /// The name of the icon theme, for icons looked up later.
    pub fn theme(&self) -> &str {
        self.chosen.as_ref().map_or("hicolor", |(_, theme)| theme)
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
    let [r, g, b] = coquille_theme::badge_rgb(key);
    Color::from_rgb8(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icons_follow_the_accent_of_the_theme_when_none_is_set() {
        let dark = style(ThemeName::Dark, "");
        assert_eq!(dark, (ThemeKind::Dark, Tokens::new(ThemeKind::Dark).accent));

        let chosen = style(ThemeName::Light, "#2f6fed");
        assert_eq!(chosen, (ThemeKind::Light, Rgb::rgb(0x2f_6f_ed)));
    }

    #[test]
    fn an_application_always_gets_the_same_color() {
        assert_eq!(tile_color("discord"), tile_color("discord"));
    }

    #[test]
    fn dock_tile_color_matches_theme_badge_rgb() {
        for app in ["discord", "steam", "org.gnome.Nautilus", "kitty"] {
            let [r, g, b] = coquille_theme::badge_rgb(app);
            assert_eq!(tile_color(app), Color::from_rgb8(r, g, b));
        }
    }
}
