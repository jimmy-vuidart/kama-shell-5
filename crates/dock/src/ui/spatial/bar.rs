//! Permanent full-height 56 px Liquid Glass vertical sidebar for spatial mode.

use chrono::{DateTime, Datelike, Local, Timelike, Weekday};
use coquille_model::OutputRole;
use iced::alignment::{Horizontal, Vertical};
use iced::border::Radius;
use iced::widget::{
    Column, Row, button, column, container, mouse_area, row, scrollable, space, stack, text,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding};

use crate::app::{BarState, Dock, Message, SpatialPopoverKind};
use crate::model::bar::{Entry, SpatialIndicator, spatial_indicator};
use crate::model::battery::Charge;
use crate::model::{Orientation, Side};
use crate::ui::bar::tray_icon_element;
use crate::ui::icons::{self, Glyph};
use crate::ui::style::{self, Palette};

/// Fixed width of the permanent vertical sidebar in logical pixels.
pub const WIDTH: u32 = 56;
/// Exclusive zone reserved on the screen edge for the sidebar.
pub const EXCLUSIVE_ZONE: i32 = 56;
/// Extra transparent width on the inner side of the surface so hover tooltips
/// render cleanly without unfolding the bar.
pub const TOOLTIP_REACH: u32 = 240;
/// Total logical width of the layer-shell surface.
pub const SURFACE_WIDTH: u32 = WIDTH + TOOLTIP_REACH;

const PAD_Y: f32 = 10.0;
const TOP_GAP: f32 = 10.0;
const BTN_SIZE: f32 = 38.0;
const BTN_RADIUS: f32 = 11.0;
const CLOCK_HEIGHT: f32 = 28.0;
const SEP_WIDTH: f32 = 22.0;
const SEP_HEIGHT: f32 = 1.0;
const TASKBAR_GAP: f32 = 6.0;
const BOTTOM_GAP: f32 = 8.0;
const TRAY_ICON: f32 = 16.0;
const TRAY_GAP: f32 = 5.0;
const TRAY_PAD: f32 = 5.0;
const PILL_RADIUS: f32 = 14.0;
const PILL_TOP_HEIGHT: f32 = 34.0;
const PILL_BASE_HEIGHT: f32 = 92.0;
const TOOLTIP_GAP: f32 = 10.0;

fn bar_x(side: Side, surface_width: u32) -> i32 {
    match side {
        Side::Left => 0,
        Side::Right => surface_width.saturating_sub(WIDTH) as i32,
    }
}

fn side_of(orientation: Orientation) -> Side {
    match orientation {
        Orientation::Vertical(side) => side,
        Orientation::Horizontal => Side::Left,
    }
}

/// Interactive input region of the sidebar surface (strictly 56 px wide).
pub fn input_region(side: Side, surface_width: u32, height: u32) -> (i32, i32, i32, i32) {
    (bar_x(side, surface_width), 0, WIDTH as i32, height as i32)
}

/// Logical rectangle `(x, y, width, height)` of taskbar item `index` inside
/// the sidebar surface, used to anchor right-click context menus.
pub fn entry_menu_rect(
    side: Side,
    surface_width: u32,
    index: usize,
    pinned_count: usize,
) -> (i32, i32, i32, i32) {
    let taskbar_top =
        (PAD_Y + BTN_SIZE + TOP_GAP + CLOCK_HEIGHT + TOP_GAP + SEP_HEIGHT + TOP_GAP) as i32;
    let step = (BTN_SIZE + TASKBAR_GAP) as i32;
    let sep_offset = if pinned_count > 0 && index >= pinned_count {
        (TASKBAR_GAP + SEP_HEIGHT) as i32
    } else {
        0
    };
    let item_x = bar_x(side, surface_width) + ((WIDTH as i32 - BTN_SIZE as i32) / 2);
    let item_y = taskbar_top + (index as i32) * step + sep_offset;
    (item_x, item_y, BTN_SIZE as i32, BTN_SIZE as i32)
}

