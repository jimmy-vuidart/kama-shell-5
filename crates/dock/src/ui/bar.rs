//! The bar: a thin column on the left or the right edge that unfolds under
//! the pointer, or a row on the bottom edge that names the icon under it.

use chrono::{DateTime, Datelike, Local, Timelike};
use iced::widget::{
    Column, Row, column, container, image, mouse_area, row, space, stack, svg, text,
};
use iced::{Alignment, Color, ContentFit, Element, Length, Padding};

use super::icons::{self, Glyph};
use super::style::{self, Palette};
use crate::app::{BarState, Dock, Message};
use crate::model::bar::{Entry, Indicator};
use crate::model::tray::TrayItem;
use crate::model::{Orientation, Side};

pub const DEFAULT_ICON_SIZE: u32 = 24;
pub const MIN_ICON_SIZE: u32 = 16;
pub const MAX_ICON_SIZE: u32 = 64;

/// Between the edge of the screen and the bar.
pub const MARGIN: f32 = 8.0;
/// Above and under the bar, for its shadow.
const SHADOW: f32 = 24.0;
const GAP: f32 = 4.0;
/// Inside the bar, above the launcher and under the clock.
const PADDING: f32 = 6.0;
/// A separation between two groups, or its title when unfolded.
const SECTION: f32 = 18.0;
const CLOCK: f32 = 54.0;
/// The room between an icon and the edge of the screen for its mark.
const MARK: f32 = 4.0;

/// Lying down: the room above the bar for the label of the icon under the
/// pointer, and on each side for a label that is wider than its icon.
const LABEL_ROOM: f32 = 58.0;
const SIDE: f32 = 110.0;
/// Lying down: a hairline between two groups, and the clock at the end.
const DIVIDER: f32 = 9.0;
const CLOCK_WIDTH: f32 = 62.0;

/// Layout metrics of the dock bar, derived from the configured icon size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub icon_size: f32,
    pub slot: f32,
    pub width: f32,
    pub exclusive_zone: i32,
    pub unfolded: f32,
    pub surface_width: u32,
    pub unfolded_reach: u32,
    pub radius: f32,
    pub tile_radius: f32,
    pub launcher_glyph: f32,
}

impl Metrics {
    pub fn new(icon_size: u32) -> Self {
        let icon = icon_size.clamp(MIN_ICON_SIZE, MAX_ICON_SIZE) as f32;
        let slot = icon + 12.0;
        let width = slot + 8.0;
        let exclusive_zone = (width + 2.0 * MARGIN).round() as i32;
        let unfolded = 200.0_f32.max(width + 100.0);
        let surface_width = (unfolded + MARGIN + SHADOW + 4.0).ceil() as u32;
        let unfolded_reach = (unfolded + MARGIN + 4.0).ceil() as u32;
        let radius = ((width - 16.0) / 2.0).clamp(10.0, 24.0);
        let tile_radius = (slot * 0.28).round();
        let launcher_glyph = (icon * (22.0 / 24.0)).round();

        Self {
            icon_size: icon,
            slot,
            width,
            exclusive_zone,
            unfolded,
            surface_width,
            unfolded_reach,
            radius,
            tile_radius,
            launcher_glyph,
        }
    }

    /// The height of the bar for a number of favorites and of other entries.
    pub fn bar_height(&self, pinned: usize, others: usize, tray_items: usize) -> f32 {
        let mut rows = vec![self.slot];
        if pinned > 0 {
            rows.push(SECTION);
            rows.extend(std::iter::repeat_n(self.slot, pinned));
        }
        if others > 0 {
            rows.push(SECTION);
            rows.extend(std::iter::repeat_n(self.slot, others));
        }
        if tray_items > 0 {
            let (_, tray_height) = crate::model::tray::vertical_size(tray_items);
            rows.push(tray_height);
        }
        rows.push(CLOCK);
        let gaps = GAP * (rows.len() - 1) as f32;
        rows.iter().sum::<f32>() + gaps + 2.0 * PADDING
    }

    /// The height of the surface that holds the bar.
    pub fn surface_height(&self, pinned: usize, others: usize, tray_items: usize) -> u32 {
        (self.bar_height(pinned, others, tray_items) + 2.0 * SHADOW).ceil() as u32
    }

    /// The width of a lying bar for a number of favorites and of other
    /// entries.
    pub fn lying_width(&self, pinned: usize, others: usize, tray_items: usize) -> f32 {
        let mut items = vec![self.slot];
        for group in [pinned, others] {
            if group > 0 {
                items.push(DIVIDER);
                items.extend(std::iter::repeat_n(self.slot, group));
            }
        }
        if tray_items > 0 {
            let (tray_width, _) = crate::model::tray::horizontal_size(tray_items);
            items.push(DIVIDER);
            items.push(tray_width);
        }
        items.extend([DIVIDER, CLOCK_WIDTH]);
        let gaps = GAP * (items.len() - 1) as f32;
        items.iter().sum::<f32>() + gaps + 2.0 * PADDING
    }

    /// How far from the left end of a lying bar the entry `index` starts.
    pub fn lying_left(&self, index: usize, pinned: usize) -> f32 {
        let dividers = if index >= pinned && pinned > 0 {
            2.0
        } else {
            1.0
        };
        PADDING + self.slot + GAP + dividers * (DIVIDER + GAP) + index as f32 * (self.slot + GAP)
    }

    /// How far from the left end of a lying bar the system tray starts.
    pub fn lying_tray_left(&self, pinned: usize, others: usize) -> f32 {
        let mut left = PADDING + self.slot + GAP;
        if pinned > 0 {
            left += DIVIDER + GAP + pinned as f32 * (self.slot + GAP);
        }
        if others > 0 {
            left += DIVIDER + GAP + others as f32 * (self.slot + GAP);
        }
        left += DIVIDER + GAP;
        left
    }

