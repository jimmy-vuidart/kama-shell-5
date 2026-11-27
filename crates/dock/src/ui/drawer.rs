//! The launcher: search, pinned applications, every application by pages,
//! games, the notifications, and the control center with its mixer. Beside
//! an upright bar it is a drawer as high as the screen; above a lying bar, a
//! drawer as wide as the screen.

use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Row, button, column, container, image, mouse_area, row, space, stack, text, text_input,
};
use iced::{Alignment, Border, Color, ContentFit, Element, Length, Padding, Vector, border, mouse};

use super::controls::{
    ControlView, control_center, heading, mixer_card, mixer_panel, notifications, rule,
};
use super::grip::grip;
use super::icons::{self, Glyph};
use super::opaque::opaque;
use super::shift::shift;
use super::style::{self, Palette};
use crate::app::{Dock, Drawer, Message, SEARCH, Target};
use crate::model::drawer;
use crate::model::{Identity, Orientation, Side};

/// The strip beside the drawer that is dragged to resize it.
const EDGE: f32 = 14.0;
/// How many games fit in the row.
const GAMES: usize = 5;
/// How many pinned applications fit in the row.
const PINNED: usize = 7;

pub fn view<'a>(dock: &'a Dock, drawer: &'a Drawer) -> Element<'a, Message> {
    let palette = dock.palette();
    let side = match drawer.orientation {
        Orientation::Vertical(side) => side,
        Orientation::Horizontal => return lying(dock, drawer, palette),
    };
    let shown = drawer.reveal.at(dock.moment);
    // What is left of the screen is dimmed, and a click there closes.
    let scrim = scrim(palette, shown);

    // From the edge of the screen inwards.
    let mut front: Vec<Element<'_, Message>> = vec![
        opaque(panel(dock, drawer, side, palette)),
        edge(drawer, palette),
    ];
    if drawer.mixer {
        // Next to the button that opens it, at the foot of the drawer: it
        // grows in, rising a little.
        let open = drawer.mixer_reveal.at(dock.moment);
        let mixer = shift(
            opaque(mixer_panel(dock, palette)),
            Vector::new(0.0, 12.0 * (1.0 - open)),
        )
        .scale(0.94 + 0.06 * open);
        let aligned = column![space().height(Length::Fill), mixer, space().height(28)];
        front.push(aligned.into());
    }
    let front: Element<'_, Message> = match side {
        Side::Left => Row::with_children(front).into(),
        Side::Right => {
            front.reverse();
            container(Row::with_children(front))
                .width(Length::Fill)
                .align_x(Alignment::End)
                .into()
        }
    };
    if shown >= 1.0 {
        return stack![scrim, front].into();
    }
    // On its way, it comes out from behind its bar: what is under the bar
    // stays, the glass there getting as opaque as the drawer is out.
    let bar_room = dock.metrics().exclusive_zone as f32;
    let travel = (drawer.width + EDGE - bar_room) * (1.0 - shown);
    let (offset, cut) = match side {
        Side::Left => (-travel, Padding::ZERO.left(bar_room)),
        Side::Right => (travel, Padding::ZERO.right(bar_room)),
    };
    let front = shift(front, Vector::new(offset, 0.0)).cut(cut);
    let under = bar_copy(dock, drawer, palette, shown);
    let under = container(container(under).width(bar_room))
        .width(Length::Fill)
        .align_x(match side {
            Side::Left => Alignment::Start,
            Side::Right => Alignment::End,
        });
    stack![scrim, under, front].into()
}

/// The dim veil over the rest of the screen, as dark as the launcher is
/// out; a click on it closes the launcher.
fn scrim<'a>(palette: Palette, shown: f32) -> Element<'a, Message> {
    let veil = style::alpha(palette.scrim, palette.scrim.a * shown);
    let scrim = container(space())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| style::fill(veil, 0.0));
    mouse_area(scrim).on_press(Message::CloseDrawer).into()
}