/// Logical rectangle `(x, y, width, height)` of tray item `index` inside the
/// sidebar surface, used to anchor tray context menus.
pub fn tray_menu_rect(
    side: Side,
    surface_width: u32,
    height: u32,
    tray_len: usize,
    index: usize,
) -> (i32, i32, i32, i32) {
    let bx = bar_x(side, surface_width);
    if tray_len == 0 {
        return (
            bx + 9,
            (height as i32) - (PAD_Y + PILL_BASE_HEIGHT + BOTTOM_GAP + TRAY_ICON) as i32,
            TRAY_ICON as i32,
            TRAY_ICON as i32,
        );
    }
    let rows = tray_len.div_ceil(2) as i32;
    let icon = TRAY_ICON as i32;
    let gap = TRAY_GAP as i32;
    let pad = TRAY_PAD as i32;
    let tray_w = pad * 2 + icon * 2 + gap + 2;
    let tray_h = pad * 2 + rows * icon + rows.saturating_sub(1) * gap + 2;
    let tray_x = bx + ((WIDTH as i32 - tray_w) / 2);
    let tray_y = (height as i32) - (PAD_Y + PILL_BASE_HEIGHT + BOTTOM_GAP) as i32 - tray_h;
    let row = (index / 2) as i32;
    let col = (index % 2) as i32;
    let x = tray_x + 1 + pad + col * (icon + gap);
    let y = tray_y + 1 + pad + row * (icon + gap);
    (x, y, icon, icon)
}

/// Formats `now` as a short uppercase French weekday + day number (e.g. `"VEN 27"`).
pub fn format_short_date(now: DateTime<Local>) -> String {
    let day = match now.weekday() {
        Weekday::Mon => "LUN",
        Weekday::Tue => "MAR",
        Weekday::Wed => "MER",
        Weekday::Thu => "JEU",
        Weekday::Fri => "VEN",
        Weekday::Sat => "SAM",
        Weekday::Sun => "DIM",
    };
    format!("{day} {}", now.day())
}

/// Renders the permanent 56 px Liquid Glass vertical sidebar and its optional
/// hover tooltip inside the `TOOLTIP_REACH` area.
pub fn view<'a>(dock: &'a Dock, bar: &'a BarState, palette: Palette) -> Element<'a, Message> {
    let side = side_of(bar.orientation);
    let launcher_visible = dock.spatial_launcher_visible_on(bar.screen);

    let top_block = column![
        launcher_button(bar.screen, side, launcher_visible, palette),
        clock_block(dock.now, palette),
        separator_line(palette),
    ]
    .spacing(TOP_GAP)
    .align_x(Alignment::Center)
    .width(Length::Fill);

    let taskbar_block = taskbar(dock, bar, side, launcher_visible, palette);
    let bottom_block = bottom_controls(dock, bar, palette);

    let bar_content = column![top_block, taskbar_block, bottom_block]
        .spacing(TOP_GAP)
        .align_x(Alignment::Center)
        .padding(Padding::from([PAD_Y, 0.0]))
        .width(WIDTH as f32)
        .height(Length::Fill);

    let bar_bg = container(bar_content)
        .width(WIDTH as f32)
        .height(Length::Fill)
        .style(move |_| style::fill(palette.glass, 0.0));

    let rim_align = match side {
        Side::Left => Horizontal::Right,
        Side::Right => Horizontal::Left,
    };
    let inner_rim = container(
        container(space().width(1).height(Length::Fill))
            .style(move |_| style::fill(palette.rim, 0.0)),
    )
    .width(WIDTH as f32)
    .height(Length::Fill)
    .align_x(rim_align);

    let sidebar: Element<'a, Message> = stack![bar_bg, inner_rim].into();
    let tooltip_area = hover_tooltip(bar, side, launcher_visible, palette);

    let layout = match side {
        Side::Left => row![sidebar, tooltip_area],
        Side::Right => row![tooltip_area, sidebar],
    };

    layout.width(Length::Fill).height(Length::Fill).into()
}

