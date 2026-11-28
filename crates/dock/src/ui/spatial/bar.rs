//! Permanent full-height Liquid Glass vertical sidebar for spatial mode.

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
use crate::ui::bar::{MAX_ICON_SIZE, MIN_ICON_SIZE, tray_icon_element};
use crate::ui::icons::{self, Glyph};
use crate::ui::style::{self, Palette};

/// Extra transparent width on the inner side of the surface so hover tooltips
/// render cleanly without unfolding the bar.
pub const TOOLTIP_REACH: u32 = 240;

const SEP_HEIGHT: f32 = 1.0;
const TOOLTIP_GAP: f32 = 10.0;

/// Layout metrics of the permanent spatial vertical sidebar, derived from the
/// configured taskbar icon size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub width: u32,
    pub exclusive_zone: i32,
    pub surface_width: u32,
    pub tile_icon: f32,
    pub tile_fallback: (f32, f32),
    pub btn_size: f32,
    pub btn_radius: f32,
    pub pad_y: f32,
    pub top_gap: f32,
    pub taskbar_gap: f32,
    pub bottom_gap: f32,
    pub clock_height: f32,
    pub clock_time_size: f32,
    pub clock_date_size: f32,
    pub sep_width: f32,
    pub launcher_glyph: f32,
    pub tray_icon: f32,
    pub tray_gap: f32,
    pub tray_pad: f32,
    pub pill_radius: f32,
    pub pill_top_height: f32,
    pub pill_base_height: f32,
    pub bell_size: f32,
    pub sys_icon: f32,
    pub sys_gap: f32,
    pub sys_pad_y: f32,
    pub unread_height: f32,
    pub unread_font: f32,
}

impl Metrics {
    /// Computes spatial sidebar metrics for the given taskbar `icon_size`.
    /// At `32 px`, this matches the original 56 px sidebar dimensions.
    pub fn new(icon_size: u32) -> Self {
        let tile_icon = icon_size.clamp(MIN_ICON_SIZE, MAX_ICON_SIZE) as f32;
        let fallback_side = (tile_icon * 0.78).round();
        let fallback_radius = (fallback_side * 0.24).round();
        let btn_size = tile_icon + 6.0;
        let width = (btn_size + 18.0).round() as u32;
        let scale = (width as f32 / 56.0).clamp(0.75, 1.5);
        let text_scale = (width as f32 / 56.0).clamp(0.82, 1.4);

        Self {
            width,
            exclusive_zone: width as i32,
            surface_width: width + TOOLTIP_REACH,
            tile_icon,
            tile_fallback: (fallback_side, fallback_radius),
            btn_size,
            btn_radius: (btn_size * (11.0 / 38.0)).round(),
            pad_y: (10.0 * scale).round(),
            top_gap: (10.0 * scale).round(),
            taskbar_gap: (6.0 * scale).round(),
            bottom_gap: (8.0 * scale).round(),
            clock_height: (28.0 * scale).round(),
            clock_time_size: 12.5 * text_scale,
            clock_date_size: 9.5 * text_scale,
            sep_width: (22.0 * scale).round(),
            launcher_glyph: (18.0 * scale).round(),
            tray_icon: (16.0 * scale).round(),
            tray_gap: (5.0 * scale).round(),
            tray_pad: (5.0 * scale).round(),
            pill_radius: (btn_size * (14.0 / 38.0)).round(),
            pill_top_height: (34.0 * scale).round(),
            pill_base_height: (92.0 * scale).round(),
            bell_size: (16.0 * scale).round(),
            sys_icon: (15.0 * scale).round(),
            sys_gap: (9.0 * scale).round(),
            sys_pad_y: (9.0 * scale).round(),
            unread_height: (16.0 * scale).round().max(13.0),
            unread_font: (9.5 * scale).max(8.0),
        }
    }

    fn bar_x(&self, side: Side, surface_width: u32) -> i32 {
        match side {
            Side::Left => 0,
            Side::Right => surface_width.saturating_sub(self.width) as i32,
        }
    }

