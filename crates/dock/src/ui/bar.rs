//! The bar: a thin column on the left edge that unfolds under the pointer,
//! or a row on the bottom edge that names the icon under it.

use chrono::{DateTime, Datelike, Local, Timelike};
use iced::widget::{Column, Row, column, container, mouse_area, row, space, stack, text};
use iced::{Alignment, Element, Length, Padding};

use super::icons::{self, Glyph};
use super::style::{self, Palette};
use crate::app::{BarState, Dock, Message};
use crate::model::Orientation;
use crate::model::bar::{Entry, Indicator};

/// The surface is as wide as the unfolded bar, with room for its shadow.
pub const SURFACE_WIDTH: u32 = 236;
/// What takes the pointer when the bar is unfolded: the bar, not its shadow.
pub const UNFOLDED_REACH: u32 = 212;
/// What the bar keeps for itself along the edge: windows start after it.
pub const EXCLUSIVE_ZONE: i32 = 60;
/// Between the edge of the screen and the bar.
const MARGIN: f32 = 8.0;
/// Above and under the bar, for its shadow.
const SHADOW: f32 = 24.0;
const WIDTH: f32 = 44.0;
const UNFOLDED: f32 = 200.0;
const RADIUS: f32 = 14.0;
const SLOT: f32 = 36.0;
const ICON: f32 = 24.0;
const GAP: f32 = 4.0;
/// Inside the bar, above the launcher and under the clock.
const PADDING: f32 = 6.0;
/// A separation between two groups, or its title when unfolded.
const SECTION: f32 = 18.0;
const CLOCK: f32 = 54.0;
/// The room left of an icon for its mark.
const MARK: f32 = 4.0;

/// Lying down: the room above the bar for the label of the icon under the
/// pointer, and on each side for a label that is wider than its icon.
const LABEL_ROOM: f32 = 58.0;
const SIDE: f32 = 110.0;
/// Lying down: a hairline between two groups, and the clock at the end.
const DIVIDER: f32 = 9.0;
const CLOCK_WIDTH: f32 = 62.0;

/// The height of the bar for a number of favorites and of other entries.
fn bar_height(pinned: usize, others: usize) -> f32 {
    // The launcher, the favorites and the others, each group under its
    // section when it has entries, the clock; a gap between each.
    let mut rows = vec![SLOT];
    if pinned > 0 {
        rows.push(SECTION);
        rows.extend(std::iter::repeat_n(SLOT, pinned));
    }
    if others > 0 {
        rows.push(SECTION);
        rows.extend(std::iter::repeat_n(SLOT, others));
    }
    rows.push(CLOCK);
    let gaps = GAP * (rows.len() - 1) as f32;
    rows.iter().sum::<f32>() + gaps + 2.0 * PADDING
}

/// The height of the surface that holds the bar.
pub fn surface_height(pinned: usize, others: usize) -> u32 {
    (bar_height(pinned, others) + 2.0 * SHADOW).ceil() as u32
}

/// The width of a lying bar for a number of favorites and of other
/// entries.
fn lying_width(pinned: usize, others: usize) -> f32 {
    // The launcher, each group behind its hairline, a hairline, the clock;
    // a gap between each.
    let mut items = vec![SLOT];
    for group in [pinned, others] {
        if group > 0 {
            items.push(DIVIDER);
            items.extend(std::iter::repeat_n(SLOT, group));
        }
    }
    items.extend([DIVIDER, CLOCK_WIDTH]);
    let gaps = GAP * (items.len() - 1) as f32;
    items.iter().sum::<f32>() + gaps + 2.0 * PADDING
}

/// How far from the left end of a lying bar the entry `index` starts.
fn lying_left(index: usize, pinned: usize) -> f32 {
    // The hairline of its group, and the one of the favorites when it
    // comes after them.
    let dividers = if index >= pinned && pinned > 0 {
        2.0
    } else {
        1.0
    };
    PADDING + SLOT + GAP + dividers * (DIVIDER + GAP) + index as f32 * (SLOT + GAP)
}