    /// The size of the surface of a bar, for a number of favorites and of
    /// other entries.
    pub fn surface_size(
        &self,
        orientation: Orientation,
        pinned: usize,
        others: usize,
        tray_items: usize,
    ) -> (u32, u32) {
        match orientation {
            Orientation::Vertical(_) => (
                self.surface_width,
                self.surface_height(pinned, others, tray_items),
            ),
            Orientation::Horizontal => {
                let width = self.lying_width(pinned, others, tray_items) + 2.0 * SIDE;
                (
                    width.ceil() as u32,
                    (LABEL_ROOM + self.width + MARGIN) as u32,
                )
            }
        }
    }

    /// The part of its surface where a bar takes the pointer (x, y, width,
    /// height): what shows, so that the windows around it stay clickable.
    pub fn input_region(
        &self,
        orientation: Orientation,
        size: (u32, u32),
        unfolded: bool,
    ) -> (i32, i32, i32, i32) {
        let (width, height) = (size.0 as i32, size.1 as i32);
        match orientation {
            Orientation::Vertical(side) => {
                let reach = if unfolded {
                    self.unfolded_reach as i32
                } else {
                    self.exclusive_zone
                };
                let x = match side {
                    Side::Left => 0,
                    Side::Right => width - reach,
                };
                (x, 0, reach, height)
            }
            Orientation::Horizontal => {
                let side = SIDE as i32;
                (
                    side,
                    LABEL_ROOM as i32,
                    width - 2 * side,
                    (self.width + MARGIN) as i32,
                )
            }
        }
    }

    /// Where the entry `index` is in the surface of its bar (x, y, width,
    /// height): what its menu opens next to. `unfolded` says an upright bar
    /// shows at its full width.
    pub fn entry_rect(
        &self,
        orientation: Orientation,
        index: usize,
        pinned: usize,
        unfolded: bool,
    ) -> (i32, i32, i32, i32) {
        match orientation {
            Orientation::Vertical(side) => self.upright_rect(side, index, pinned, unfolded),
            Orientation::Horizontal => (
                (SIDE + self.lying_left(index, pinned)) as i32,
                (LABEL_ROOM - 6.0) as i32,
                self.slot as i32,
                (self.width + 6.0) as i32,
            ),
        }
    }

    /// Where the tray item `index` is in the surface of its bar (x, y, width,
    /// height): what its menu opens next to.
    pub fn tray_rect(
        &self,
        orientation: Orientation,
        index: usize,
        pinned: usize,
        others: usize,
        tray_items: usize,
        unfolded: bool,
    ) -> (i32, i32, i32, i32) {
        match orientation {
            Orientation::Vertical(side) => {
                self.upright_tray_rect(side, index, pinned, others, tray_items, unfolded)
            }
            Orientation::Horizontal => {
                let (col, row) = crate::model::tray::horizontal_coord(index);
                let left = self.lying_tray_left(pinned, others)
                    + col as f32 * (crate::model::tray::ICON_SIZE + crate::model::tray::GAP);
                let top = LABEL_ROOM
                    + (self.width
                        - (2.0 * crate::model::tray::ICON_SIZE + crate::model::tray::GAP))
                        / 2.0
                    + row as f32 * (crate::model::tray::ICON_SIZE + crate::model::tray::GAP);
                (
                    (SIDE + left) as i32,
                    top as i32,
                    crate::model::tray::ICON_SIZE as i32,
                    crate::model::tray::ICON_SIZE as i32,
                )
            }
        }
    }

    fn upright_tray_rect(
        &self,
        side: Side,
        index: usize,
        pinned: usize,
        others: usize,
        tray_items: usize,
        _unfolded: bool,
    ) -> (i32, i32, i32, i32) {
        let (col, row) = crate::model::tray::vertical_coord(index);
        let bar_h = self.bar_height(pinned, others, tray_items);
        let (_, tray_h) = crate::model::tray::vertical_size(tray_items);
        let tray_bottom = SHADOW + bar_h - PADDING - CLOCK - GAP;
        let tray_top = tray_bottom - tray_h;
        let top = tray_top + row as f32 * (crate::model::tray::ICON_SIZE + crate::model::tray::GAP);
        let offset_x =
            (self.width - (2.0 * crate::model::tray::ICON_SIZE + crate::model::tray::GAP)) / 2.0;
        let x = match side {
            Side::Left => {
                MARGIN
                    + offset_x
                    + col as f32 * (crate::model::tray::ICON_SIZE + crate::model::tray::GAP)
            }
            Side::Right => {
                let tray_w = 2.0 * crate::model::tray::ICON_SIZE + crate::model::tray::GAP;
                self.surface_width as f32 - MARGIN - offset_x - tray_w
                    + col as f32 * (crate::model::tray::ICON_SIZE + crate::model::tray::GAP)
            }
        };
        (
            x as i32,
            top as i32,
            crate::model::tray::ICON_SIZE as i32,
            crate::model::tray::ICON_SIZE as i32,
        )
    }

    fn upright_rect(
        &self,
        side: Side,
        index: usize,
        pinned: usize,
        unfolded: bool,
    ) -> (i32, i32, i32, i32) {
        let mut top = SHADOW + PADDING + self.slot + GAP;
        top += index as f32 * (self.slot + GAP);
        let sections = if index >= pinned && pinned > 0 {
            2.0
        } else {
            1.0
        };
        top += sections * (SECTION + GAP);
        let width = if unfolded { self.unfolded } else { self.width };
        let x = match side {
            Side::Left => MARGIN,
            Side::Right => self.surface_width as f32 - MARGIN - width,
        };
        (x as i32, top as i32, width as i32, self.slot as i32)
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new(DEFAULT_ICON_SIZE)
    }
}

#[cfg(test)]
pub fn surface_height(pinned: usize, others: usize) -> u32 {
    Metrics::default().surface_height(pinned, others, 0)
}

#[cfg(test)]
pub fn surface_size(orientation: Orientation, pinned: usize, others: usize) -> (u32, u32) {
    Metrics::default().surface_size(orientation, pinned, others, 0)
}

#[cfg(test)]
pub fn input_region(
    orientation: Orientation,
    size: (u32, u32),
    unfolded: bool,
) -> (i32, i32, i32, i32) {
    Metrics::default().input_region(orientation, size, unfolded)
}