    /// Interactive input region of the sidebar surface (strictly `width` px wide).
    pub fn input_region(
        &self,
        side: Side,
        surface_width: u32,
        height: u32,
    ) -> (i32, i32, i32, i32) {
        (
            self.bar_x(side, surface_width),
            0,
            self.width as i32,
            height as i32,
        )
    }

    /// Logical rectangle `(x, y, width, height)` of taskbar item `index` inside
    /// the sidebar surface, used to anchor right-click context menus.
    pub fn entry_menu_rect(
        &self,
        side: Side,
        surface_width: u32,
        index: usize,
        pinned_count: usize,
    ) -> (i32, i32, i32, i32) {
        let taskbar_top = (self.pad_y
            + self.btn_size
            + self.top_gap
            + self.clock_height
            + self.top_gap
            + SEP_HEIGHT
            + self.top_gap) as i32;
        let step = (self.btn_size + self.taskbar_gap) as i32;
        let sep_offset = if pinned_count > 0 && index >= pinned_count {
            (self.taskbar_gap + SEP_HEIGHT) as i32
        } else {
            0
        };
        let item_x =
            self.bar_x(side, surface_width) + ((self.width as i32 - self.btn_size as i32) / 2);
        let item_y = taskbar_top + (index as i32) * step + sep_offset;
        (item_x, item_y, self.btn_size as i32, self.btn_size as i32)
    }

    /// Logical rectangle `(x, y, width, height)` of tray item `index` inside the
    /// sidebar surface, used to anchor tray context menus.
    pub fn tray_menu_rect(
        &self,
        side: Side,
        surface_width: u32,
        height: u32,
        tray_len: usize,
        index: usize,
    ) -> (i32, i32, i32, i32) {
        let bx = self.bar_x(side, surface_width);
        let icon = self.tray_icon as i32;
        if tray_len == 0 {
            return (
                bx + ((self.width as i32 - self.btn_size as i32) / 2),
                (height as i32)
                    - (self.pad_y + self.pill_base_height + self.bottom_gap + self.tray_icon)
                        as i32,
                icon,
                icon,
            );
        }
        let rows = tray_len.div_ceil(2) as i32;
        let gap = self.tray_gap as i32;
        let pad = self.tray_pad as i32;
        let tray_w = pad * 2 + icon * 2 + gap + 2;
        let tray_h = pad * 2 + rows * icon + rows.saturating_sub(1) * gap + 2;
        let tray_x = bx + ((self.width as i32 - tray_w) / 2);
        let tray_y = (height as i32)
            - (self.pad_y + self.pill_base_height + self.bottom_gap) as i32
            - tray_h;
        let row = (index / 2) as i32;
        let col = (index % 2) as i32;
        let x = tray_x + 1 + pad + col * (icon + gap);
        let y = tray_y + 1 + pad + row * (icon + gap);
        (x, y, icon, icon)
    }
}