fn separator_line<'a>(palette: Palette) -> Element<'a, Message> {
    container(space().width(SEP_WIDTH).height(SEP_HEIGHT))
        .style(move |_| style::fill(palette.separator, 0.0))
        .into()
}

fn outer_edge_bar<'a>(side: Side, color: Color) -> Element<'a, Message> {
    let radius = match side {
        Side::Left => Radius {
            top_left: 0.0,
            top_right: 3.0,
            bottom_right: 3.0,
            bottom_left: 0.0,
        },
        Side::Right => Radius {
            top_left: 3.0,
            top_right: 0.0,
            bottom_right: 0.0,
            bottom_left: 3.0,
        },
    };
    container(space().width(3).height(20))
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            border: Border {
                radius,
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn wrap_with_outer_mark<'a>(
    tile: Element<'a, Message>,
    mark: Option<Element<'a, Message>>,
    side: Side,
    inset: f32,
) -> Element<'a, Message> {
    let centered_tile = container(tile)
        .width(WIDTH as f32)
        .height(BTN_SIZE)
        .center_x(WIDTH as f32)
        .center_y(BTN_SIZE);

    let Some(mark) = mark else {
        return centered_tile.into();
    };

    let (align, pad) = match side {
        Side::Left => (Horizontal::Left, Padding::ZERO.left(inset)),
        Side::Right => (Horizontal::Right, Padding::ZERO.right(inset)),
    };
    let positioned_mark = container(mark)
        .width(WIDTH as f32)
        .height(BTN_SIZE)
        .padding(pad)
        .align_x(align)
        .align_y(Vertical::Center);

    stack![centered_tile, positioned_mark].into()
}

fn launcher_button<'a>(
    screen: OutputRole,
    side: Side,
    active: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let icon = icons::glyph(Glyph::Launcher, 18.0, palette.text);
    let btn = button(container(icon).center_x(BTN_SIZE).center_y(BTN_SIZE))
        .width(BTN_SIZE)
        .height(BTN_SIZE)
        .padding(0)
        .on_press(Message::Launcher(Some(screen)))
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let bg = if active {
                if hovered { palette.hover } else { palette.card }
            } else if hovered {
                palette.hover
            } else {
                Color::TRANSPARENT
            };
            let border = if active {
                Border {
                    color: palette.rim,
                    width: 1.0,
                    radius: BTN_RADIUS.into(),
                }
            } else {
                Border {
                    radius: BTN_RADIUS.into(),
                    ..Border::default()
                }
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border,
                ..button::Style::default()
            }
        });

    let mark = active.then(|| outer_edge_bar(side, palette.accent));
    wrap_with_outer_mark(btn.into(), mark, side, 0.0)
}

fn clock_block<'a>(now: DateTime<Local>, palette: Palette) -> Element<'a, Message> {
    let hhmm = format!("{:02}:{:02}", now.hour(), now.minute());
    let date = format_short_date(now);

    let time_text = text(hhmm).size(12.5).font(style::BOLD).color(palette.text);
    let date_text = text(date)
        .size(9.5)
        .font(style::SEMIBOLD)
        .color(palette.muted);

    container(
        column![time_text, date_text]
            .spacing(2)
            .align_x(Alignment::Center),
    )
    .height(CLOCK_HEIGHT)
    .center_x(WIDTH as f32)
    .center_y(CLOCK_HEIGHT)
    .into()
}

fn taskbar<'a>(
    dock: &'a Dock,
    bar: &'a BarState,
    side: Side,
    launcher_visible: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let pinned_count = bar.pinned();
    let has_unpinned = bar.entries.len() > pinned_count;

    let mut items: Column<'a, Message> = column![].spacing(TASKBAR_GAP).align_x(Alignment::Center);
    for (index, entry) in bar.entries.iter().enumerate() {
        if index == pinned_count && pinned_count > 0 && has_unpinned {
            items = items.push(separator_line(palette));
        }
        items = items.push(app_tile(
            dock,
            bar,
            index,
            entry,
            side,
            launcher_visible,
            palette,
        ));
    }

    scrollable(items.width(WIDTH as f32))
        .width(WIDTH as f32)
        .height(Length::Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(0).scroller_width(0),
        ))
        .into()
}