#[cfg(test)]
pub fn entry_rect(
    orientation: Orientation,
    index: usize,
    pinned: usize,
    unfolded: bool,
) -> (i32, i32, i32, i32) {
    Metrics::default().entry_rect(orientation, index, pinned, unfolded)
}

/// How an upright bar shows: the edge it stands on, whether its texts have
/// room, whether it reacts to the pointer.
#[derive(Clone, Copy)]
struct Look {
    side: Side,
    labels: bool,
    live: bool,
}

/// Where what follows the edge of the screen lines up in an upright bar.
fn outward(side: Side) -> Alignment {
    match side {
        Side::Left => Alignment::Start,
        Side::Right => Alignment::End,
    }
}

/// A line of an upright bar, its items given from the edge of the screen
/// inwards: the icons of a bar on the right keep to the right.
fn from_edge<'a>(side: Side, mut items: Vec<Element<'a, Message>>) -> Row<'a, Message> {
    if side == Side::Right {
        items.reverse();
    }
    Row::with_children(items).align_y(Alignment::Center)
}

/// Starts slowly, ends slowly.
fn ease(progress: f32) -> f32 {
    progress * progress * (3.0 - 2.0 * progress)
}

/// What the clock of the bar says: hours, minutes, the short date of the
/// folded bar and the long one of the unfolded bar.
pub fn clock<T: Datelike + Timelike>(now: &T) -> (String, String, String, String) {
    const DAYS: [&str; 7] = [
        "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
    ];
    const MONTHS: [&str; 12] = [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ];
    let day = DAYS[now.weekday().num_days_from_monday() as usize];
    let month = MONTHS[now.month0() as usize];
    let number = match now.day() {
        1 => "1er".to_owned(),
        other => other.to_string(),
    };
    let short: String = day.chars().take(3).collect();
    (
        format!("{:02}", now.hour()),
        format!("{:02}", now.minute()),
        format!("{short}. {}", now.day()),
        format!("{day} {number} {month}"),
    )
}

pub fn view<'a>(dock: &'a Dock, state: &'a BarState) -> Element<'a, Message> {
    let palette = dock.palette();
    let metrics = dock.metrics();
    match state.orientation {
        Orientation::Vertical(side) => bar(
            dock,
            state,
            side,
            ease(state.unfold),
            true,
            metrics,
            palette,
        ),
        Orientation::Horizontal => {
            let label: Element<'_, Message> = match hovered_entry(dock, state) {
                Some((index, entry)) => label(dock, state, index, entry, metrics, palette),
                None => match hovered_tray(dock, state) {
                    Some((index, item)) => tray_label(dock, state, index, item, metrics, palette),
                    None => space().into(),
                },
            };
            let lying =
                container(lying(dock, state, true, metrics, palette)).center_x(Length::Fill);
            column![
                container(label).height(LABEL_ROOM).align_y(Alignment::End),
                lying,
                space().height(MARGIN)
            ]
            .into()
        }
    }
}

/// A bar as the launcher shows it under itself: folded and still.
pub fn still<'a>(dock: &'a Dock, state: &'a BarState, palette: Palette) -> Element<'a, Message> {
    let metrics = dock.metrics();
    match state.orientation {
        Orientation::Vertical(side) => bar(dock, state, side, 0.0, false, metrics, palette),
        Orientation::Horizontal => {
            let lying =
                container(lying(dock, state, false, metrics, palette)).center_x(Length::Fill);
            column![space().height(MARGIN), lying, space().height(MARGIN)].into()
        }
    }
}

/// The entry under the pointer, unless its menu is open: the menu says
/// enough.
fn hovered_entry<'a>(dock: &Dock, state: &'a BarState) -> Option<(usize, &'a Entry)> {
    let has_menu = dock
        .menu
        .as_ref()
        .is_some_and(|(_, bar, _)| *bar == state.id);
    let index = state.hover_entry.filter(|_| !has_menu)?;
    Some((index, state.entries.get(index)?))
}

fn hovered_tray<'a>(dock: &'a Dock, state: &BarState) -> Option<(usize, &'a TrayItem)> {
    let has_menu = dock
        .tray_menu
        .as_ref()
        .is_some_and(|(_, bar, _)| *bar == state.id);
    let index = state.hover_tray.filter(|_| !has_menu)?;
    Some((index, dock.tray.get(index)?))
}

fn tray_label<'a>(
    _dock: &Dock,
    state: &BarState,
    index: usize,
    item: &'a TrayItem,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let name = text(&item.title)
        .size(13)
        .font(style::SEMIBOLD)
        .color(palette.text);
    let detail = item.tooltip.as_deref().unwrap_or(&item.title);
    let texts = column![name, text(detail).size(11).color(palette.muted)].spacing(1);
    let card = container(texts)
        .padding(Padding::from([6.0, 10.0]))
        .max_width(2.0 * SIDE - 20.0)
        .style(move |_| style::outlined(palette.glass_strong, palette.rim, 10.0));
    let (col, _) = crate::model::tray::horizontal_coord(index);
    let tray_start = metrics.lying_tray_left(state.pinned(), state.entries.len() - state.pinned());
    let left = (SIDE
        + tray_start
        + col as f32 * (crate::model::tray::ICON_SIZE + crate::model::tray::GAP)
        - 8.0)
        .max(0.0);
    container(card)
        .padding(Padding {
            left,
            bottom: 6.0,
            top: 0.0,
            right: 0.0,
        })
        .into()
}

/// The name and the state of the entry under the pointer, above its icon.
fn label<'a>(
    dock: &Dock,
    state: &BarState,
    index: usize,
    entry: &'a Entry,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let unread = dock.unread_of(entry);
    let (detail, color) = if unread > 0 {
        (
            format!("{} · {unread} non lus", entry.subtitle),
            palette.accent,
        )
    } else {
        (entry.subtitle.clone(), palette.muted)
    };
    let name = text(&entry.identity.name)
        .size(13)
        .font(style::SEMIBOLD)
        .color(palette.text);
    let texts = column![name, text(detail).size(11).color(color)].spacing(1);
    let card = container(texts)
        .padding(Padding::from([6.0, 10.0]))
        .max_width(2.0 * SIDE - 20.0)
        .style(move |_| style::outlined(palette.glass_strong, palette.rim, 10.0));
    let left = (SIDE + metrics.lying_left(index, state.pinned()) - 8.0).max(0.0);
    container(card)
        .padding(Padding {
            left,
            bottom: 6.0,
            top: 0.0,
            right: 0.0,
        })
        .into()
}