fn side_of(orientation: Orientation) -> Side {
    match orientation {
        Orientation::Vertical(side) => side,
        Orientation::Horizontal => Side::Left,
    }
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

/// Renders the permanent Liquid Glass vertical sidebar and its optional
/// hover tooltip inside the `TOOLTIP_REACH` area.
pub fn view<'a>(dock: &'a Dock, bar: &'a BarState, palette: Palette) -> Element<'a, Message> {
    let metrics = dock.spatial_metrics();
    let side = side_of(bar.orientation);
    let launcher_visible = dock.spatial_launcher_visible_on(bar.screen);

    let top_block = column![
        launcher_button(bar.screen, side, launcher_visible, metrics, palette),
        clock_block(dock.now, metrics, palette),
        separator_line(metrics, palette),
    ]
    .spacing(metrics.top_gap)
    .align_x(Alignment::Center)
    .width(Length::Fill);

    let taskbar_block = taskbar(dock, bar, side, launcher_visible, metrics, palette);
    let bottom_block = bottom_controls(dock, bar, metrics, palette);

    let bar_content = column![top_block, taskbar_block, bottom_block]
        .spacing(metrics.top_gap)
        .align_x(Alignment::Center)
        .padding(Padding::from([metrics.pad_y, 0.0]))
        .width(metrics.width as f32)
        .height(Length::Fill);

    let bar_bg = container(bar_content)
        .width(metrics.width as f32)
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
    .width(metrics.width as f32)
    .height(Length::Fill)
    .align_x(rim_align);

    let sidebar: Element<'a, Message> = stack![bar_bg, inner_rim].into();
    let tooltip_area = hover_tooltip(bar, side, metrics, palette);

    let layout = match side {
        Side::Left => row![sidebar, tooltip_area],
        Side::Right => row![tooltip_area, sidebar],
    };

    layout.width(Length::Fill).height(Length::Fill).into()
}

fn separator_line<'a>(metrics: Metrics, palette: Palette) -> Element<'a, Message> {
    container(space().width(metrics.sep_width).height(SEP_HEIGHT))
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
    metrics: Metrics,
) -> Element<'a, Message> {
    let centered_tile = container(tile)
        .width(metrics.width as f32)
        .height(metrics.btn_size)
        .center_x(metrics.width as f32)
        .center_y(metrics.btn_size);

    let Some(mark) = mark else {
        return centered_tile.into();
    };

    let (align, pad) = match side {
        Side::Left => (Horizontal::Left, Padding::ZERO.left(inset)),
        Side::Right => (Horizontal::Right, Padding::ZERO.right(inset)),
    };
    let positioned_mark = container(mark)
        .width(metrics.width as f32)
        .height(metrics.btn_size)
        .padding(pad)
        .align_x(align)
        .align_y(Vertical::Center);

    stack![centered_tile, positioned_mark].into()
}

fn launcher_button<'a>(
    screen: OutputRole,
    side: Side,
    active: bool,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let icon = icons::glyph(Glyph::Launcher, metrics.launcher_glyph, palette.text);
    let btn = button(
        container(icon)
            .center_x(metrics.btn_size)
            .center_y(metrics.btn_size),
    )
    .width(metrics.btn_size)
    .height(metrics.btn_size)
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
                radius: metrics.btn_radius.into(),
            }
        } else {
            Border {
                radius: metrics.btn_radius.into(),
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
    wrap_with_outer_mark(btn.into(), mark, side, 0.0, metrics)
}

fn clock_block<'a>(
    now: DateTime<Local>,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let hhmm = format!("{:02}:{:02}", now.hour(), now.minute());
    let date = format_short_date(now);

    let time_text = text(hhmm)
        .size(metrics.clock_time_size)
        .font(style::BOLD)
        .color(palette.text);
    let date_text = text(date)
        .size(metrics.clock_date_size)
        .font(style::SEMIBOLD)
        .color(palette.muted);

    container(
        column![time_text, date_text]
            .spacing(2)
            .align_x(Alignment::Center),
    )
    .height(metrics.clock_height)
    .center_x(metrics.width as f32)
    .center_y(metrics.clock_height)
    .into()
}

fn taskbar<'a>(
    dock: &'a Dock,
    bar: &'a BarState,
    side: Side,
    launcher_visible: bool,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let pinned_count = bar.pinned();
    let has_unpinned = bar.entries.len() > pinned_count;

    let mut items: Column<'a, Message> = column![]
        .spacing(metrics.taskbar_gap)
        .align_x(Alignment::Center);
    for (index, entry) in bar.entries.iter().enumerate() {
        if index == pinned_count && pinned_count > 0 && has_unpinned {
            items = items.push(separator_line(metrics, palette));
        }
        items = items.push(app_tile(
            dock,
            bar,
            index,
            entry,
            side,
            launcher_visible,
            metrics,
            palette,
        ));
    }

    scrollable(items.width(metrics.width as f32))
        .width(metrics.width as f32)
        .height(Length::Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(0).scroller_width(0),
        ))
        .into()
}

