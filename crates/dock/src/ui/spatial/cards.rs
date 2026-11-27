//! Informational Liquid Glass cards for the spatial launcher:
//! « Vie quotidienne », « Tâches des projets », « Jeux » (with inline editable notes),
//! and « Agenda · 7 jours ».

use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Row, button, column, container, image, mouse_area, row, space, stack, text, text_input,
};
use iced::{Alignment, Border, Color, ContentFit, Element, Length, Padding};

use crate::app::{Dock, Message};
use crate::model::widgets::{
    CalendarEvent, DailyTask, ProjectTask, StartedGame, day_label, range_label,
};
use crate::ui::icons::{self, Glyph};
use crate::ui::style::{self, Palette};

/// Text input identifier for the inline game note editor, used to auto-focus
/// when `Message::GameNoteEdit` is handled.
pub const GAME_NOTE_INPUT: &str = "spatial-game-note";

/// Rows of tiles a paginated card shows on one page.
const GAMES_ROWS: usize = 2;
const CALENDAR_ROWS: usize = 3;

/// The paginated cards of the spatial launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    Games,
    Calendar,
}

/// Compact pager (‹ page/total ›), or nothing when a single page holds everything.
fn card_pager<'a>(
    card: CardKind,
    page: usize,
    pages: usize,
    palette: Palette,
) -> Option<Element<'a, Message>> {
    if pages <= 1 {
        return None;
    }
    let arrow = |glyph: Glyph, target: Option<usize>| {
        let enabled = target.is_some();
        let color = if enabled {
            palette.text
        } else {
            style::alpha(palette.muted, 0.45)
        };
        let btn = button(container(icons::glyph(glyph, 12.0, color)).center(Length::Fill))
            .width(22)
            .height(22)
            .padding(0)
            .style(style::flat(palette.hover, 7.0));
        match target {
            Some(page) => btn.on_press(Message::SpatialCardPage(card, page)),
            None => btn,
        }
    };
    let label = text(format!("{} / {pages}", page + 1))
        .size(11.5)
        .font(style::MEDIUM)
        .color(palette.muted);
    Some(
        row![
            arrow(Glyph::ChevronLeft, page.checked_sub(1)),
            label,
            arrow(Glyph::ChevronRight, (page + 1 < pages).then_some(page + 1)),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into(),
    )
}

/// Items of `page` (clamped), with the clamped page and the page count.
fn paginate<T>(items: Vec<T>, page: usize, size: usize) -> (Vec<T>, usize, usize) {
    let size = size.max(1);
    let pages = items.len().div_ceil(size).max(1);
    let page = page.min(pages - 1);
    let visible = items.into_iter().skip(page * size).take(size).collect();
    (visible, page, pages)
}

fn glass_card<'a>(
    title: &'a str,
    meta: String,
    pager: Option<Element<'a, Message>>,
    body: Element<'a, Message>,
    fill_height: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let dot =
        container(space().width(8).height(8)).style(move |_| style::fill(palette.accent, 4.0));
    let title_label = text(title)
        .size(12)
        .font(style::SEMIBOLD)
        .color(palette.muted);
    let meta_label = text(meta)
        .size(11.5)
        .font(style::MEDIUM)
        .color(palette.muted);
    let mut header = row![dot, title_label, space().width(Length::Fill), meta_label]
        .spacing(8)
        .align_y(Alignment::Center);
    if let Some(pager) = pager {
        header = header.push(pager);
    }

    container(column![header, body].spacing(10))
        .width(Length::Fill)
        .height(if fill_height {
            Length::Fill
        } else {
            Length::Shrink
        })
        .padding(Padding::from([14.0, 18.0]))
        .style(move |_| style::outlined(palette.glass, palette.rim, 20.0))
        .into()
}

/// « Vie quotidienne » interactive checklist card.
pub fn daily_tasks_card<'a>(
    dock: &'a Dock,
    fill_height: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let rem = dock.widgets.remaining_daily_tasks();
    let meta = remaining_label(rem);
    let items: Vec<Element<'a, Message>> = dock
        .widgets
        .daily_tasks
        .iter()
        .map(|task| daily_task_row(task, palette))
        .collect();
    let body = Column::with_children(items).spacing(4).into();
    glass_card("VIE QUOTIDIENNE", meta, None, body, fill_height, palette)
}