/// The bar lying on the bottom edge: launcher, favorites, other open
/// applications, clock. `live` says whether it reacts to the pointer.
fn lying<'a>(
    dock: &'a Dock,
    state: &'a BarState,
    live: bool,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let pinned = state.pinned();
    let divider = || {
        let line = container(space().width(1).height(22))
            .style(move |_| style::fill(palette.separator, 0.0));
        container(line).center_x(DIVIDER).center_y(metrics.width)
    };

    let mut items: Row<'_, Message> =
        row![lying_launcher(dock, state, live, metrics, palette)].spacing(GAP);
    for (index, entry) in state.entries.iter().enumerate() {
        if index == 0 || index == pinned {
            items = items.push(divider());
        }
        items = items.push(lying_entry(
            dock, state, index, entry, live, metrics, palette,
        ));
    }
    if !dock.tray.is_empty() {
        items = items
            .push(divider())
            .push(lying_tray(dock, state, live, metrics, palette));
    }
    items = items
        .push(divider())
        .push(lying_time(&dock.now, metrics, palette));

    let body = container(items)
        .height(metrics.width)
        .padding(Padding::from([0.0, PADDING]))
        .style(move |_| container::Style {
            shadow: palette.shadow(),
            ..style::outlined(palette.glass, palette.rim, metrics.radius)
        });
    if !live {
        return body.into();
    }
    mouse_area(body)
        .on_enter(Message::BarHover(state.id, true))
        .on_exit(Message::BarHover(state.id, false))
        .into()
}

fn lying_launcher<'a>(
    dock: &Dock,
    state: &BarState,
    live: bool,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let open = dock.drawer.is_some();
    let (color, background) = if open {
        (palette.on_accent(), palette.accent)
    } else {
        (palette.accent, iced::Color::TRANSPARENT)
    };
    let icon = container(icons::glyph(Glyph::Launcher, metrics.launcher_glyph, color))
        .center_x(metrics.slot)
        .center_y(metrics.slot)
        .style(move |_| style::fill(background, metrics.tile_radius));
    let area = mouse_area(container(icon).center_y(metrics.width));
    if live || open {
        area.on_press(Message::Launcher(Some(state.screen))).into()
    } else {
        area.into()
    }
}

/// An icon of a lying bar, with its mark under it.
fn lying_entry<'a>(
    dock: &'a Dock,
    state: &BarState,
    index: usize,
    entry: &'a Entry,
    live: bool,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let hovered = live && state.hover_entry == Some(index);
    let dragged = dock.drag.is_some_and(|drag| drag.at == index && drag.moved);
    let unread = dock.unread_of(entry);

    let mark: Element<'_, Message> = match entry.indicator {
        Indicator::Here => container(space().width(12).height(3))
            .style(move |_| style::fill(palette.accent, 1.5))
            .into(),
        Indicator::Elsewhere => container(space().width(4).height(3))
            .style(move |_| style::fill(palette.muted, 1.5))
            .into(),
        Indicator::None => space().height(3).into(),
    };
    let tile = if hovered || dragged {
        palette.hover
    } else {
        iced::Color::TRANSPARENT
    };
    let icon = container(icons::app(&dock.icons, &entry.identity, metrics.icon_size))
        .center_x(metrics.slot)
        .center_y(metrics.slot)
        .style(move |_| style::fill(tile, metrics.tile_radius));
    let icon: Element<'_, Message> = if unread > 0 {
        let count = container(badge(unread, 14.0))
            .width(metrics.slot)
            .height(metrics.slot)
            .align_x(Alignment::End)
            .align_y(Alignment::Start);
        stack![icon, count].into()
    } else {
        icon.into()
    };
    let mark_height = (metrics.width - metrics.slot - 2.0).max(0.0);
    let slot = column![
        space().height(2),
        icon,
        container(mark).center_x(metrics.slot).height(mark_height)
    ];
    if !live {
        return slot.into();
    }
    mouse_area(slot)
        .on_enter(Message::EntryHover(state.id, Some(index)))
        .on_exit(Message::EntryHover(state.id, None))
        .on_press(Message::EntryPress(state.id, index))
        .on_release(Message::EntryRelease(state.id, index))
        .on_middle_press(Message::EntryMiddle(state.id, index))
        .on_right_press(Message::EntryMenu(state.id, index))
        .into()
}

fn lying_time<'a>(
    now: &DateTime<Local>,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let (hours, minutes, short, _) = clock(now);
    let time = text(format!("{hours}:{minutes}"))
        .size(14)
        .font(style::BOLD)
        .color(palette.text);
    let date = text(short).size(10).color(palette.muted);
    container(column![time, date].align_x(Alignment::End))
        .width(CLOCK_WIDTH)
        .padding(Padding::from([0.0, 6.0]))
        .align_x(Alignment::End)
        .center_y(metrics.width)
        .into()
}