fn app_tile<'a>(
    dock: &'a Dock,
    bar: &'a BarState,
    index: usize,
    entry: &'a Entry,
    side: Side,
    launcher_visible: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let hovered = bar.hover_entry == Some(index);
    let indicator = spatial_indicator(entry, bar.screen, launcher_visible);

    let bg = if hovered {
        palette.hover
    } else {
        match indicator {
            SpatialIndicator::ActiveHere => palette.card,
            SpatialIndicator::Here | SpatialIndicator::Elsewhere => {
                style::alpha(palette.card, palette.card.a * 0.65)
            }
            SpatialIndicator::None => Color::TRANSPARENT,
        }
    };

    let tile_icon = icons::tile(
        &dock.icons,
        &entry.identity,
        32.0,
        (25.0, 6.0),
        palette,
        hovered,
    );

    let tile_box = container(tile_icon)
        .center_x(BTN_SIZE)
        .center_y(BTN_SIZE)
        .style(move |_| {
            if indicator == SpatialIndicator::ActiveHere {
                style::outlined(bg, palette.rim, BTN_RADIUS)
            } else {
                style::fill(bg, BTN_RADIUS)
            }
        });

    let unread = dock.spatial_unread_for(entry);
    let tile_with_badge: Element<'a, Message> = if unread > 0 {
        let badge_el = container(unread_badge(unread))
            .width(BTN_SIZE)
            .height(BTN_SIZE)
            .align_x(Horizontal::Right)
            .align_y(Vertical::Top);
        stack![tile_box, badge_el].into()
    } else {
        tile_box.into()
    };

    let interactive = mouse_area(tile_with_badge)
        .on_enter(Message::EntryHover(bar.id, Some(index)))
        .on_exit(Message::EntryHover(bar.id, None))
        .on_press(Message::EntryPress(bar.id, index))
        .on_release(Message::EntryRelease(bar.id, index))
        .on_right_press(Message::EntryMenu(bar.id, index))
        .on_middle_press(Message::EntryMiddle(bar.id, index));

    let (mark, inset) = match indicator {
        SpatialIndicator::ActiveHere => (Some(outer_edge_bar(side, palette.accent)), 0.0),
        SpatialIndicator::Here => (
            Some(
                container(space().width(5).height(5))
                    .style(move |_| style::fill(style::alpha(palette.accent, 0.85), 2.5))
                    .into(),
            ),
            2.0,
        ),
        SpatialIndicator::Elsewhere => (
            Some(
                container(space().width(6).height(6))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(Color::TRANSPARENT)),
                        border: Border {
                            color: palette.accent,
                            width: 1.5,
                            radius: 3.0.into(),
                        },
                        ..container::Style::default()
                    })
                    .into(),
            ),
            2.0,
        ),
        SpatialIndicator::None => (None, 0.0),
    };

    wrap_with_outer_mark(interactive.into(), mark, side, inset)
}

fn unread_badge<'a>(count: usize) -> Element<'a, Message> {
    let label = if count > 99 {
        "99+".to_owned()
    } else {
        count.to_string()
    };
    let text_el = text(label).size(9.5).font(style::BOLD).color(Color::WHITE);
    container(text_el)
        .center_x(Length::Shrink)
        .center_y(16.0)
        .padding(Padding::from([0.0, 4.0]))
        .style(|_| style::fill(style::DANGER, 8.0))
        .into()
}

