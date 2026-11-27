//! Decoupled notifications-only and control-center-only popovers (+ mixer) for spatial mode.

use iced::widget::{Row, container, mouse_area, space, stack};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::app::{Dock, Message, SpatialPopover, SpatialPopoverKind};
use crate::model::Side;
use crate::ui::controls::{self, ControlView};
use crate::ui::opaque::opaque;
use crate::ui::style::{self, Palette};

pub const POPOVER_WIDTH: f32 = 348.0;
pub const NOTIFICATIONS_HEIGHT: f32 = 420.0;
pub const EMPTY_NOTIFICATIONS_HEIGHT: f32 = 240.0;
pub const EDGE_MARGIN: f32 = 12.0;
pub const MIXER_GAP: f32 = 10.0;

const POPOVER_RADIUS: f32 = 18.0;
const POPOVER_PADDING: f32 = 14.0;

/// Returns the left-to-right visual order of panels inside the spatial popover
/// row for the given sidebar `side` and `mixer_open` state.
#[cfg(test)]
pub fn popover_order(side: Side, mixer_open: bool) -> &'static [&'static str] {
    match (side, mixer_open) {
        (_, false) => &["popover"],
        (Side::Left, true) => &["popover", "mixer"],
        (Side::Right, true) => &["mixer", "popover"],
    }
}

/// Renders the full-screen transparent backdrop and the decoupled spatial
/// popover (notifications or control center, plus optional per-app mixer).
pub fn view<'a>(
    dock: &'a Dock,
    popover: &'a SpatialPopover,
    palette: Palette,
) -> Element<'a, Message> {
    let backdrop = mouse_area(
        container(space())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| style::fill(Color::TRANSPARENT, 0.0)),
    )
    .on_press(Message::CloseSpatialPopover);

    let body: Element<'a, Message> = match popover.kind {
        SpatialPopoverKind::Notifications => {
            let height = if dock.notify.history.is_empty() {
                EMPTY_NOTIFICATIONS_HEIGHT
            } else {
                NOTIFICATIONS_HEIGHT
            };
            container(controls::notifications(dock, palette))
                .height(height)
                .max_height(NOTIFICATIONS_HEIGHT)
                .into()
        }
        SpatialPopoverKind::Controls => {
            let view = ControlView {
                side: popover.side,
                show_mixer_toggle: true,
                mixer_open: popover.mixer,
                confirm: popover.confirm,
                screenshot_prompt: popover.screenshot_prompt,
                note: popover.note.as_deref(),
            };
            controls::control_center(dock, view, palette)
        }
    };

    let popover_card = opaque(
        container(body)
            .width(POPOVER_WIDTH)
            .padding(POPOVER_PADDING)
            .style(move |_| container::Style {
                shadow: palette.shadow(),
                ..style::outlined(palette.glass_strong, palette.rim, POPOVER_RADIUS)
            }),
    );

    let show_mixer = popover.kind == SpatialPopoverKind::Controls && popover.mixer;
    let content: Element<'a, Message> = if show_mixer {
        let mixer = opaque(controls::mixer_panel(dock, palette));
        let items = match popover.side {
            Side::Left => vec![popover_card, mixer],
            Side::Right => vec![mixer, popover_card],
        };
        Row::with_children(items)
            .spacing(MIXER_GAP)
            .align_y(Alignment::End)
            .into()
    } else {
        popover_card
    };

    let align_x = match popover.side {
        Side::Left => Alignment::Start,
        Side::Right => Alignment::End,
    };
    let foreground = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding::from([EDGE_MARGIN, EDGE_MARGIN]))
        .align_x(align_x)
        .align_y(Alignment::End);

    stack![backdrop, foreground].into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popover_order_mirrors_between_left_and_right() {
        assert_eq!(popover_order(Side::Left, false), &["popover"]);
        assert_eq!(popover_order(Side::Right, false), &["popover"]);
        assert_eq!(popover_order(Side::Left, true), &["popover", "mixer"]);
        assert_eq!(popover_order(Side::Right, true), &["mixer", "popover"]);
    }

    #[test]
    fn popover_dimensions_match_design_spec() {
        assert_eq!(POPOVER_WIDTH, 348.0);
        assert_eq!(NOTIFICATIONS_HEIGHT, 420.0);
        assert_eq!(EDGE_MARGIN, 12.0);
        assert_eq!(MIXER_GAP, 10.0);
        const { assert!(EMPTY_NOTIFICATIONS_HEIGHT < NOTIFICATIONS_HEIGHT) };
        assert_eq!(EMPTY_NOTIFICATIONS_HEIGHT, 240.0);
        assert_eq!(
            POPOVER_WIDTH + MIXER_GAP + controls::MIXER_WIDTH,
            348.0 + 10.0 + 292.0
        );
    }
}