pub fn tray_icon_element<'a, Message: 'a>(
    dock: &'a Dock,
    item: &'a TrayItem,
    size: f32,
) -> Element<'a, Message> {
    if let Some(pixmap) = &item.pixmap {
        return image(image::Handle::from_rgba(
            pixmap.width,
            pixmap.height,
            pixmap.pixels.clone(),
        ))
        .width(size)
        .height(size)
        .content_fit(ContentFit::Contain)
        .into();
    }
    if let Some(icon_name) = &item.icon_name {
        if let Some(icon) = dock.icons.get(icon_name) {
            return match icon {
                icons::Icon::Vector(handle) => svg(handle.clone()).width(size).height(size).into(),
                icons::Icon::Raster(handle) => image(handle.clone())
                    .width(size)
                    .height(size)
                    .content_fit(ContentFit::Contain)
                    .into(),
            };
        }
        let theme = coquille_apps::icon_theme();
        if let Some(path) = coquille_apps::find_icon(icon_name, &theme) {
            let is_svg = path.extension().is_some_and(|ext| ext == "svg");
            if is_svg {
                return svg(svg::Handle::from_path(path))
                    .width(size)
                    .height(size)
                    .into();
            } else {
                return image(image::Handle::from_path(path))
                    .width(size)
                    .height(size)
                    .content_fit(ContentFit::Contain)
                    .into();
            }
        }
    }
    let letter = item
        .title
        .chars()
        .next()
        .or_else(|| item.id.chars().next())
        .map_or('?', |c| c.to_ascii_uppercase());
    let label = text(letter.to_string())
        .size(size * 0.5)
        .font(style::BOLD)
        .color(Color::WHITE);
    let color = icons::tile_color(&item.id);
    container(label)
        .center_x(Length::Fixed(size))
        .center_y(Length::Fixed(size))
        .style(move |_| style::fill(color, size * 0.26))
        .into()
}

fn render_tray_slot<'a>(
    dock: &'a Dock,
    state: &BarState,
    index: usize,
    item: &'a TrayItem,
    live: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let hovered = state.hover_tray == Some(index);
    let tile_bg = if hovered {
        palette.hover
    } else {
        Color::TRANSPARENT
    };
    let icon = tray_icon_element(dock, item, crate::model::tray::ICON_SIZE);
    let slot = container(icon)
        .center_x(crate::model::tray::ICON_SIZE)
        .center_y(crate::model::tray::ICON_SIZE)
        .style(move |_| style::fill(tile_bg, 4.0));
    if !live {
        return slot.into();
    }
    mouse_area(slot)
        .on_enter(Message::TrayHover(state.id, Some(index)))
        .on_exit(Message::TrayHover(state.id, None))
        .on_press(Message::TrayClick(item.address.clone()))
        .on_middle_press(Message::TrayMiddle(item.address.clone()))
        .on_right_press(Message::TrayMenu(state.id, index))
        .into()
}

fn lying_tray<'a>(
    dock: &'a Dock,
    state: &'a BarState,
    live: bool,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let count = dock.tray.len();
    let num_cols = crate::model::tray::horizontal_columns(count);
    let mut columns: Row<'_, Message> = row![].spacing(crate::model::tray::GAP);

    for c in 0..num_cols {
        let mut col_slots: Column<'_, Message> = column![].spacing(crate::model::tray::GAP);
        let idx1 = c * 2;
        let idx2 = c * 2 + 1;
        if let Some(item) = dock.tray.get(idx1) {
            col_slots = col_slots.push(render_tray_slot(dock, state, idx1, item, live, palette));
        }
        if let Some(item) = dock.tray.get(idx2) {
            col_slots = col_slots.push(render_tray_slot(dock, state, idx2, item, live, palette));
        }
        columns = columns.push(col_slots);
    }

    container(columns).center_y(metrics.width).into()
}

fn upright_tray<'a>(
    dock: &'a Dock,
    state: &'a BarState,
    look: Look,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let Look { side, live, .. } = look;
    let count = dock.tray.len();
    let num_rows = crate::model::tray::vertical_rows(count);

    let mut col_items: Column<'_, Message> = column![].spacing(crate::model::tray::GAP);
    for r in 0..num_rows {
        let mut row_items: Row<'_, Message> = row![].spacing(crate::model::tray::GAP);
        let idx1 = r * 2;
        let idx2 = r * 2 + 1;
        if let Some(item) = dock.tray.get(idx1) {
            row_items = row_items.push(render_tray_slot(dock, state, idx1, item, live, palette));
        }
        if let Some(item) = dock.tray.get(idx2) {
            row_items = row_items.push(render_tray_slot(dock, state, idx2, item, live, palette));
        }
        col_items = col_items.push(row_items);
    }

    let offset_x =
        (metrics.width - (2.0 * crate::model::tray::ICON_SIZE + crate::model::tray::GAP)) / 2.0;
    let content = from_edge(side, vec![space().width(offset_x).into(), col_items.into()]);
    container(content)
        .width(Length::Fill)
        .align_x(outward(side))
        .into()
}

/// The bar standing on the left or the right edge. The drawer shows it too,
/// folded and still: `live` says whether it reacts to the pointer.
fn bar<'a>(
    dock: &'a Dock,
    state: &'a BarState,
    side: Side,
    unfold: f32,
    live: bool,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let width = metrics.width + (metrics.unfolded - metrics.width) * unfold;
    // Texts come once there is room for them.
    let labels = unfold > 0.4;
    let pinned = state.pinned();
    let others = state.entries.len() - pinned;
    let look = Look { side, labels, live };

    let mut rows: Column<'_, Message> =
        column![launcher(dock, state, look, metrics, palette)].spacing(GAP);
    for (index, entry) in state.entries.iter().enumerate() {
        if index == 0 && pinned > 0 {
            rows = rows.push(section("FAVORIS", side, labels, palette));
        }
        if index == pinned {
            rows = rows.push(section("OUVERTES", side, labels, palette));
        }
        rows = rows.push(entry_row(dock, state, index, entry, look, metrics, palette));
    }
    rows = rows.push(space().height(Length::Fill));
    if !dock.tray.is_empty() {
        rows = rows.push(upright_tray(dock, state, look, metrics, palette));
        rows = rows.push(space().height(GAP));
    }
    rows = rows.push(time(&dock.now, side, labels, palette));

    let body = container(rows)
        .width(width)
        .height(metrics.bar_height(pinned, others, dock.tray.len()))
        .padding(Padding::from([PADDING, 0.0]))
        .clip(true)
        .style(move |_| container::Style {
            shadow: palette.shadow(),
            ..style::outlined(palette.glass, palette.rim, metrics.radius)
        });
    let body: Element<'_, Message> = if live {
        mouse_area(body)
            .on_enter(Message::BarHover(state.id, true))
            .on_exit(Message::BarHover(state.id, false))
            .into()
    } else {
        body.into()
    };
    // It unfolds away from its edge.
    let (left, right) = match side {
        Side::Left => (MARGIN, 0.0),
        Side::Right => (0.0, MARGIN),
    };
    container(body)
        .width(Length::Fill)
        .align_x(outward(side))
        .padding(Padding {
            left,
            top: SHADOW,
            bottom: SHADOW,
            right,
        })
        .into()
}

