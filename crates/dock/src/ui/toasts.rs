//! Overlay toast cards glued to the end of a dock bar.
//!
//! Their geometry is in the model (`toast_geometry`): along an upright bar
//! the cards pile up from its bottom end, along a lying bar they line up
//! leftwards from its right end.

use iced::widget::text::Wrapping;
use iced::widget::{Column, Row, button, column, container, image, mouse_area, row, text};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use super::icons::{self, Glyph, Icon as ThemeIcon};
use super::style::{self, Palette};
use crate::app::{Dock, Message};
use crate::model::Orientation;
use crate::model::notify::{self, Icon, Notice};

pub fn view<'a>(dock: &'a Dock, orientation: Orientation) -> Element<'a, Message> {
    let palette = dock.palette();
    let notices = dock.notify.visible_notices();
    let cards: Vec<Element<'a, Message>> = notices
        .into_iter()
        .map(|notice| card(dock, notice, palette, orientation))
        .collect();
    let stack: Element<'a, Message> = match orientation {
        Orientation::Vertical(_) => Column::with_children(cards)
            .spacing(notify::TOAST_STACK_GAP as f32)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Start)
            .into(),
        Orientation::Horizontal => Row::with_children(cards)
            .spacing(notify::TOAST_STACK_GAP as f32)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .into(),
    };
    let aligned = match orientation {
        Orientation::Vertical(_) => container(stack)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End),
        Orientation::Horizontal => container(stack)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::End),
    };
    aligned.padding(0).into()
}

fn card<'a>(
    dock: &'a Dock,
    notice: &'a Notice,
    palette: Palette,
    orientation: Orientation,
) -> Element<'a, Message> {
    let (card_w, card_h) = match orientation {
        Orientation::Vertical(_) => (
            notify::TOAST_CARD_W_VERTICAL as f32,
            notify::TOAST_CARD_H_VERTICAL as f32,
        ),
        Orientation::Horizontal => (
            notify::TOAST_CARD_W_HORIZONTAL as f32,
            notify::TOAST_CARD_H_HORIZONTAL as f32,
        ),
    };
    let id = notice.id;
    let picture = notice_icon(dock, notice, 32.0);
    let summary = text(if notice.summary.is_empty() {
        notice.app.clone()
    } else {
        notice.summary.clone()
    })
    .size(13)
    .font(style::SEMIBOLD)
    .color(palette.text)
    .wrapping(Wrapping::None)
    .line_height(1.2);
    let summary = container(summary).width(Length::Fill).height(16).clip(true);
    let body = if notice.body.is_empty() {
        None
    } else {
        let lines = match orientation {
            Orientation::Vertical(_) => 32.0,
            Orientation::Horizontal => 16.0,
        };
        Some(
            container(
                text(notify::plain_text(&notice.body))
                    .size(12)
                    .color(palette.muted)
                    .wrapping(Wrapping::Word)
                    .line_height(1.2),
            )
            .width(Length::Fill)
            .height(lines)
            .clip(true),
        )
    };
    let mut words: Column<'a, Message> = column![summary].spacing(2);
    if let Some(body) = body {
        words = words.push(body);
    }
    let actions: Vec<Element<'a, Message>> = notice
        .actions
        .iter()
        .filter(|(key, _)| key != "default")
        .map(|(key, label)| {
            button(
                container(
                    text(label.clone())
                        .size(12)
                        .font(style::MEDIUM)
                        .color(palette.accent)
                        .wrapping(Wrapping::None),
                )
                .max_width(72.0)
                .clip(true),
            )
            .padding(Padding::from([4.0, 8.0]))
            .on_press(Message::NoticeAction {
                id,
                key: key.clone(),
            })
            .style(style::flat(palette.hover, 8.0))
            .into()
        })
        .collect();
    if !actions.is_empty() {
        words = words.push(
            container(Row::with_children(actions).spacing(4).wrap())
                .width(Length::Fill)
                .height(24)
                .clip(true),
        );
    }

    let close = button(icons::glyph(Glyph::Close, 12.0, palette.muted))
        .padding(4)
        .on_press(Message::NoticeDismiss(id))
        .style(style::flat(palette.hover, 8.0));

    let body_hit = mouse_area(words.width(Length::Fill).height(Length::Fill))
        .on_press(Message::NoticeOpen(id));
    let inner = row![picture, body_hit, close]
        .spacing(10)
        .align_y(Alignment::Start)
        .height(Length::Fill);

    container(inner)
        .padding(12)
        .width(card_w)
        .height(card_h)
        .clip(true)
        .style(move |_| {
            let mut fill = style::outlined(palette.glass, palette.rim, 12.0);
            fill.shadow = palette.shadow();
            fill
        })
        .into()
}

/// The picture of a notice: the image it carries, else its icon, else the
/// icon of its application.
pub fn notice_icon<'a>(dock: &'a Dock, notice: &'a Notice, size: f32) -> Element<'a, Message> {
    let source = notice.image.as_ref().or(notice.icon.as_ref());
    match source {
        Some(Icon::Pixels {
            width,
            height,
            rgba,
        }) => {
            return image(image::Handle::from_rgba(*width, *height, rgba.clone()))
                .width(size)
                .height(size)
                .content_fit(ContentFit::Contain)
                .into();
        }
        Some(Icon::File(path)) => {
            let path = path.strip_prefix("file://").unwrap_or(path);
            return image(image::Handle::from_path(path))
                .width(size)
                .height(size)
                .content_fit(ContentFit::Contain)
                .into();
        }
        Some(Icon::Named(name)) => {
            if let Some(icon) = dock.icons.get(name) {
                return themed(icon.clone(), size);
            }
            // Looked up once, then cached by the icon lookup.
            if let Some(path) = coquille_apps::find_icon(name, dock.icons.theme()) {
                return themed(ThemeIcon::from_path(&path), size);
            }
        }
        None => {}
    }
    let app = notice.desktop_entry.as_deref().unwrap_or(&notice.app);
    icons::app(&dock.icons, &dock.identify(app), size)
}

fn themed<'a>(icon: ThemeIcon, size: f32) -> Element<'a, Message> {
    match icon {
        ThemeIcon::Vector(handle) => iced::widget::svg(handle).width(size).height(size).into(),
        ThemeIcon::Raster(handle) => image(handle)
            .width(size)
            .height(size)
            .content_fit(ContentFit::Contain)
            .into(),
    }
}