/// The copy of the bar the launcher draws over it, on the glass of the
/// launcher made as opaque as it is `shown`.
fn bar_copy<'a>(
    dock: &'a Dock,
    drawer: &'a Drawer,
    palette: Palette,
    shown: f32,
) -> Element<'a, Message> {
    let bar: Element<'_, Message> = match dock.bar_on(drawer.screen) {
        Some(state) => super::bar::still(dock, state, palette),
        None => space().into(),
    };
    let glass = style::alpha(palette.glass, palette.glass.a * shown);
    container(bar)
        .width(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_| style::fill(glass, 0.0))
        .into()
}

/// The edge of the drawer away from its bar, which is dragged to make it
/// wider or narrower. It shows as a small grip, brighter while it is held.
fn edge<'a>(drawer: &Drawer, palette: Palette) -> Element<'a, Message> {
    let color = if drawer.resizing {
        palette.accent
    } else {
        palette.rim
    };
    let handle = container(space().width(4).height(44)).style(move |_| style::fill(color, 2.0));
    let strip = container(handle)
        .width(EDGE)
        .center_x(EDGE)
        .center_y(Length::Fill);
    let held = drawer.resizing;
    let interaction = mouse::Interaction::ResizingHorizontally;
    grip(
        strip,
        held,
        Message::ResizeStart,
        Message::ResizeTo,
        Message::ResizeEnd,
        interaction,
    )
}

fn panel<'a>(
    dock: &'a Dock,
    drawer: &'a Drawer,
    side: Side,
    palette: Palette,
) -> Element<'a, Message> {
    let bar: Element<'_, Message> = match dock.bar_on(drawer.screen) {
        Some(state) => super::bar::still(dock, state, palette),
        None => space().into(),
    };
    let bar_room = dock.metrics().exclusive_zone as f32;
    let bar = container(bar).width(bar_room).center_y(Length::Fill);

    let mut content: Column<'_, Message> = column![search(drawer, palette)].spacing(12);
    if drawer.query.trim().is_empty() {
        // Nothing pinned: no empty heading.
        if !dock.config.pinned.is_empty() {
            content = content
                .push(pinned(dock, drawer.width - bar_room, palette))
                .push(rule(palette));
        }
        content = content
            .push(applications(dock, drawer, palette))
            .push(rule(palette))
            .push(games(dock, palette));
    } else {
        content = content.push(results(dock, drawer, palette));
    }
    // At the foot, the notifications on the left, as high as the control
    // center on the right, whichever side the bar is on.
    // Fixed widths: iced then measures the control center first, and gives
    // its height to the notifications.
    let half = (drawer.width - bar_room - 20.0 - 12.0) / 2.0;
    let control = control_center(
        dock,
        ControlView {
            side,
            show_mixer_toggle: true,
            mixer_open: drawer.mixer,
            confirm: drawer.confirm,
            screenshot_prompt: drawer.screenshot_prompt,
            note: drawer.note.as_deref(),
        },
        palette,
    );
    let foot = row![
        container(notifications(dock, palette)).width(half.floor()),
        container(control).width(half.ceil()),
    ]
    .spacing(12)
    .height(Length::Shrink);
    content = content.push(space().height(Length::Fill)).push(foot);
    // The bar keeps to its edge of the screen; the corners away from it
    // are the round ones.
    let (left, right) = match side {
        Side::Left => (4.0, 16.0),
        Side::Right => (16.0, 4.0),
    };
    let content = container(content).padding(Padding {
        top: 16.0,
        right,
        bottom: 16.0,
        left,
    });
    let (inside, radius) = match side {
        Side::Left => (
            row![bar, content],
            border::Radius::default().top_right(18).bottom_right(18),
        ),
        Side::Right => (
            row![content, bar],
            border::Radius::default().top_left(18).bottom_left(18),
        ),
    };

    container(inside)
        .width(drawer.width)
        .height(Length::Fill)
        .style(move |_| container::Style {
            border: Border {
                color: palette.rim,
                width: 1.0,
                radius,
            },
            shadow: palette.shadow(),
            ..style::fill(palette.glass, 0.0)
        })
        .into()
}

