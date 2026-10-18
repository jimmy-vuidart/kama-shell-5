//! Toasts: capsules of the dock's matter, thickness and roundness, piled up
//! in the bottom corner of the screen on the dock's side (geometry in the
//! model, `toast_geometry`). The newest sits in the corner.

use iced::widget::text::Wrapping;
use iced::widget::{Column, Row, button, column, container, image, mouse_area, row, text};
use iced::{Alignment, ContentFit, Element, Length, Padding};

use super::icons::{self, Glyph, Icon as ThemeIcon};
use super::style::{self, Palette};
use crate::app::{Dock, Message, ToastSurface};
use crate::model::notify::{self, Icon, Notice};

/// Buttons a capsule shows besides its cross; the others wait for the app.
const ACTIONS: usize = 2;

pub fn view<'a>(dock: &'a Dock, surface: &ToastSurface) -> Element<'a, Message> {
    let palette = dock.palette();
    let mut notices = dock.notify.visible_notices();
    notices.truncate(surface.shown);
    // Newest first in the model; the newest goes at the bottom, in the corner.
    let capsules = notices
        .into_iter()
        .rev()
        .map(|notice| capsule(dock, notice, palette));
    let pile = Column::with_children(capsules).spacing(notify::TOAST_GAP as f32);
    let (top, right, bottom, left) = surface.geometry.padding;
    container(pile)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: top as f32,
            right: right as f32,
            bottom: bottom as f32,
            left: left as f32,
        })
        .align_y(Alignment::End)
        .into()
}

/// One notice, as the dock bar looks: its icon where a dock icon would be,
/// the summary over a line of body, its buttons and a cross.
fn capsule<'a>(dock: &'a Dock, notice: &'a Notice, palette: Palette) -> Element<'a, Message> {
    let metrics = dock.metrics();
    let id = notice.id;
    let picture = notice_icon(dock, notice, metrics.icon_size);

    let title = if notice.summary.is_empty() {
        &notice.app
    } else {
        &notice.summary
    };
    let body = notify::plain_text(&notice.body).replace('\n', " ");
    let detail = if body.trim().is_empty() {
        notice.app.clone()
    } else {
        body
    };
    let words = column![
        text(title.as_str())
            .size(13)
            .font(style::SEMIBOLD)
            .color(palette.text)
            .wrapping(Wrapping::None),
        text(detail)
            .size(12)
            .color(palette.muted)
            .wrapping(Wrapping::None),
    ]
    .spacing(1);
    let words = mouse_area(container(words).width(Length::Fill).clip(true))
        .on_press(Message::NoticeOpen(id));

    let actions = notice
        .actions
        .iter()
        .filter(|(key, _)| key != "default")
        .take(ACTIONS)
        .map(|(key, label)| {
            button(
                container(
                    text(label.as_str())
                        .size(12)
                        .font(style::MEDIUM)
                        .color(palette.accent)
                        .wrapping(Wrapping::None),
                )
                .max_width(80.0)
                .clip(true),
            )
            .padding(Padding::from([4.0, 8.0]))
            .on_press(Message::NoticeAction {
                id,
                key: key.clone(),
            })
            .style(style::flat(palette.hover, metrics.tile_radius))
            .into()
        });
    let actions = Row::with_children(actions).spacing(2);

    let close = button(icons::glyph(Glyph::Close, 12.0, palette.muted))
        .padding(6)
        .on_press(Message::NoticeDismiss(id))
        .style(style::flat(palette.hover, metrics.tile_radius));

    // The icon sits where the dock's icons do, as far from the edges.
    let inset = (metrics.width - metrics.icon_size) / 2.0;
    let inner = row![picture, words, actions, close]
        .spacing(10)
        .align_y(Alignment::Center);
    container(inner)
        .width(notify::TOAST_WIDTH as f32)
        .height(metrics.width)
        .padding(Padding {
            top: 0.0,
            right: inset / 2.0,
            bottom: 0.0,
            left: inset,
        })
        .center_y(metrics.width)
        .clip(true)
        .style(move |_| iced::widget::container::Style {
            shadow: palette.shadow(),
            ..style::outlined(palette.glass, palette.rim, metrics.radius)
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