/// The size of the surface of a bar, for a number of favorites and of
/// other entries.
pub fn surface_size(orientation: Orientation, pinned: usize, others: usize) -> (u32, u32) {
    match orientation {
        Orientation::Vertical => (SURFACE_WIDTH, surface_height(pinned, others)),
        Orientation::Horizontal => {
            let width = lying_width(pinned, others) + 2.0 * SIDE;
            (width.ceil() as u32, (LABEL_ROOM + WIDTH + MARGIN) as u32)
        }
    }
}

/// The part of its surface where a bar takes the pointer (x, y, width,
/// height): what shows, so that the windows around it stay clickable.
pub fn input_region(
    orientation: Orientation,
    size: (u32, u32),
    unfolded: bool,
) -> (i32, i32, i32, i32) {
    let (width, height) = (size.0 as i32, size.1 as i32);
    match orientation {
        Orientation::Vertical if unfolded => (0, 0, UNFOLDED_REACH as i32, height),
        Orientation::Vertical => (0, 0, EXCLUSIVE_ZONE, height),
        Orientation::Horizontal => {
            let side = SIDE as i32;
            (
                side,
                LABEL_ROOM as i32,
                width - 2 * side,
                (WIDTH + MARGIN) as i32,
            )
        }
    }
}

/// Where the entry `index` is in the surface of its bar (x, y, width,
/// height): what its menu opens next to. `unfolded` says an upright bar
/// shows at its full width.
pub fn entry_rect(
    orientation: Orientation,
    index: usize,
    pinned: usize,
    unfolded: bool,
) -> (i32, i32, i32, i32) {
    match orientation {
        Orientation::Vertical => upright_rect(index, pinned, unfolded),
        // The whole height of the bar, and a little more: the menu opens
        // just above it.
        Orientation::Horizontal => (
            (SIDE + lying_left(index, pinned)) as i32,
            (LABEL_ROOM - 6.0) as i32,
            SLOT as i32,
            (WIDTH + 6.0) as i32,
        ),
    }
}