fn search<'a>(drawer: &'a Drawer, palette: Palette) -> Element<'a, Message> {
    let field = text_input("Rechercher une appli ou un jeu", &drawer.query)
        .id(SEARCH)
        .on_input(Message::Query)
        .on_submit(Message::Submit)
        .size(15)
        .padding(0)
        .style(move |_, _| text_input::Style {
            background: Color::TRANSPARENT.into(),
            border: Border::default(),
            icon: palette.muted,
            placeholder: palette.muted,
            value: palette.text,
            selection: style::alpha(palette.accent, 0.35),
        });
    let line = row![icons::glyph(Glyph::Search, 18.0, palette.muted), field]
        .spacing(12)
        .align_y(Alignment::Center);
    container(line)
        .height(drawer::SEARCH_HEIGHT)
        .align_y(Alignment::Center)
        .padding(Padding::from([0.0, 16.0]))
        .style(move |_| container::Style {
            border: Border {
                color: palette.accent,
                width: 1.5,
                radius: 12.0.into(),
            },
            ..style::fill(palette.card, 12.0)
        })
        .into()
}

/// The pinned applications, each with the place it opens in, in a column
/// `width` wide.
fn pinned<'a>(dock: &'a Dock, width: f32, palette: Palette) -> Element<'a, Message> {
    // The room a name has under its icon, whatever the width of the column.
    let tile = (width - 20.0) / PINNED as f32 - 4.0;
    let mut tiles: Row<'_, Message> = row![].spacing(4);
    let mut shown = Vec::new();
    for name in &dock.config.pinned {
        let identity = dock.identify(name);
        if shown.contains(&identity.key) || shown.len() == PINNED {
            continue;
        }
        shown.push(identity.key.clone());
        let place = drawer::place(&identity, &dock.snapshot.habits);
        let floating = place == drawer::Place::Floating;
        let color = if floating {
            palette.muted
        } else {
            palette.accent
        };
        let pill = container(
            text(place.label(dock.snapshot.screens.len() == 1))
                .size(10)
                .font(style::SEMIBOLD)
                .color(color),
        )
        .padding(Padding::from([2.0, 7.0]))
        .style(move |_| style::fill(style::alpha(color, 0.14), 6.0));
        let name = text(identity.name.clone())
            .size(12)
            .font(style::MEDIUM)
            .color(palette.text)
            .wrapping(Wrapping::None);
        let name = container(name).max_width(tile - 6.0).height(16).clip(true);
        let content = column![icons::app(&dock.icons, &identity, 42.0), name, pill]
            .spacing(6)
            .align_x(Alignment::Center);
        let tile = button(container(content).center_x(Length::Fill))
            .width(Length::FillPortion(1))
            .padding(Padding::from([10.0, 2.0]))
            .on_press(Message::Open(Target::App(identity.key)))
            .style(style::flat(palette.hover, 12.0));
        tiles = tiles.push(tile);
    }
    let title = text("Épinglées")
        .size(15)
        .font(style::SEMIBOLD)
        .color(palette.text);
    column![title, tiles].spacing(8).into()
}

/// One cell of a grid: an icon and a name on one line.
fn cell<'a>(
    dock: &'a Dock,
    target: Target,
    identity: &Identity,
    selected: bool,
    height: f32,
    palette: Palette,
) -> Element<'a, Message> {
    // Two lines at most, as in the design; a longer name is cut.
    let name = text(identity.name.clone())
        .size(12)
        .font(style::MEDIUM)
        .color(palette.text);
    let content = row![
        icons::app(&dock.icons, identity, 26.0),
        container(name)
            .width(Length::Fill)
            .max_height(30)
            .clip(true)
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let idle = if selected {
        palette.hover
    } else {
        Color::TRANSPARENT
    };
    button(container(content).center_y(Length::Fill))
        .width(Length::FillPortion(1))
        .height(height)
        .padding(Padding::from([0.0, 5.0]))
        .on_press(Message::Open(target))
        .style(style::tinted(idle, palette.hover, 10.0))
        .into()
}

/// Cells in rows of `columns`; the last row is filled with blanks so that
/// the cells keep their width.
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