fn bottom_controls<'a>(
    dock: &'a Dock,
    bar: &'a BarState,
    palette: Palette,
) -> Element<'a, Message> {
    let mut col: Column<'a, Message> = column![].spacing(BOTTOM_GAP).align_x(Alignment::Center);

    if !dock.tray.is_empty() {
        col = col.push(compact_tray(dock, bar, palette));
    }
    col = col.push(split_pill(dock, bar, palette));
    col.width(WIDTH as f32).into()
}

fn compact_tray<'a>(dock: &'a Dock, bar: &'a BarState, palette: Palette) -> Element<'a, Message> {
    let mut rows: Column<'a, Message> = column![].spacing(TRAY_GAP);
    for pair in dock.tray.chunks(2).enumerate() {
        let (row_idx, items) = pair;
        let mut r: Row<'a, Message> = row![].spacing(TRAY_GAP);
        for (col_idx, item) in items.iter().enumerate() {
            let index = row_idx * 2 + col_idx;
            let hovered = bar.hover_tray == Some(index);
            let bg = if hovered {
                palette.hover
            } else {
                Color::TRANSPARENT
            };
            let icon = tray_icon_element(dock, item, TRAY_ICON);
            let slot = container(icon)
                .center_x(TRAY_ICON)
                .center_y(TRAY_ICON)
                .style(move |_| style::fill(bg, 4.0));
            let interactive = mouse_area(slot)
                .on_enter(Message::TrayHover(bar.id, Some(index)))
                .on_exit(Message::TrayHover(bar.id, None))
                .on_press(Message::TrayClick(item.address.clone()))
                .on_middle_press(Message::TrayMiddle(item.address.clone()))
                .on_right_press(Message::TrayMenu(bar.id, index));
            r = r.push(interactive);
        }
        rows = rows.push(r);
    }

    let tray_bg = style::alpha(palette.card, palette.card.a * 0.55);
    container(rows)
        .padding(TRAY_PAD)
        .style(move |_| style::outlined(tray_bg, palette.separator, 10.0))
        .into()
}

fn split_pill<'a>(dock: &'a Dock, bar: &'a BarState, palette: Palette) -> Element<'a, Message> {
    let open_kind = dock
        .spatial_popover
        .as_ref()
        .filter(|p| p.screen == bar.screen)
        .map(|p| p.kind);

    let notif_open = open_kind == Some(SpatialPopoverKind::Notifications);
    let ctrl_open = open_kind == Some(SpatialPopoverKind::Controls);

    let bell_color = if notif_open {
        palette.accent
    } else {
        palette.text
    };
    let bell_icon = icons::glyph(Glyph::Bell, 16.0, bell_color);
    let bell_content: Element<'a, Message> = if dock.spatial_visible_unread() > 0 {
        let dot = container(
            container(space().width(7).height(7)).style(|_| style::fill(style::DANGER, 3.5)),
        )
        .width(20)
        .height(20)
        .align_x(Horizontal::Right)
        .align_y(Vertical::Top);
        stack![container(bell_icon).center_x(20).center_y(20), dot].into()
    } else {
        bell_icon
    };

    let notif_zone = button(
        container(bell_content)
            .width(BTN_SIZE)
            .height(PILL_TOP_HEIGHT)
            .center_x(BTN_SIZE)
            .center_y(PILL_TOP_HEIGHT),
    )
    .width(BTN_SIZE)
    .padding(0)
    .on_press(Message::SpatialPopoverToggle(
        bar.screen,
        SpatialPopoverKind::Notifications,
    ))
    .style(pill_zone_style(
        notif_open,
        palette,
        Radius {
            top_left: PILL_RADIUS,
            top_right: PILL_RADIUS,
            bottom_right: 0.0,
            bottom_left: 0.0,
        },
    ));

    let wifi_color = if ctrl_open {
        palette.accent
    } else if dock.network.on {
        palette.text
    } else {
        palette.muted
    };
    let audio_glyph = if is_headphones(dock) {
        Glyph::Headphones
    } else {
        Glyph::Speakers
    };
    let audio_color = if dock.audio.muted {
        style::DANGER
    } else if ctrl_open {
        palette.accent
    } else {
        palette.text
    };

    let mut sys_icons = column![
        icons::glyph(Glyph::Wifi, 15.0, wifi_color),
        icons::glyph(audio_glyph, 15.0, audio_color),
    ]
    .spacing(9)
    .align_x(Alignment::Center);

    if let Some(battery) = dock.battery {
        let low = battery.charge == Charge::Discharging && battery.percent <= 10;
        let bat_glyph = if battery.charge == Charge::Charging {
            Glyph::BatteryCharging
        } else {
            Glyph::Battery
        };
        let bat_color = if low {
            style::DANGER
        } else if ctrl_open {
            palette.accent
        } else {
            palette.text
        };
        sys_icons = sys_icons.push(icons::glyph(bat_glyph, 15.0, bat_color));
    }

    let ctrl_zone = button(
        container(sys_icons)
            .width(BTN_SIZE)
            .padding(Padding::from([9.0, 0.0]))
            .center_x(BTN_SIZE),
    )
    .width(BTN_SIZE)
    .padding(0)
    .on_press(Message::SpatialPopoverToggle(
        bar.screen,
        SpatialPopoverKind::Controls,
    ))
    .style(pill_zone_style(
        ctrl_open,
        palette,
        Radius {
            top_left: 0.0,
            top_right: 0.0,
            bottom_right: PILL_RADIUS,
            bottom_left: PILL_RADIUS,
        },
    ));

    let pill_inner = column![notif_zone, separator_line(palette), ctrl_zone]
        .align_x(Alignment::Center)
        .width(BTN_SIZE);

    container(pill_inner)
        .width(BTN_SIZE)
        .style(move |_| style::outlined(palette.card, palette.rim, PILL_RADIUS))
        .into()
}