#[allow(clippy::too_many_arguments)]
fn app_tile<'a>(
    dock: &'a Dock,
    bar: &'a BarState,
    index: usize,
    entry: &'a Entry,
    side: Side,
    launcher_visible: bool,
    metrics: Metrics,
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
        metrics.tile_icon,
        metrics.tile_fallback,
        palette,
        hovered,
    );

    let tile_box = container(tile_icon)
        .center_x(metrics.btn_size)
        .center_y(metrics.btn_size)
        .style(move |_| {
            if indicator == SpatialIndicator::ActiveHere {
                style::outlined(bg, palette.rim, metrics.btn_radius)
            } else {
                style::fill(bg, metrics.btn_radius)
            }
        });

    let unread = dock.spatial_unread_for(entry);
    let tile_with_badge: Element<'a, Message> = if unread > 0 {
        let badge_el = container(unread_badge(unread, metrics))
            .width(metrics.btn_size)
            .height(metrics.btn_size)
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

    wrap_with_outer_mark(interactive.into(), mark, side, inset, metrics)
}

fn unread_badge<'a>(count: usize, metrics: Metrics) -> Element<'a, Message> {
    let label = if count > 99 {
        "99+".to_owned()
    } else {
        count.to_string()
    };
    let text_el = text(label)
        .size(metrics.unread_font)
        .font(style::BOLD)
        .color(Color::WHITE);
    container(text_el)
        .center_x(Length::Shrink)
        .center_y(metrics.unread_height)
        .padding(Padding::from([0.0, 4.0]))
        .style(move |_| style::fill(style::DANGER, metrics.unread_height / 2.0))
        .into()
}

fn bottom_controls<'a>(
    dock: &'a Dock,
    bar: &'a BarState,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let mut col: Column<'a, Message> = column![]
        .spacing(metrics.bottom_gap)
        .align_x(Alignment::Center);

    if !dock.tray.is_empty() {
        col = col.push(compact_tray(dock, bar, metrics, palette));
    }
    col = col.push(split_pill(dock, bar, metrics, palette));
    col.width(metrics.width as f32).into()
}