fn remaining_label(remaining: usize) -> String {
    if remaining == 1 {
        "1 restante".to_owned()
    } else {
        format!("{remaining} restantes")
    }
}

fn daily_task_row<'a>(task: &'a DailyTask, palette: Palette) -> Element<'a, Message> {
    let done = task.done;
    let checkbox: Element<'a, Message> = if done {
        container(icons::glyph(Glyph::Check, 11.0, palette.on_accent()))
            .center_x(17)
            .center_y(17)
            .style(move |_| style::fill(palette.accent, 8.5))
            .into()
    } else {
        container(space().width(17).height(17))
            .style(move |_| container::Style {
                background: Some(Color::TRANSPARENT.into()),
                border: Border {
                    color: palette.muted,
                    width: 1.5,
                    radius: 8.5.into(),
                },
                ..container::Style::default()
            })
            .into()
    };

    let title_color = if done { palette.muted } else { palette.text };
    let title = text(&task.title)
        .size(13)
        .font(style::MEDIUM)
        .color(title_color);
    let meta = text(&task.meta).size(11.5).color(palette.muted);

    let line = row![checkbox, title, space().width(Length::Fill), meta]
        .spacing(10)
        .align_y(Alignment::Center);

    button(line)
        .width(Length::Fill)
        .padding(Padding::from([8.0, 10.0]))
        .on_press(Message::DailyTaskToggle(task.id.clone()))
        .style(style::flat(palette.hover, 12.0))
        .into()
}

/// « Tâches des projets » simulated project tickets card.
pub fn project_tasks_card<'a>(
    dock: &'a Dock,
    fill_height: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let items: Vec<Element<'a, Message>> = dock
        .widgets
        .project_tasks
        .iter()
        .map(|task| project_task_row(task, palette))
        .collect();
    let body = Column::with_children(items).spacing(6).into();
    glass_card(
        "TÂCHES DES PROJETS",
        "Données simulées".to_owned(),
        None,
        body,
        fill_height,
        palette,
    )
}

fn project_task_row<'a>(task: &'a ProjectTask, palette: Palette) -> Element<'a, Message> {
    let top = row![
        text(&task.id)
            .size(11.5)
            .font(style::SEMIBOLD)
            .color(palette.accent),
        text(" · ").size(11.5).color(palette.muted),
        text(&task.project).size(11.5).color(palette.muted),
    ]
    .align_y(Alignment::Center);

    let title = text(&task.title)
        .size(12.5)
        .font(style::SEMIBOLD)
        .color(palette.text);

    let left = column![top, title].spacing(2).width(Length::Fill);

    let pill = container(
        text(&task.status)
            .size(11)
            .font(style::SEMIBOLD)
            .color(palette.accent),
    )
    .padding(Padding::from([3.0, 9.0]))
    .style(move |_| style::fill(style::alpha(palette.accent, 0.16), 999.0));

    container(row![left, pill].spacing(10).align_y(Alignment::Center))
        .width(Length::Fill)
        .padding(Padding::from([7.0, 12.0]))
        .style(move |_| style::outlined(palette.card, palette.separator, 12.0))
        .into()
}

/// « Jeux » started games card with compact Steam cover banners and inline editable notes.
pub fn games_card<'a>(
    dock: &'a Dock,
    columns: usize,
    fill_height: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let games =
        dock.widgets
            .started_games(&dock.games, &dock.playtimes, dock.now.timestamp(), true);
    let columns = columns.max(1);
    let (games, page, pages) = paginate(games, dock.games_page, columns * GAMES_ROWS);
    let cells: Vec<Element<'a, Message>> = games
        .into_iter()
        .map(|game| game_tile(dock, game, palette))
        .collect();
    let body = grid(cells, columns, 8.0);
    glass_card(
        "JEUX",
        "Clic sur le commentaire pour éditer".to_owned(),
        card_pager(CardKind::Games, page, pages, palette),
        body,
        fill_height,
        palette,
    )
}