fn pill_zone_style(
    open: bool,
    palette: Palette,
    radius: Radius,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let bg = if open {
            style::alpha(palette.accent, 0.18)
        } else if hovered {
            palette.hover
        } else {
            Color::TRANSPARENT
        };
        button::Style {
            background: Some(Background::Color(bg)),
            border: Border {
                radius,
                ..Border::default()
            },
            ..button::Style::default()
        }
    }
}

fn is_headphones(dock: &Dock) -> bool {
    let out = dock.audio.output.to_lowercase();
    let name = dock.audio.output_name.to_lowercase();
    out.contains("casque")
        || out.contains("headphone")
        || out.contains("headset")
        || name.contains("headphone")
        || name.contains("headset")
}

fn hover_tooltip<'a>(
    bar: &'a BarState,
    side: Side,
    launcher_visible: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let Some(index) = bar.hover_entry else {
        return space()
            .width(TOOLTIP_REACH as f32)
            .height(Length::Fill)
            .into();
    };
    let Some(entry) = bar.entries.get(index) else {
        return space()
            .width(TOOLTIP_REACH as f32)
            .height(Length::Fill)
            .into();
    };

    let state_label = tooltip_state(entry, bar.screen, launcher_visible);
    let label = format!("{} · {state_label}", entry.identity.name);
    let pill = container(
        text(label)
            .size(11.5)
            .font(style::MEDIUM)
            .color(palette.text),
    )
    .padding(Padding::from([5.0, 10.0]))
    .style(move |_| container::Style {
        shadow: palette.shadow(),
        ..style::outlined(palette.glass_strong, palette.rim_bright, 9.0)
    });

    let (_, item_y, _, item_h) = entry_menu_rect(side, SURFACE_WIDTH, index, bar.pinned());
    let top_offset = (item_y as f32 + (item_h as f32 - 26.0) / 2.0).max(PAD_Y);

    let (align_x, pad) = match side {
        Side::Left => (
            Horizontal::Left,
            Padding::ZERO.top(top_offset).left(TOOLTIP_GAP),
        ),
        Side::Right => (
            Horizontal::Right,
            Padding::ZERO.top(top_offset).right(TOOLTIP_GAP),
        ),
    };

    container(pill)
        .width(TOOLTIP_REACH as f32)
        .height(Length::Fill)
        .padding(pad)
        .align_x(align_x)
        .align_y(Vertical::Top)
        .into()
}