fn applications<'a>(dock: &'a Dock, drawer: &'a Drawer, palette: Palette) -> Element<'a, Message> {
    let count = dock.apps.len();
    let pinned = dock.pinned_in_drawer();
    let size = drawer.page_size(pinned);
    let pages = drawer::page_count(count, size);
    let page = drawer.page.min(pages - 1);

    let arrow = |glyph: Glyph, forward: bool, enabled: bool| {
        let color = if enabled {
            palette.text
        } else {
            style::alpha(palette.muted, 0.5)
        };
        let arrow = button(container(icons::glyph(glyph, 14.0, color)).center(Length::Fill))
            .width(28)
            .height(28)
            .padding(0)
            .style(style::flat(palette.hover, 8.0));
        if enabled {
            arrow.on_press(Message::TurnPage(forward))
        } else {
            arrow
        }
    };
    let mut pager: Row<'_, Message> = row![arrow(Glyph::ChevronLeft, false, page > 0)].spacing(4);
    for number in drawer::page_buttons(page, pages) {
        let current = number == page;
        let color = if current {
            palette.on_accent()
        } else {
            palette.text
        };
        let idle = if current {
            palette.accent
        } else {
            Color::TRANSPARENT
        };
        let hover = if current {
            palette.accent
        } else {
            palette.hover
        };
        let label = text((number + 1).to_string())
            .size(13)
            .font(style::SEMIBOLD)
            .color(color);
        pager = pager.push(
            button(container(label).center(Length::Fill))
                .width(28)
                .height(28)
                .padding(0)
                .on_press(Message::Page(number))
                .style(style::tinted(idle, hover, 8.0)),
        );
    }
    pager = pager.push(arrow(Glyph::ChevronRight, true, page + 1 < pages));

    let header = heading("Applications", drawer::summary(count, page, size), palette)
        .push(space().width(Length::Fill))
        .push(pager);
    let header = container(header).center_y(drawer::HEADING_HEIGHT);
    let columns = drawer.app_columns(pinned);
    let layout = drawer.lying_layout(pinned);
    let rows = layout.map_or(drawer::ROWS, |layout| layout.app_rows);
    let height = drawer::APP_ROW;
    let cells = drawer::page(&dock.apps, page, size)
        .iter()
        .enumerate()
        .map(|(index, app)| {
            let identity = dock.identify(&app.id);
            let selected = drawer.selected == Some(index);
            cell(
                dock,
                Target::App(app.id.clone()),
                &identity,
                selected,
                height,
                palette,
            )
        });
    // Always as high as a full page, so that nothing jumps on the last one.
    let full = rows as f32 * height + (rows - 1) as f32 * drawer::APP_GAP;
    let rows = container(grid(cells.collect(), columns, drawer::APP_GAP)).height(full);
    let body = mouse_area(rows).on_scroll(|delta| {
        let down = match delta {
            mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y < 0.0,
        };
        Message::TurnPage(down)
    });
    column![header, body].spacing(drawer::HEADING_GAP).into()
}

fn results<'a>(dock: &'a Dock, drawer: &'a Drawer, palette: Palette) -> Element<'a, Message> {
    let hits = dock.hits(&drawer.query);
    let detail = match hits.len() {
        0 => "rien ne correspond".to_owned(),
        1 => "1 résultat · Entrée pour ouvrir".to_owned(),
        count => format!("{count} résultats · Entrée pour ouvrir"),
    };
    // Without an arrow key, Enter opens the first one: show it.
    let selected = drawer.selected.unwrap_or(0);
    let columns = drawer.result_columns();
    let cells = hits
        .into_iter()
        .take(drawer.results_shown())
        .enumerate()
        .map(|(index, (target, identity))| {
            cell(
                dock,
                target,
                &identity,
                index == selected,
                drawer::APP_ROW,
                palette,
            )
        });
    column![
        container(heading("Résultats", detail, palette)).center_y(drawer::HEADING_HEIGHT),
        grid(cells.collect(), columns, drawer::APP_GAP)
    ]
    .spacing(drawer::HEADING_GAP)
    .into()
}