fn game_tile<'a>(dock: &'a Dock, game: StartedGame, palette: Palette) -> Element<'a, Message> {
    let fallback_bg = style::mix(palette.glass_strong, palette.accent, 0.25);
    let background: Element<'a, Message> = match &game.cover {
        Some(path) => image(image::Handle::from_path(path))
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(ContentFit::Cover)
            .border_radius(12)
            .into(),
        None => container(space().width(Length::Fill).height(Length::Fill))
            .style(move |_| style::fill(fallback_bg, 12.0))
            .into(),
    };

    let bottom_scrim = container(
        container(space().width(Length::Fill).height(54))
            .style(move |_| style::fill(Color::from_rgba(0.06, 0.05, 0.12, 0.62), 0.0)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::End);

    let meta_pill = container(
        text(game.meta_pill)
            .size(10)
            .font(style::SEMIBOLD)
            .color(Color::WHITE),
    )
    .padding(Padding::from([2.0, 7.0]))
    .style(|_| style::fill(Color::from_rgba(0.06, 0.05, 0.12, 0.58), 999.0));

    let play_badge = container(icons::glyph(Glyph::Play, 9.5, Color::WHITE))
        .center_x(22)
        .center_y(22)
        .style(|_| style::fill(Color::from_rgba(0.06, 0.05, 0.12, 0.58), 11.0));

    let top_row =
        row![meta_pill, space().width(Length::Fill), play_badge].align_y(Alignment::Center);

    let title = container(
        text(game.title)
            .size(12.5)
            .font(style::BOLD)
            .color(Color::WHITE)
            .wrapping(Wrapping::None),
    )
    .width(Length::Fill)
    .max_height(17)
    .clip(true);

    let launch_zone = mouse_area(
        container(column![top_row, space().height(Length::Fill), title].spacing(2))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding {
                top: 7.0,
                right: 8.0,
                bottom: 2.0,
                left: 8.0,
            }),
    )
    .on_press(Message::SpatialLaunchGame(game.key.clone()));

    let note_zone = game_note_zone(dock, &game.key, game.note, palette);

    let foreground = column![launch_zone, note_zone]
        .width(Length::Fill)
        .height(Length::Fill);

    container(stack![background, bottom_scrim, foreground])
        .width(Length::FillPortion(1))
        .height(88)
        .clip(true)
        .style(move |_| container::Style {
            border: Border {
                color: palette.rim,
                width: 1.0,
                radius: 12.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn game_note_zone<'a>(
    dock: &'a Dock,
    key: &str,
    note: String,
    palette: Palette,
) -> Element<'a, Message> {
    let editing = dock
        .editing_game
        .as_ref()
        .filter(|(editing_key, _)| editing_key == key);

    let content: Element<'a, Message> = if let Some((_, draft)) = editing {
        let input = text_input("Ajouter un commentaire…", draft)
            .id(GAME_NOTE_INPUT)
            .on_input(Message::GameNoteDraft)
            .on_submit(Message::GameNoteSave)
            .size(11.5)
            .padding(Padding::from([4.0, 8.0]))
            .width(Length::Fill)
            .style(move |_, _| text_input::Style {
                background: Color::from_rgba(0.06, 0.05, 0.12, 0.65).into(),
                border: Border {
                    color: palette.accent,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                icon: Color::WHITE,
                placeholder: Color::from_rgba(1.0, 1.0, 1.0, 0.6),
                value: Color::WHITE,
                selection: style::alpha(palette.accent, 0.45),
            });

        let save = button(icons::glyph(Glyph::Check, 12.0, Color::WHITE))
            .padding(Padding::from([5.0, 6.0]))
            .on_press(Message::GameNoteSave)
            .style(style::tinted(
                palette.accent,
                style::alpha(palette.accent, 0.85),
                8.0,
            ));

        let cancel = button(icons::glyph(Glyph::Close, 12.0, Color::WHITE))
            .padding(Padding::from([5.0, 6.0]))
            .on_press(Message::GameNoteCancel)
            .style(style::flat(Color::from_rgba(1.0, 1.0, 1.0, 0.18), 8.0));

        row![input, save, cancel]
            .spacing(4)
            .align_y(Alignment::Center)
            .into()
    } else {
        let label = if note.is_empty() {
            "Ajouter un commentaire…".to_owned()
        } else {
            note.clone()
        };
        let label_color = if note.is_empty() {
            Color::from_rgba(1.0, 1.0, 1.0, 0.68)
        } else {
            Color::from_rgba(1.0, 1.0, 1.0, 0.92)
        };
        let note_text = container(
            text(label)
                .size(10.5)
                .color(label_color)
                .wrapping(Wrapping::None),
        )
        .width(Length::Fill)
        .max_height(15)
        .clip(true);
        let edit_icon = icons::glyph(Glyph::Edit, 10.5, Color::from_rgba(1.0, 1.0, 1.0, 0.78));

        button(
            row![note_text, edit_icon]
                .spacing(5)
                .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding(Padding::from([2.0, 5.0]))
        .on_press(Message::GameNoteEdit(key.to_owned(), note))
        .style(style::flat(Color::from_rgba(1.0, 1.0, 1.0, 0.16), 6.0))
        .into()
    };

    container(content)
        .width(Length::Fill)
        .padding(Padding {
            top: 0.0,
            right: 6.0,
            bottom: 5.0,
            left: 6.0,
        })
        .into()
}

/// « Agenda · 7 jours » upcoming events card.
pub fn calendar_card<'a>(
    dock: &'a Dock,
    columns: usize,
    fill_height: bool,
    palette: Palette,
) -> Element<'a, Message> {
    let today = dock.now.date_naive();
    let meta = range_label(today);
    let columns = columns.max(1);
    let events: Vec<&CalendarEvent> = dock.widgets.calendar_events.iter().collect();
    let (events, page, pages) = paginate(events, dock.calendar_page, columns * CALENDAR_ROWS);
    let cells: Vec<Element<'a, Message>> = events
        .into_iter()
        .map(|ev| calendar_event_tile(ev, today, palette))
        .collect();
    let body = grid(cells, columns, 8.0);
    let pager = card_pager(CardKind::Calendar, page, pages, palette);
    glass_card("AGENDA · 7 JOURS", meta, pager, body, fill_height, palette)
}