fn tooltip_state(entry: &Entry, here: OutputRole, launcher_visible: bool) -> String {
    match spatial_indicator(entry, here, launcher_visible) {
        SpatialIndicator::None => "Épinglée".to_owned(),
        SpatialIndicator::ActiveHere | SpatialIndicator::Here => "Sur cet écran".to_owned(),
        SpatialIndicator::Elsewhere => {
            let screen = entry
                .windows
                .first()
                .map_or("un autre écran", |win| screen_name(win.screen));
            format!("Sur {screen} · clic pour téléporter")
        }
    }
}

fn screen_name(screen: OutputRole) -> &'static str {
    match screen {
        OutputRole::Left => "Écran gauche",
        OutputRole::Center => "Écran central",
        OutputRole::Right => "Écran droit",
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    #[test]
    fn input_region_covers_only_the_56_px_bar_on_each_side() {
        assert_eq!(
            input_region(Side::Left, SURFACE_WIDTH, 1440),
            (0, 0, 56, 1440)
        );
        assert_eq!(
            input_region(Side::Right, SURFACE_WIDTH, 1440),
            (240, 0, 56, 1440)
        );
    }

    #[test]
    fn entry_menu_rect_accounts_for_pinned_separator_and_side() {
        let first_left = entry_menu_rect(Side::Left, SURFACE_WIDTH, 0, 2);
        let second_left = entry_menu_rect(Side::Left, SURFACE_WIDTH, 1, 2);
        let first_unpinned = entry_menu_rect(Side::Left, SURFACE_WIDTH, 2, 2);
        let right = entry_menu_rect(Side::Right, SURFACE_WIDTH, 0, 2);

        assert_eq!(first_left.0, 9);
        assert_eq!(right.0, 240 + 9);
        assert_eq!(first_left.2, 38);
        assert_eq!(first_left.3, 38);
        assert_eq!(second_left.1 - first_left.1, 44);
        assert_eq!(first_unpinned.1 - second_left.1, 44 + 7);
    }

    #[test]
    fn tray_menu_rect_positions_2_column_grid_above_bottom_pill() {
        let slot0 = tray_menu_rect(Side::Left, SURFACE_WIDTH, 1080, 3, 0);
        let slot1 = tray_menu_rect(Side::Left, SURFACE_WIDTH, 1080, 3, 1);
        let slot2 = tray_menu_rect(Side::Left, SURFACE_WIDTH, 1080, 3, 2);

        assert_eq!(slot0.2, 16);
        assert_eq!(slot0.3, 16);
        assert_eq!(slot1.0 - slot0.0, 21);
        assert_eq!(slot1.1, slot0.1);
        assert_eq!(slot2.0, slot0.0);
        assert_eq!(slot2.1 - slot0.1, 21);
    }

    #[test]
    fn format_short_date_uses_uppercase_french_weekdays() {
        let local = |y, m, d| {
            NaiveDate::from_ymd_opt(y, m, d)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
                .and_local_timezone(Local)
                .unwrap()
        };

        assert_eq!(format_short_date(local(2026, 11, 23)), "LUN 23");
        assert_eq!(format_short_date(local(2026, 11, 24)), "MAR 24");
        assert_eq!(format_short_date(local(2026, 11, 25)), "MER 25");
        assert_eq!(format_short_date(local(2026, 11, 26)), "JEU 26");
        assert_eq!(format_short_date(local(2026, 11, 27)), "VEN 27");
        assert_eq!(format_short_date(local(2026, 11, 28)), "SAM 28");
        assert_eq!(format_short_date(local(2026, 11, 29)), "DIM 29");
    }
}
