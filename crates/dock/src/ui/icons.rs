//! Icons: the ones of applications, found in the icon theme of the shell
//! (Reversal, see `coquille_apps::icon_theme_for`). The few symbols the shell
//! draws itself come from `coquille-ui`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use coquille_ipc::ThemeName;
use coquille_theme::{Color as Rgb, ThemeKind, Tokens};
pub use coquille_ui::glyph::{Glyph, glyph};
use iced::widget::{container, image, space, stack, svg, text};
use iced::{Border, Color, ContentFit, Element, Length};

use super::style::{self, Palette};
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
    /// Those whose own rounded square fills the tile (see [`fits_the_tile`]).
    fitting: HashSet<String>,
}

impl Icons {
    /// Looks every name up in the icon theme that goes with `style`. This
    /// reads many directories: to do away from the interface.
    pub fn load(names: Vec<String>, style: IconStyle) -> Self {
        let theme = coquille_apps::icon_theme_for(style.0, style.1);
        tracing::info!(theme, "icon theme");
        let mut found = HashMap::new();
        let mut fitting = HashSet::new();
        for name in names {
            if let Some(path) = coquille_apps::find_icon(&name, &theme) {
                if fits_the_tile(&path) {
                    fitting.insert(name.clone());
                }
                found.insert(name, Icon::from_path(&path));
            }
        }
        Self {
            chosen: Some((style, theme)),
            found,
            fitting,
        }
    }

    pub fn get(&self, name: &str) -> Option<&Icon> {
        self.found.get(name)
    }

    /// Whether the icon named `name` draws its own rounded square, as wide
    /// as a tile.
    pub fn fits(&self, name: &str) -> bool {
        self.fitting.contains(name)
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

/// Whether an icon is one of the Reversal theme, whose rounded square fills
/// the same share of every icon: the others (hicolor, pixmaps) have shapes
/// of their own.
pub fn fits_the_tile(path: &Path) -> bool {
    path.components().any(|part| {
        part.as_os_str()
            .to_str()
            .is_some_and(|name| name.starts_with("Reversal"))
    })
}

/// The icon of an application on a tile of the dock: a rounded square of
/// `side`, centered in a box of `slot`, with a faint shadow. An icon of the
/// theme covers the tile with its own square; another one sits on a light
/// plate, smaller, as the white tiles of the mockup; the picture of a game
/// fills it. `lifted` is the tile under the pointer.
pub fn tile<'a, Message: 'a>(
    icons: &Icons,
    identity: &Identity,
    slot: f32,
    (side, radius): (f32, f32),
    palette: Palette,
    lifted: bool,
) -> Element<'a, Message> {
    let fits = identity
        .icon
        .as_deref()
        .is_some_and(|name| icons.fits(name));
    let shadow = if lifted {
        palette.tile_shadow_lifted()
    } else {
        palette.tile_shadow()
    };
    let found = identity.icon.as_deref().and_then(|name| icons.get(name));
    // Under what fills the tile, the plate only carries the shadow: a pixel
    // smaller, it never shows past it.
    let covered = fits || identity.cover.is_some() || found.is_none();
    let plate_side = if covered { side - 2.0 } else { side };
    let plate =
        container(space().width(plate_side).height(plate_side)).style(move |_| container::Style {
            shadow,
            border: Border {
                color: palette.rim_bright,
                width: if covered { 0.0 } else { 0.5 },
                radius: radius.into(),
            },
            ..style::fill(palette.glass_strong, radius)
        });
    let picture: Element<'a, Message> = if identity.cover.is_some() {
        app(icons, identity, side)
    } else if fits {
        app(icons, identity, slot)
    } else if found.is_some() {
        app(icons, identity, (side * 0.68).round())
    } else {
        initial(&identity.name, &identity.key, side)
    };
    let centered = |content: Element<'a, Message>| -> Element<'a, Message> {
        container(content).center_x(slot).center_y(slot).into()
    };
    let mut layers = stack![centered(plate.into()), centered(picture)];
    if lifted {
        let veil = container(space().width(side).height(side))
            .style(move |_| style::fill(style::alpha(Color::WHITE, 0.14), radius));
        layers = layers.push(centered(veil.into()));
    }
    layers.into()
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
    fn only_the_icons_of_reversal_fill_the_tile() {
        let reversal = Path::new("/usr/share/icons/Reversal-purple-dark/apps/scalable/kitty.svg");
        assert!(fits_the_tile(reversal));
        assert!(fits_the_tile(Path::new(
            "/usr/share/icons/Reversal/apps/scalable/firefox.svg"
        )));
        assert!(!fits_the_tile(Path::new(
            "/usr/share/icons/hicolor/48x48/apps/antigravity.png"
        )));
        assert!(!fits_the_tile(Path::new("/usr/share/pixmaps/steam.png")));
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