/// The picture of a game with its play mark, as wide as its place.
fn cover<'a>(game: &crate::steam::Game, height: f32, palette: Palette) -> Element<'a, Message> {
    let picture: Element<'_, Message> = match &game.cover {
        Some(path) => image(image::Handle::from_path(path))
            .width(Length::Fill)
            .height(height)
            .content_fit(ContentFit::Cover)
            .border_radius(8)
            .into(),
        None => {
            let color = icons::tile_color(&game.name);
            container(space())
                .width(Length::Fill)
                .height(height)
                .style(move |_| style::fill(color, 8.0))
                .into()
        }
    };
    let play = container(icons::glyph(Glyph::Play, 12.0, palette.text))
        .center(18)
        .style(move |_| style::fill(palette.glass_strong, 9.0));
    let play = container(play)
        .width(Length::Fill)
        .height(height)
        .padding(5)
        .align_x(Alignment::End)
        .align_y(Alignment::End);
    stack![picture, play].into()
}

/// A game: its picture, its name under it.
fn game_card<'a>(game: &crate::steam::Game, height: f32, palette: Palette) -> Element<'a, Message> {
    let name = text(game.name.clone())
        .size(12)
        .font(style::SEMIBOLD)
        .color(palette.text)
        .wrapping(Wrapping::None);
    let name = container(name).width(Length::Fill).height(16).clip(true);
    button(column![cover(game, height, palette), name].spacing(6))
        .width(Length::FillPortion(1))
        .padding(4)
        .on_press(Message::Open(Target::Game(game.id)))
        .style(style::flat(palette.hover, 10.0))
        .into()
}

/// The games of the lying launcher: a column of the ones played last, in
/// as many columns of cards as its width holds.
fn games_column<'a>(
    dock: &'a Dock,
    layout: &drawer::LyingLayout,
    palette: Palette,
) -> Element<'a, Message> {
    let count = dock.games.len();
    let title = text("Jeux")
        .size(15)
        .font(style::SEMIBOLD)
        .color(palette.text);
    // On the line of the title of the applications.
    let title = container(title).center_y(drawer::HEADING_HEIGHT);
    if count == 0 {
        let nothing = text("Aucun jeu Steam installé.")
            .size(12)
            .color(palette.muted);
        return column![title, nothing].spacing(drawer::HEADING_GAP).into();
    }
    let shown = dock
        .games
        .iter()
        .take(layout.games_shown())
        .map(|game| game_card(game, 62.0, palette));
    let library = text(format!("Toute la ludothèque · {count} ›"))
        .size(12)
        .font(style::MEDIUM)
        .color(palette.text);
    let library = button(container(library).center_y(Length::Fill))
        .height(drawer::LIBRARY_HEIGHT)
        .padding(Padding::from([0.0, 10.0]))
        .on_press(Message::Library)
        .style(style::tinted(palette.card, palette.hover, 8.0));
    let cards = grid(shown.collect(), layout.game_columns, drawer::GAME_GAP);
    let cards = column![cards, library].spacing(drawer::GAME_GAP);
    column![title, cards].spacing(drawer::HEADING_GAP).into()
}

fn games<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
    let count = dock.games.len();
    let detail = if count == 0 {
        "aucun jeu Steam installé".to_owned()
    } else if dock.snapshot.screens.len() == 1 {
        "s'ouvrent en plein écran, ancrés".to_owned()
    } else {
        "s'ouvrent au centre, ancrés".to_owned()
    };
    let mut header = heading("Jeux", detail, palette).push(space().width(Length::Fill));
    if count > 0 {
        let label = text(format!("Toute la ludothèque · {count} ›"))
            .size(12)
            .font(style::MEDIUM)
            .color(palette.text);
        header = header.push(
            button(label)
                .padding(Padding::from([5.0, 10.0]))
                .on_press(Message::Library)
                .style(style::tinted(palette.card, palette.hover, 8.0)),
        );
    }

    let mut cards: Row<'_, Message> = row![].spacing(10);
    for game in dock.games.iter().take(GAMES) {
        cards = cards.push(game_card(game, 58.0, palette));
    }
    for _ in count.min(GAMES)..GAMES {
        cards = cards.push(space().width(Length::FillPortion(1)));
    }
    column![header, cards].spacing(8).into()
}