fn upright_rect(index: usize, pinned: usize, unfolded: bool) -> (i32, i32, i32, i32) {
    let mut top = SHADOW + PADDING + SLOT + GAP;
    top += index as f32 * (SLOT + GAP);
    // The titles of the groups above it: its own, and the one of the
    // favorites when it comes after them.
    let sections = if index >= pinned && pinned > 0 {
        2.0
    } else {
        1.0
    };
    top += sections * (SECTION + GAP);
    let width = if unfolded { UNFOLDED } else { WIDTH };
    (MARGIN as i32, top as i32, width as i32, SLOT as i32)
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
    match state.orientation {
        Orientation::Vertical => bar(dock, state, ease(state.unfold), true, palette),
        Orientation::Horizontal => {
            let label: Element<'_, Message> = match hovered_entry(dock, state) {
                Some((index, entry)) => label(dock, state, index, entry, palette),
                None => space().into(),
            };
            let lying = container(lying(dock, state, true, palette)).center_x(Length::Fill);
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
    match state.orientation {
        Orientation::Vertical => bar(dock, state, 0.0, false, palette),
        Orientation::Horizontal => {
            let lying = container(lying(dock, state, false, palette)).center_x(Length::Fill);
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

/// The name and the state of the entry under the pointer, above its icon.
fn label<'a>(
    dock: &Dock,
    state: &BarState,
    index: usize,
    entry: &'a Entry,
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
        .style(move |_| {
            style::outlined(
                iced::Color {
                    a: 1.0,
                    ..palette.bar
                },
                palette.bar_border,
                10.0,
            )
        });
    let left = (SIDE + lying_left(index, state.pinned()) - 8.0).max(0.0);
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
    palette: Palette,
) -> Element<'a, Message> {
    let pinned = state.pinned();
    let divider = || {
        let line = container(space().width(1).height(22))
            .style(move |_| style::fill(palette.separator, 0.0));
        container(line).center_x(DIVIDER).center_y(WIDTH)
    };

    let mut items: Row<'_, Message> = row![lying_launcher(dock, state, live, palette)].spacing(GAP);
    for (index, entry) in state.entries.iter().enumerate() {
        if index == 0 || index == pinned {
            items = items.push(divider());
        }
        items = items.push(lying_entry(dock, state, index, entry, live, palette));
    }
    items = items.push(divider()).push(lying_time(&dock.now, palette));

    let body = container(items)
        .height(WIDTH)
        .padding(Padding::from([0.0, PADDING]))
        .style(move |_| container::Style {
            shadow: palette.shadow(),
            ..style::outlined(palette.bar, palette.bar_border, RADIUS)
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
    palette: Palette,
) -> Element<'a, Message> {
    let open = dock.drawer.is_some();
    let (color, background) = if open {
        (palette.on_accent(), palette.accent)
    } else {
        (palette.accent, iced::Color::TRANSPARENT)
    };
    let icon = container(icons::glyph(Glyph::Launcher, 22.0, color))
        .center_x(SLOT)
        .center_y(SLOT)
        .style(move |_| style::fill(background, 10.0));
    let area = mouse_area(container(icon).center_y(WIDTH));
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
    let icon = container(icons::app(&dock.icons, &entry.identity, ICON))
        .center_x(SLOT)
        .center_y(SLOT)
        .style(move |_| style::fill(tile, 10.0));
    let icon: Element<'_, Message> = if unread > 0 {
        let count = container(badge(unread, 14.0))
            .width(SLOT)
            .height(SLOT)
            .align_x(Alignment::End)
            .align_y(Alignment::Start);
        stack![icon, count].into()
    } else {
        icon.into()
    };
    let slot = column![
        space().height(2),
        icon,
        container(mark).center_x(SLOT).height(WIDTH - SLOT - 2.0)
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

fn lying_time<'a>(now: &DateTime<Local>, palette: Palette) -> Element<'a, Message> {
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
        .center_y(WIDTH)
        .into()
}

/// The bar standing on the left edge. The drawer shows it too, folded and
/// still: `live` says whether it reacts to the pointer.
fn bar<'a>(
    dock: &'a Dock,
    state: &'a BarState,
    unfold: f32,
    live: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let width = WIDTH + (UNFOLDED - WIDTH) * unfold;
    // Texts come once there is room for them.
    let labels = unfold > 0.4;
    let pinned = state.pinned();
    let others = state.entries.len() - pinned;

    let mut rows: Column<'_, Message> =
        column![launcher(dock, state, labels, live, palette)].spacing(GAP);
    for (index, entry) in state.entries.iter().enumerate() {
        if index == 0 && pinned > 0 {
            rows = rows.push(section("FAVORIS", labels, palette));
        }
        if index == pinned {
            rows = rows.push(section("OUVERTES", labels, palette));
        }
        rows = rows.push(entry_row(dock, state, index, entry, labels, live, palette));
    }
    rows = rows.push(space().height(Length::Fill));
    rows = rows.push(time(&dock.now, labels, palette));

    let body = container(rows)
        .width(width)
        .height(bar_height(pinned, others))
        .padding(Padding::from([PADDING, 0.0]))
        .clip(true)
        .style(move |_| container::Style {
            shadow: palette.shadow(),
            ..style::outlined(palette.bar, palette.bar_border, RADIUS)
        });
    let body: Element<'_, Message> = if live {
        mouse_area(body)
            .on_enter(Message::BarHover(state.id, true))
            .on_exit(Message::BarHover(state.id, false))
            .into()
    } else {
        body.into()
    };
    container(body)
        .padding(Padding {
            left: MARGIN,
            top: SHADOW,
            bottom: SHADOW,
            right: 0.0,
        })
        .into()
}

/// The button of the drawer, at the top of the bar.
fn launcher<'a>(
    dock: &Dock,
    state: &BarState,
    labels: bool,
    live: bool,
    palette: Palette,
) -> Element<'a, Message> {
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
    let icon = container(icons::glyph(Glyph::Launcher, 22.0, color))
        .center_x(SLOT)
        .center_y(SLOT)
        .style(move |_| style::fill(background, 10.0));
    let mut content: Row<'_, Message> = row![space().width(MARK), icon].align_y(Alignment::Center);
    if labels {
        let label = text("Lanceur")
            .size(14)
            .font(style::SEMIBOLD)
            .color(palette.text);
        content = content.push(space().width(8)).push(label);
    }
    let area = mouse_area(container(content).height(SLOT).width(Length::Fill));
    if live || open {
        area.on_press(Message::Launcher(Some(state.screen))).into()
    } else {
        area.into()
    }
}

