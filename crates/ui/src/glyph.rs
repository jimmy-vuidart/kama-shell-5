//! The few symbols the shell draws itself.

use iced::widget::svg;
use iced::{Color, Element};

/// A symbol of the shell, drawn as strokes on a 24 × 24 grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Launcher,
    Search,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
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
    /// The pages of the settings screen.
    Brush,
    Picture,
    Screens,
    Gamepad,
    Mouse,
    Keyboard,
    Check,
    Camera,
    Crop,
    /// A laptop's battery, on battery and charging.
    Battery,
    BatteryCharging,
    /// The notifications.
    Bell,
    /// The password asked by polkit.
    Lock,
    Wifi,
    Speakers,
    Edit,
    Warning,
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
            Self::ChevronUp => r#"<path d="m6 15 6-6 6 6"/>"#,
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
            Self::Brush => {
                r#"<path d="M12 3a9 9 0 1 0 0 18c1.4 0 2-1 2-2 0-1.6-1.2-1.8-1.2-3.2 0-1 .8-1.8 2-1.8H17a4 4 0 0 0 4-4c0-3.9-4-7-9-7z"/><circle cx="7.5" cy="12" r="1" fill="black"/><circle cx="10" cy="7.8" r="1" fill="black"/><circle cx="15" cy="7.8" r="1" fill="black"/>"#
            }
            Self::Picture => {
                r#"<rect x="3" y="4.5" width="18" height="15" rx="2.5"/><path d="m3.5 17 5-5.5 4 4 2.5-2.5 5.5 5.5"/><circle cx="15.5" cy="9" r="1.6"/>"#
            }
            Self::Screens => {
                r#"<rect x="7.5" y="6" width="9" height="9" rx="1.5"/><path d="M5 8H3.5a1 1 0 0 0-1 1v4a1 1 0 0 0 1 1H5M19 8h1.5a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H19M9.5 19h5M12 15v4"/>"#
            }
            Self::Gamepad => {
                r#"<path d="M7.5 7h9a5 5 0 0 1 4.9 4l.9 4.6a2.4 2.4 0 0 1-4.2 2L16 15.5H8l-2.1 2.1a2.4 2.4 0 0 1-4.2-2L2.6 11A5 5 0 0 1 7.5 7z"/><path d="M8 9.5v3M6.5 11h3"/><circle cx="16" cy="11" r="1" fill="black"/>"#
            }
            Self::Mouse => {
                r#"<rect x="5.5" y="3" width="13" height="18" rx="6.5"/><path d="M12 7v4.5"/>"#
            }
            Self::Keyboard => {
                r#"<rect x="2.5" y="6" width="19" height="12" rx="2"/><path d="M6.5 10h.01M10.2 10h.01M13.8 10h.01M17.5 10h.01M8 14h8"/>"#
            }
            Self::Check => r#"<path d="m5 12.5 4.5 4.5L19 7.5"/>"#,
            Self::Camera => {
                r#"<path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"/><circle cx="12" cy="13" r="4"/>"#
            }
            Self::Crop => {
                r#"<path d="M6 2v14a2 2 0 0 0 2 2h14"/><path d="M18 22V8a2 2 0 0 0-2-2H2"/>"#
            }
            Self::Battery => {
                r#"<rect x="2.5" y="7" width="17" height="10" rx="2.5"/><path d="M22 10.5v3"/><rect x="5.5" y="10" width="7" height="4" rx="1" fill="black" stroke="none"/>"#
            }
            Self::BatteryCharging => {
                r#"<rect x="2.5" y="7" width="17" height="10" rx="2.5"/><path d="M22 10.5v3"/><path d="M12 9.2 9.5 12.4h3l-2.5 3.2"/>"#
            }
            Self::Bell => {
                r#"<path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.5 2h-15z"/><path d="M10 20.5a2.2 2.2 0 0 0 4 0"/>"#
            }
            Self::Lock => {
                r#"<rect x="4.5" y="10.5" width="15" height="10.5" rx="2.5"/><path d="M8 10.5V7.5a4 4 0 0 1 8 0v3"/><path d="M12 14.5v2.5"/>"#
            }
            Self::Wifi => {
                r#"<path d="M5 12.55a11 11 0 0 1 14.08 0"/><path d="M8.53 16.11a6 6 0 0 1 6.95 0"/><circle cx="12" cy="19.5" r="1" fill="black" stroke="none"/>"#
            }
            Self::Speakers => {
                r#"<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4z"/><path d="M15.5 9a4 4 0 0 1 0 6"/>"#
            }
            Self::Edit => {
                r#"<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/>"#
            }
            Self::Warning => {
                r#"<path d="M10.3 3.9 2.3 18a2 2 0 0 0 1.7 3h16a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z"/><path d="M12 9.5v4.5"/><circle cx="12" cy="17" r=".8" fill="black" stroke="none"/>"#
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_symbol_is_a_well_formed_drawing() {
        let glyphs = [
            Glyph::Launcher,
            Glyph::Search,
            Glyph::ChevronLeft,
            Glyph::ChevronRight,
            Glyph::ChevronUp,
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
            Glyph::Brush,
            Glyph::Screens,
            Glyph::Gamepad,
            Glyph::Mouse,
            Glyph::Keyboard,
            Glyph::Check,
            Glyph::Camera,
            Glyph::Crop,
            Glyph::Battery,
            Glyph::BatteryCharging,
            Glyph::Bell,
            Glyph::Lock,
            Glyph::Wifi,
            Glyph::Speakers,
            Glyph::Edit,
            Glyph::Warning,
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