fn calendar_event_tile<'a>(
    ev: &'a CalendarEvent,
    today: chrono::NaiveDate,
    palette: Palette,
) -> Element<'a, Message> {
    let is_today = ev.day_offset == 0;
    let border_color = if is_today {
        style::alpha(palette.accent, 0.45)
    } else {
        palette.separator
    };
    let stripe_color = style::parse_color(&ev.accent_hex).unwrap_or(palette.accent);
    let stripe =
        container(space().width(3.5).height(30)).style(move |_| style::fill(stripe_color, 2.0));

    let day_color = if is_today {
        palette.accent
    } else {
        palette.muted
    };
    let top = row![
        text(day_label(ev.day_offset, today))
            .size(11)
            .font(style::SEMIBOLD)
            .color(day_color),
        space().width(Length::Fill),
        text(&ev.time).size(11).color(palette.muted),
    ]
    .align_y(Alignment::Center);

    let title = text(&ev.title)
        .size(12.5)
        .font(style::SEMIBOLD)
        .color(palette.text);

    let details = column![top, title].spacing(2).width(Length::Fill);

    container(row![stripe, details].spacing(10).align_y(Alignment::Center))
        .width(Length::FillPortion(1))
        .padding(Padding::from([9.0, 11.0]))
        .style(move |_| style::outlined(palette.card, border_color, 13.0))
        .into()
}

fn grid<'a>(cells: Vec<Element<'a, Message>>, columns: usize, gap: f32) -> Element<'a, Message> {
    let mut rows: Column<'_, Message> = column![].spacing(gap);
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let mut line: Row<'_, Message> = row![].spacing(gap);
        for _ in 0..columns {
            line = line.push(match cells.next() {
                Some(cell) => cell,
                None => space().width(Length::FillPortion(1)).into(),
            });
        }
        rows = rows.push(line);
    }
    rows.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paginate_clamps_and_slices() {
        let items: Vec<u32> = (0..7).collect();
        assert_eq!(paginate(items.clone(), 0, 3), (vec![0, 1, 2], 0, 3));
        assert_eq!(paginate(items.clone(), 2, 3), (vec![6], 2, 3));
        assert_eq!(paginate(items, 9, 3), (vec![6], 2, 3));
        assert_eq!(paginate(Vec::<u32>::new(), 4, 3), (vec![], 0, 1));
    }

    #[test]
    fn remaining_label_formats_singular_and_plural() {
        assert_eq!(remaining_label(0), "0 restantes");
        assert_eq!(remaining_label(1), "1 restante");
        assert_eq!(remaining_label(4), "4 restantes");
    }
}