/// A hairline between two groups; unfolded, the name of the group.
fn section(title: &'static str, labels: bool, palette: Palette) -> Element<'static, Message> {
    if labels {
        let title = text(title)
            .size(10)
            .font(style::SEMIBOLD)
            .color(palette.muted);
        return container(title)
            .height(SECTION)
            .padding(Padding::from([0.0, 14.0]))
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
    labels: bool,
    live: bool,
    palette: Palette,
) -> Element<'a, Message> {
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
    let mark = container(mark)
        .width(MARK)
        .center_y(SLOT)
        .align_x(Alignment::End);

    let tile = if hovered && !labels || dragged {
        palette.hover
    } else {
        iced::Color::TRANSPARENT
    };
    let icon = container(icons::app(&dock.icons, &entry.identity, ICON))
        .center_x(SLOT)
        .center_y(SLOT)
        .style(move |_| style::fill(tile, 10.0));
    // Folded, the count of unread messages sits on the corner of the icon.
    let icon: Element<'_, Message> = if unread > 0 && !labels {
        let count = container(badge(unread, 14.0))
            .width(SLOT)
            .height(SLOT)
            .align_x(Alignment::End)
            .align_y(Alignment::Start);
        stack![icon, count].into()
    } else {
        icon.into()
    };

    let mut content: Row<'_, Message> = row![mark, icon].align_y(Alignment::Center);
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
            .clip(true);
        content = content.push(space().width(8)).push(texts);
        if unread > 0 {
            content = content.push(badge(unread, 18.0)).push(space().width(10));
        }
    }
    let background = if hovered && labels {
        palette.hover
    } else {
        iced::Color::TRANSPARENT
    };
    let line = container(content)
        .height(SLOT)
        .width(Length::Fill)
        .style(move |_| style::fill(background, 10.0));
    let line = container(line).padding(Padding::from([0.0, if labels { 4.0 } else { 0.0 }]));
    if !live {
        return line.into();
    }
    mouse_area(line)
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

fn time<'a>(now: &DateTime<Local>, labels: bool, palette: Palette) -> Element<'a, Message> {
    let (hours, minutes, short, long) = clock(now);
    if labels {
        let time = text(format!("{hours}:{minutes}"))
            .size(20)
            .font(style::BOLD)
            .color(palette.text);
        let date = text(long).size(12).color(palette.muted);
        let line = row![time, date].spacing(10).align_y(Alignment::Center);
        return container(line)
            .height(CLOCK)
            .padding(Padding::from([0.0, 14.0]))
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
        let upright =
            |index, pinned, unfolded| entry_rect(Orientation::Vertical, index, pinned, unfolded);
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
        assert_eq!(surface_size(Orientation::Vertical, 3, 2).0, SURFACE_WIDTH);
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
        let upright = surface_size(Orientation::Vertical, 2, 0);
        let lying = surface_size(Orientation::Horizontal, 2, 0);

        assert_eq!(
            input_region(Orientation::Vertical, upright, false),
            (0, 0, 60, upright.1 as i32)
        );
        assert_eq!(
            input_region(Orientation::Vertical, upright, true).2,
            UNFOLDED_REACH as i32
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
}