/// The button of the drawer, at the top of the bar.
fn launcher<'a>(
    dock: &Dock,
    state: &BarState,
    look: Look,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let Look { side, labels, live } = look;
    let open = dock.drawer.is_some();
    let color = if open {
        palette.on_accent()
    } else {
        palette.accent
    };
    let background = if open {
        palette.accent
    } else {
        iced::Color::TRANSPARENT
    };
    let icon = container(icons::glyph(Glyph::Launcher, metrics.launcher_glyph, color))
        .center_x(metrics.slot)
        .center_y(metrics.slot)
        .style(move |_| style::fill(background, metrics.tile_radius));
    let mut content: Vec<Element<'_, Message>> = vec![space().width(MARK).into(), icon.into()];
    if labels {
        let label = text("Lanceur")
            .size(14)
            .font(style::SEMIBOLD)
            .color(palette.text);
        content.extend([space().width(8).into(), label.into()]);
    }
    let content = container(from_edge(side, content))
        .height(metrics.slot)
        .width(Length::Fill)
        .align_x(outward(side));
    let area = mouse_area(content);
    if live || open {
        area.on_press(Message::Launcher(Some(state.screen))).into()
    } else {
        area.into()
    }
}

/// A hairline between two groups; unfolded, the name of the group.
fn section(
    title: &'static str,
    side: Side,
    labels: bool,
    palette: Palette,
) -> Element<'static, Message> {
    if labels {
        let title = text(title)
            .size(10)
            .font(style::SEMIBOLD)
            .color(palette.muted);
        return container(title)
            .height(SECTION)
            .width(Length::Fill)
            .padding(Padding::from([0.0, 14.0]))
            .align_x(outward(side))
            .align_y(Alignment::End)
            .into();
    }
    let line = container(space().width(Length::Fill).height(1))
        .style(move |_| style::fill(palette.separator, 0.0));
    container(line)
        .height(SECTION)
        .padding(Padding::from([0.0, 9.0]))
        .center_y(SECTION)
        .into()
}

fn entry_row<'a>(
    dock: &'a Dock,
    state: &BarState,
    index: usize,
    entry: &'a Entry,
    look: Look,
    metrics: Metrics,
    palette: Palette,
) -> Element<'a, Message> {
    let Look { side, labels, live } = look;
    let hovered = live && state.hover_entry == Some(index);
    let dragged = dock.drag.is_some_and(|drag| drag.at == index && drag.moved);
    let unread = dock.unread_of(entry);

    let mark: Element<'_, Message> = match entry.indicator {
        Indicator::Here => container(space().width(3).height(12))
            .style(move |_| style::fill(palette.accent, 1.5))
            .into(),
        Indicator::Elsewhere => container(space().width(3).height(4))
            .style(move |_| style::fill(palette.muted, 1.5))
            .into(),
        Indicator::None => space().width(3).into(),
    };
    // Against its icon.
    let mark = container(mark)
        .width(MARK)
        .center_y(metrics.slot)
        .align_x(match side {
            Side::Left => Alignment::End,
            Side::Right => Alignment::Start,
        });

    let tile = if hovered && !labels || dragged {
        palette.hover
    } else {
        iced::Color::TRANSPARENT
    };
    let icon = container(icons::app(&dock.icons, &entry.identity, metrics.icon_size))
        .center_x(metrics.slot)
        .center_y(metrics.slot)
        .style(move |_| style::fill(tile, metrics.tile_radius));
    // Folded, the count of unread messages sits on the corner of the icon.
    let icon: Element<'_, Message> = if unread > 0 && !labels {
        let count = container(badge(unread, 14.0))
            .width(metrics.slot)
            .height(metrics.slot)
            .align_x(Alignment::End)
            .align_y(Alignment::Start);
        stack![icon, count].into()
    } else {
        icon.into()
    };

    let mut content: Vec<Element<'_, Message>> = vec![mark.into(), icon];
    if labels {
        let name = text(&entry.identity.name)
            .size(14)
            .font(style::SEMIBOLD)
            .color(palette.text);
        let (detail, color) = if unread > 0 {
            (
                format!("{} · {unread} non lus", entry.subtitle),
                palette.accent,
            )
        } else {
            (entry.subtitle.clone(), palette.muted)
        };
        let detail = text(detail).size(11).color(color);
        let texts = column![name, detail]
            .spacing(1)
            .width(Length::Fill)
            .align_x(outward(side))
            .clip(true);
        content.extend([space().width(8).into(), texts.into()]);
        if unread > 0 {
            content.extend([badge(unread, 18.0), space().width(10).into()]);
        }
    }
    let background = if hovered && labels {
        palette.hover
    } else {
        iced::Color::TRANSPARENT
    };
    let row = container(from_edge(side, content))
        .height(metrics.slot)
        .width(Length::Fill)
        .align_x(outward(side))
        .style(move |_| style::fill(background, metrics.tile_radius));
    let row = container(row).padding(Padding::from([0.0, if labels { 4.0 } else { 0.0 }]));
    if !live {
        return row.into();
    }
    mouse_area(row)
        .on_enter(Message::EntryHover(state.id, Some(index)))
        .on_exit(Message::EntryHover(state.id, None))
        .on_press(Message::EntryPress(state.id, index))
        .on_release(Message::EntryRelease(state.id, index))
        .on_middle_press(Message::EntryMiddle(state.id, index))
        .on_right_press(Message::EntryMenu(state.id, index))
        .into()
}

