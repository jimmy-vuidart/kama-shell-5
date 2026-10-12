//! The bar: a thin column on the left edge that unfolds under the pointer.

use chrono::{DateTime, Datelike, Local, Timelike};
use iced::widget::{Column, Row, column, container, mouse_area, row, space, stack, text};
use iced::{Alignment, Element, Length, Padding};

use super::icons::{self, Glyph};
use super::style::{self, Palette};
use crate::app::{Dock, Message};
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

/// The height of the bar for a number of favorites and of other entries.
fn bar_height(pinned: usize, others: usize) -> f32 {
    // The launcher, a section, the favorites, maybe a section and the
    // others, the clock; a gap between each.
    let mut rows = vec![SLOT, SECTION];
    rows.extend(std::iter::repeat_n(SLOT, pinned));
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

/// Where the entry `index` is in the surface (x, y, width, height): what
/// its menu opens next to. `unfolded` says the bar shows at its full width.
pub fn entry_rect(index: usize, pinned: usize, unfolded: bool) -> (i32, i32, i32, i32) {
    let mut top = SHADOW + PADDING + SLOT + GAP + SECTION + GAP;
    top += index as f32 * (SLOT + GAP);
    if index >= pinned {
        top += SECTION + GAP;
    }
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

pub fn view(dock: &Dock) -> Element<'_, Message> {
    let palette = dock.palette();
    let unfold = ease(dock.unfold);
    bar(dock, unfold, true, palette)
}

/// The bar itself. The drawer shows it too, folded and still: `live` says
/// whether it reacts to the pointer.
pub fn bar(dock: &Dock, unfold: f32, live: bool, palette: Palette) -> Element<'_, Message> {
    let width = WIDTH + (UNFOLDED - WIDTH) * unfold;
    // Texts come once there is room for them.
    let labels = unfold > 0.4;
    let pinned = dock.pinned();
    let others = dock.entries.len() - pinned;

    let mut rows: Column<'_, Message> = column![launcher(dock, labels, live, palette)].spacing(GAP);
    rows = rows.push(section("FAVORIS", labels, palette));
    for (index, entry) in dock.entries.iter().enumerate() {
        if index == pinned {
            rows = rows.push(section("SUR CET ÉCRAN", labels, palette));
        }
        rows = rows.push(entry_row(dock, index, entry, labels, live, palette));
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
            .on_enter(Message::BarHover(true))
            .on_exit(Message::BarHover(false))
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
fn launcher(dock: &Dock, labels: bool, live: bool, palette: Palette) -> Element<'_, Message> {
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
        area.on_press(Message::Launcher).into()
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
    index: usize,
    entry: &'a Entry,
    labels: bool,
    live: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let hovered = live && dock.hover_entry == Some(index);
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
        .on_enter(Message::EntryHover(Some(index)))
        .on_exit(Message::EntryHover(None))
        .on_press(Message::EntryPress(index))
        .on_release(Message::EntryRelease(index))
        .on_middle_press(Message::EntryMiddle(index))
        .on_right_press(Message::EntryMenu(index))
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
        let empty = surface_height(0, 0);

        assert_eq!(surface_height(1, 0) - empty, (SLOT + GAP) as u32);
        // The first entry that is not a favorite brings its section along.
        assert_eq!(
            surface_height(1, 1) - surface_height(1, 0),
            (SLOT + GAP + SECTION + GAP) as u32
        );
    }

    #[test]
    fn the_menu_opens_next_to_its_entry() {
        let (x, first, width, height) = entry_rect(0, 3, false);
        let (_, second, ..) = entry_rect(1, 3, false);
        let (_, other, ..) = entry_rect(3, 3, false);
        let (.., unfolded, _) = entry_rect(0, 3, true);

        assert_eq!((x, width, height), (8, 44, 36));
        assert_eq!(unfolded, 200);
        assert_eq!(second - first, 40);
        // Past the favorites, the title of the second group is in between.
        assert_eq!(other - first, 3 * 40 + 22);
    }

    #[test]
    fn the_unfolding_starts_and_ends_gently() {
        assert!(ease(0.0).abs() < f32::EPSILON);
        assert!((ease(1.0) - 1.0).abs() < f32::EPSILON);
        assert!(ease(0.1) < 0.1 && ease(0.9) > 0.9);
    }
}