/// The launcher above a lying bar: a drawer as wide as the screen, rising
/// from its bottom edge, the bar drawn again in its foot. Search on top,
/// then four columns: applications, games, notifications, control center.
fn lying<'a>(dock: &'a Dock, drawer: &'a Drawer, palette: Palette) -> Element<'a, Message> {
    let shown = drawer.reveal.at(dock.moment);
    let scrim = scrim(palette, shown);

    let pinned = dock.pinned_in_drawer();
    let layout = drawer
        .lying_layout(pinned)
        .unwrap_or_else(|| drawer::lying_layout(1920.0, 1080.0, drawer.height, pinned));
    let bar_room = dock.metrics().exclusive_zone_for(Orientation::Horizontal) as f32;

    let mut columns: Vec<Element<'_, Message>> = Vec::new();
    if drawer.query.trim().is_empty() {
        let mut apps: Column<'_, Message> = column![].spacing(drawer::PINNED_GAP);
        if pinned {
            let pinned = container(self::pinned(dock, layout.apps, palette));
            apps = apps.push(pinned.height(drawer::PINNED_HEIGHT));
        }
        apps = apps.push(applications(dock, drawer, palette));
        columns.push(apps.width(layout.apps).into());
        if let Some(games) = layout.games {
            let column = games_column(dock, &layout, palette);
            columns.push(container(column).width(games).into());
        }
    } else {
        // The results take the place of the applications and the games.
        let wide = layout.results();
        columns.push(container(results(dock, drawer, palette)).width(wide).into());
    }
    columns.push(
        container(notifications(dock, palette))
            .width(layout.notices)
            .into(),
    );
    let control = column![
        control_center(
            dock,
            ControlView {
                side: Side::Left,
                show_mixer_toggle: false,
                mixer_open: drawer.mixer,
                confirm: drawer.confirm,
                screenshot_prompt: drawer.screenshot_prompt,
                note: drawer.note.as_deref(),
            },
            palette,
        ),
        mixer_card(dock, palette)
    ]
    .spacing(drawer::LYING_GAP);
    columns.push(container(control).width(layout.control).into());
    // As high as the launcher lets it: the notifications fill that height,
    // the control center keeps its own, the mixer under it the rest.
    let body = Row::with_children(columns)
        .spacing(drawer::LYING_GAP)
        .height(layout.body);
    let content = column![search(drawer, palette), body]
        .spacing(drawer::SEARCH_GAP)
        .width(layout.row);
    let content = container(content)
        .width(Length::Fill)
        .height(drawer.height)
        .padding(Padding {
            top: drawer::LYING_PADDING,
            bottom: drawer::LYING_FOOT,
            ..Padding::ZERO
        })
        .center_x(Length::Fill);

    // The bar is drawn again in the foot of the drawer, as it covers it.
    let bar: Element<'_, Message> = match dock.bar_on(drawer.screen) {
        Some(state) => super::bar::still(dock, state, palette),
        None => space().height(bar_room).into(),
    };
    let panel = container(column![content, bar])
        .width(Length::Fill)
        .clip(true)
        .style(move |_| container::Style {
            border: Border {
                color: palette.rim,
                width: 1.0,
                radius: border::Radius::default().top_left(18).top_right(18),
            },
            shadow: palette.shadow(),
            ..style::fill(palette.glass, 0.0)
        });

    // Its top edge is dragged to make it higher or lower.
    let color = if drawer.resizing {
        palette.accent
    } else {
        palette.rim
    };
    let handle = container(space().width(44).height(4)).style(move |_| style::fill(color, 2.0));
    let edge = grip(
        container(handle).center_x(Length::Fill).center_y(EDGE),
        drawer.resizing,
        Message::ResizeStart,
        Message::ResizeTo,
        Message::ResizeEnd,
        mouse::Interaction::ResizingVertically,
    );

    let front = column![space().height(Length::Fill), edge, opaque(panel)];
    if shown >= 1.0 {
        return stack![scrim, front].into();
    }
    // On its way, it rises from behind its bar, which stays.
    let travel = (drawer.height + EDGE) * (1.0 - shown);
    let front = shift(front, Vector::new(0.0, travel)).cut(Padding::ZERO.bottom(bar_room));
    let under = column![
        space().height(Length::Fill),
        container(bar_copy(dock, drawer, palette, shown)).height(bar_room)
    ];
    stack![scrim, under, front].into()
}