/// The red disc of unread messages.
fn badge<'a>(count: u32, size: f32) -> Element<'a, Message> {
    let label = if count > 99 {
        "99+".to_owned()
    } else {
        count.to_string()
    };
    let label = text(label)
        .size(size * 0.62)
        .font(style::BOLD)
        .color(iced::Color::WHITE);
    container(label)
        .center_x(Length::Shrink)
        .center_y(size)
        .padding(Padding::from([0.0, size * 0.3]))
        .style(move |_| style::fill(style::DANGER, size / 2.0))
        .into()
}

fn time<'a>(
    now: &DateTime<Local>,
    side: Side,
    labels: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let (hours, minutes, short, long) = clock(now);
    if labels {
        let time = text(format!("{hours}:{minutes}"))
            .size(20)
            .font(style::BOLD)
            .color(palette.text);
        let date = text(long).size(12).color(palette.muted);
        let line = from_edge(side, vec![time.into(), date.into()]).spacing(10);
        return container(line)
            .height(CLOCK)
            .width(Length::Fill)
            .padding(Padding::from([0.0, 14.0]))
            .align_x(outward(side))
            .center_y(CLOCK)
            .into();
    }
    let digits = |value: String| text(value).size(14).font(style::BOLD).color(palette.text);
    let date = text(short).size(10).color(palette.muted);
    let stacked = column![digits(hours), digits(minutes), space().height(2), date]
        .align_x(Alignment::Center)
        .spacing(0);
    container(stacked)
        .height(CLOCK)
        .center_x(Length::Fill)
        .center_y(CLOCK)
        .into()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    const WIDTH: f32 = 44.0;
    const SLOT: f32 = 36.0;
    const SURFACE_WIDTH: u32 = 236;
    const UNFOLDED_REACH: u32 = 212;

    #[test]
    fn the_clock_speaks_french() {
        let evening = NaiveDate::from_ymd_opt(2026, 10, 11)
            .unwrap()
            .and_hms_opt(21, 14, 0)
            .unwrap();

        let (hours, minutes, short, long) = clock(&evening);

        assert_eq!((hours.as_str(), minutes.as_str()), ("21", "14"));
        assert_eq!(short, "dim. 11");
        assert_eq!(long, "dimanche 11 octobre");
    }

    #[test]
    fn hours_and_minutes_keep_two_digits_and_the_first_is_the_premier() {
        let morning = NaiveDate::from_ymd_opt(2026, 8, 1)
            .unwrap()
            .and_hms_opt(7, 5, 0)
            .unwrap();

        let (hours, minutes, short, long) = clock(&morning);

        assert_eq!((hours.as_str(), minutes.as_str()), ("07", "05"));
        assert_eq!(short, "sam. 1");
        assert_eq!(long, "samedi 1er août");
    }

    #[test]
    fn the_bar_grows_with_its_entries() {
        assert_eq!(
            surface_height(2, 0) - surface_height(1, 0),
            (SLOT + GAP) as u32
        );
        // The first entry of a group brings the title of the group along.
        let with_title = (SLOT + GAP + SECTION + GAP) as u32;
        assert_eq!(surface_height(1, 0) - surface_height(0, 0), with_title);
        assert_eq!(surface_height(1, 1) - surface_height(1, 0), with_title);
        assert_eq!(surface_height(0, 1) - surface_height(0, 0), with_title);
    }

    #[test]
    fn the_menu_opens_next_to_its_entry() {
        let left = Orientation::Vertical(Side::Left);
        let upright = |index, pinned, unfolded| entry_rect(left, index, pinned, unfolded);
        let (x, first, width, height) = upright(0, 3, false);
        let (_, second, ..) = upright(1, 3, false);
        let (_, other, ..) = upright(3, 3, false);
        let (.., unfolded, _) = upright(0, 3, true);

        assert_eq!((x, width, height), (8, 44, 36));
        assert_eq!(unfolded, 200);
        assert_eq!(second - first, 40);
        // Past the favorites, the title of the second group is in between.
        assert_eq!(other - first, 3 * 40 + 22);
        // Without favorites, the first open application takes their place.
        assert_eq!(upright(0, 0, false).1, first);
    }

    #[test]
    fn on_the_right_edge_the_entries_keep_to_the_right_of_the_surface() {
        let (left, right) = (
            Orientation::Vertical(Side::Left),
            Orientation::Vertical(Side::Right),
        );
        let (x, y, width, height) = entry_rect(right, 1, 3, false);
        let (unfolded, ..) = entry_rect(right, 1, 3, true);

        // As far from its edge as on the left, and at the same height.
        assert_eq!(x + width, SURFACE_WIDTH as i32 - 8);
        assert_eq!((y, width, height), {
            let (_, y, width, height) = entry_rect(left, 1, 3, false);
            (y, width, height)
        });
        // Unfolded, the bar grew to the left.
        assert_eq!(unfolded, SURFACE_WIDTH as i32 - 8 - 200);
        assert_eq!(surface_size(right, 3, 2), surface_size(left, 3, 2));
    }

    #[test]
    fn a_lying_bar_widens_with_its_entries() {
        let width = |pinned, others| surface_size(Orientation::Horizontal, pinned, others).0;

        assert_eq!(width(2, 0) - width(1, 0), (SLOT + GAP) as u32);
        // The first entry of a group brings its hairline along.
        let with_divider = (SLOT + GAP + DIVIDER + GAP) as u32;
        assert_eq!(width(1, 0) - width(0, 0), with_divider);
        assert_eq!(width(1, 1) - width(1, 0), with_divider);
    }

    #[test]
    fn a_lying_bar_is_as_high_as_the_bar_and_the_label_above_it() {
        let (_, height) = surface_size(Orientation::Horizontal, 3, 2);

        assert_eq!(height, (LABEL_ROOM + WIDTH + MARGIN) as u32);
        assert_eq!(
            surface_size(Orientation::Vertical(Side::Left), 3, 2).0,
            SURFACE_WIDTH
        );
    }

    #[test]
    fn the_icons_of_a_lying_bar_follow_each_other_from_the_left() {
        let lying = |index, pinned| entry_rect(Orientation::Horizontal, index, pinned, false);
        let (first, y, width, _) = lying(0, 3);
        let (second, ..) = lying(1, 3);
        let (other, ..) = lying(3, 3);

        // Past the room for labels, the launcher and a hairline.
        assert_eq!(first, (SIDE + PADDING + SLOT + GAP + DIVIDER + GAP) as i32);
        assert_eq!(second - first, 40);
        assert_eq!(other - first, 3 * 40 + 13);
        assert_eq!(lying(0, 0).0, first);
        assert_eq!(width, 36);
        // The menu opens just above the bar.
        assert!(y < LABEL_ROOM as i32);
    }

    #[test]
    fn a_bar_takes_the_pointer_only_where_it_shows() {
        let (left, right) = (
            Orientation::Vertical(Side::Left),
            Orientation::Vertical(Side::Right),
        );
        let upright = surface_size(left, 2, 0);
        let lying = surface_size(Orientation::Horizontal, 2, 0);

        assert_eq!(
            input_region(left, upright, false),
            (0, 0, 60, upright.1 as i32)
        );
        assert_eq!(input_region(left, upright, true).2, UNFOLDED_REACH as i32);
        // On the right edge, what shows is at the right end of the surface.
        let width = SURFACE_WIDTH as i32;
        assert_eq!(
            input_region(right, upright, false),
            (width - 60, 0, 60, upright.1 as i32)
        );
        let reach = UNFOLDED_REACH as i32;
        assert_eq!(
            input_region(right, upright, true),
            (width - reach, 0, reach, upright.1 as i32)
        );
        // Lying down: the bar itself, not the room kept for labels.
        let (x, y, width, height) = input_region(Orientation::Horizontal, lying, false);
        assert_eq!((x, y), (SIDE as i32, LABEL_ROOM as i32));
        assert_eq!(width, lying.0 as i32 - 2 * SIDE as i32);
        assert_eq!(height, (WIDTH + MARGIN) as i32);
    }

    #[test]
    fn the_unfolding_starts_and_ends_gently() {
        assert!(ease(0.0).abs() < f32::EPSILON);
        assert!((ease(1.0) - 1.0).abs() < f32::EPSILON);
        assert!(ease(0.1) < 0.1 && ease(0.9) > 0.9);
    }

    #[test]
    fn the_metrics_scale_with_icon_size() {
        let default = Metrics::default();
        assert_eq!(default.icon_size, 24.0);
        assert_eq!(default.slot, 36.0);
        assert_eq!(default.width, 44.0);
        assert_eq!(default.exclusive_zone, 60);
        assert_eq!(default.surface_width, 236);
        assert_eq!(default.unfolded_reach, 212);

        let large = Metrics::new(32);
        assert_eq!(large.icon_size, 32.0);
        assert_eq!(large.slot, 44.0);
        assert_eq!(large.width, 52.0);
        assert_eq!(large.exclusive_zone, 68);

        let small = Metrics::new(16);
        assert_eq!(small.icon_size, 16.0);
        assert_eq!(small.slot, 28.0);
        assert_eq!(small.width, 36.0);
        assert_eq!(small.exclusive_zone, 52);

        // Clamping bounds
        assert_eq!(Metrics::new(5).icon_size, MIN_ICON_SIZE as f32);
        assert_eq!(Metrics::new(128).icon_size, MAX_ICON_SIZE as f32);
    }

    #[test]
    fn the_bar_grows_with_tray_items() {
        let metrics = Metrics::default();
        let without_tray = metrics.bar_height(1, 0, 0);
        let with_two = metrics.bar_height(1, 0, 2);
        let with_four = metrics.bar_height(1, 0, 4);

        // 2 items in vertical dock = 1 row (height 16) + bar GAP (4)
        assert_eq!(with_two - without_tray, 20.0);
        // 4 items = 2 rows (additional 16 + tray GAP 3 = 19)
        assert_eq!(with_four - with_two, 19.0);

        // Horizontal bar widens with tray
        let lying_without = metrics.lying_width(1, 0, 0);
        let lying_two = metrics.lying_width(1, 0, 2);
        let lying_three = metrics.lying_width(1, 0, 3);
        // 2 items in horizontal dock = 1 column (width 16) + DIVIDER (9) + 2 * bar GAP (8)
        assert_eq!(lying_two - lying_without, 33.0);
        // 3 items = 2 columns (additional 16 + tray GAP 3 = 19)
        assert_eq!(lying_three - lying_two, 19.0);
    }

    #[test]
    fn tray_items_rects_are_positioned_correctly() {
        let metrics = Metrics::default();
        // Horizontal: 2 rows of items
        let rect0 = metrics.tray_rect(Orientation::Horizontal, 0, 1, 0, 3, false);
        let rect1 = metrics.tray_rect(Orientation::Horizontal, 1, 1, 0, 3, false);
        let rect2 = metrics.tray_rect(Orientation::Horizontal, 2, 1, 0, 3, false);

        // rect0 is col 0, row 0; rect1 is col 0, row 1; rect2 is col 1, row 0
        assert_eq!(rect0.0, rect1.0); // same x
        assert_eq!(rect1.1 - rect0.1, 19); // 16 + 3 GAP in y
        assert_eq!(rect2.0 - rect0.0, 19); // 16 + 3 GAP in x
        assert_eq!(rect0.1, rect2.1); // same y

        // Vertical: 2 columns of items
        let left = Orientation::Vertical(Side::Left);
        let vrect0 = metrics.tray_rect(left, 0, 1, 0, 3, false);
        let vrect1 = metrics.tray_rect(left, 1, 1, 0, 3, false);
        let vrect2 = metrics.tray_rect(left, 2, 1, 0, 3, false);

        // vrect0 is col 0, row 0; vrect1 is col 1, row 0; vrect2 is col 0, row 1
        assert_eq!(vrect0.1, vrect1.1); // same y
        assert_eq!(vrect1.0 - vrect0.0, 19); // 16 + 3 GAP in x
        assert_eq!(vrect2.1 - vrect0.1, 19); // 16 + 3 GAP in y
        assert_eq!(vrect0.0, vrect2.0); // same x
    }
}