fn compact_tray<'a>(
    dock: &'a Dock,
    bar: &'a BarState,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let mut rows: Column<'a, Message> = column![].spacing(metrics.tray_gap);
    for pair in dock.tray.chunks(2).enumerate() {
        let (row_idx, items) = pair;
        let mut r: Row<'a, Message> = row![].spacing(metrics.tray_gap);
        for (col_idx, item) in items.iter().enumerate() {
            let index = row_idx * 2 + col_idx;
            let hovered = bar.hover_tray == Some(index);
            let bg = if hovered {
                palette.hover
            } else {
                Color::TRANSPARENT
            };
            let icon = tray_icon_element(dock, item, metrics.tray_icon);
            let slot = container(icon)
                .center_x(metrics.tray_icon)
                .center_y(metrics.tray_icon)
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
        .padding(metrics.tray_pad)
        .style(move |_| style::outlined(tray_bg, palette.separator, 10.0))
        .into()
}

fn split_pill<'a>(
    dock: &'a Dock,
    bar: &'a BarState,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
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
    let bell_icon = icons::glyph(Glyph::Bell, metrics.bell_size, bell_color);
    let bell_box = metrics.bell_size + 4.0;
    let bell_content: Element<'a, Message> = if dock.spatial_visible_unread() > 0 {
        let dot = container(
            container(space().width(7).height(7)).style(|_| style::fill(style::DANGER, 3.5)),
        )
        .width(bell_box)
        .height(bell_box)
        .align_x(Horizontal::Right)
        .align_y(Vertical::Top);
        stack![
            container(bell_icon).center_x(bell_box).center_y(bell_box),
            dot
        ]
        .into()
    } else {
        bell_icon
    };

    let notif_zone = button(
        container(bell_content)
            .width(metrics.btn_size)
            .height(metrics.pill_top_height)
            .center_x(metrics.btn_size)
            .center_y(metrics.pill_top_height),
    )
    .width(metrics.btn_size)
    .padding(0)
    .on_press(Message::SpatialPopoverToggle(
        bar.screen,
        SpatialPopoverKind::Notifications,
    ))
    .style(pill_zone_style(
        notif_open,
        palette,
        Radius {
            top_left: metrics.pill_radius,
            top_right: metrics.pill_radius,
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
        icons::glyph(Glyph::Wifi, metrics.sys_icon, wifi_color),
        icons::glyph(audio_glyph, metrics.sys_icon, audio_color),
    ]
    .spacing(metrics.sys_gap)
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
        sys_icons = sys_icons.push(icons::glyph(bat_glyph, metrics.sys_icon, bat_color));
    }

    let ctrl_zone = button(
        container(sys_icons)
            .width(metrics.btn_size)
            .padding(Padding::from([metrics.sys_pad_y, 0.0]))
            .center_x(metrics.btn_size),
    )
    .width(metrics.btn_size)
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
            bottom_right: metrics.pill_radius,
            bottom_left: metrics.pill_radius,
        },
    ));

    let pill_inner = column![notif_zone, separator_line(metrics, palette), ctrl_zone]
        .align_x(Alignment::Center)
        .width(metrics.btn_size);

    container(pill_inner)
        .width(metrics.btn_size)
        .style(move |_| style::outlined(palette.card, palette.rim, metrics.pill_radius))
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
    metrics: Metrics,
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

    let pill = container(
        text(&entry.identity.name)
            .size(11.5)
            .font(style::MEDIUM)
            .color(palette.text),
    )
    .padding(Padding::from([5.0, 10.0]))
    .style(move |_| container::Style {
        shadow: palette.shadow(),
        ..style::outlined(palette.glass_strong, palette.rim_bright, 9.0)
    });

    let (_, item_y, _, item_h) =
        metrics.entry_menu_rect(side, metrics.surface_width, index, bar.pinned());
    let top_offset = (item_y as f32 + (item_h as f32 - 26.0) / 2.0).max(metrics.pad_y);

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

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    #[test]
    fn input_region_covers_only_the_bar_on_each_side() {
        let m = Metrics::new(32);
        assert_eq!(m.width, 56);
        assert_eq!(
            m.input_region(Side::Left, m.surface_width, 1440),
            (0, 0, 56, 1440)
        );
        assert_eq!(
            m.input_region(Side::Right, m.surface_width, 1440),
            (240, 0, 56, 1440)
        );
    }

    #[test]
    fn metrics_scale_with_icon_size() {
        let small = Metrics::new(16);
        let default = Metrics::new(24);
        let medium = Metrics::new(32);
        let large = Metrics::new(64);

        assert_eq!(small.tile_icon, 16.0);
        assert_eq!(default.tile_icon, 24.0);
        assert_eq!(medium.tile_icon, 32.0);
        assert_eq!(large.tile_icon, 64.0);

        assert!(small.width < default.width);
        assert!(default.width < medium.width);
        assert!(medium.width < large.width);
        assert_eq!(medium.width, 56);
        assert_eq!(medium.exclusive_zone, 56);
        assert_eq!(small.exclusive_zone, small.width as i32);
        assert_eq!(large.exclusive_zone, large.width as i32);
    }

    #[test]
    fn entry_menu_rect_accounts_for_pinned_separator_and_side() {
        let m = Metrics::new(32);
        let first_left = m.entry_menu_rect(Side::Left, m.surface_width, 0, 2);
        let second_left = m.entry_menu_rect(Side::Left, m.surface_width, 1, 2);
        let first_unpinned = m.entry_menu_rect(Side::Left, m.surface_width, 2, 2);
        let right = m.entry_menu_rect(Side::Right, m.surface_width, 0, 2);

        assert_eq!(first_left.0, 9);
        assert_eq!(right.0, 240 + 9);
        assert_eq!(first_left.2, 38);
        assert_eq!(first_left.3, 38);
        assert_eq!(second_left.1 - first_left.1, 44);
        assert_eq!(first_unpinned.1 - second_left.1, 44 + 7);
    }

    #[test]
    fn tray_menu_rect_positions_2_column_grid_above_bottom_pill() {
        let m = Metrics::new(32);
        let slot0 = m.tray_menu_rect(Side::Left, m.surface_width, 1080, 3, 0);
        let slot1 = m.tray_menu_rect(Side::Left, m.surface_width, 1080, 3, 1);
        let slot2 = m.tray_menu_rect(Side::Left, m.surface_width, 1080, 3, 2);

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
