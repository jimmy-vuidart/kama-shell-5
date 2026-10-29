//! The launcher: search, pinned applications, every application by pages,
//! games, the notifications, and the control center with its mixer. Beside
//! an upright bar it is a drawer as high as the screen; above a lying bar, a
//! drawer as wide as the screen.

use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Row, button, column, container, image, mouse_area, pick_list, progress_bar, row,
    scrollable, slider, space, stack, text, text_input,
};
use iced::{
    Alignment, Background, Border, Color, ContentFit, Element, Length, Padding, Vector, border,
    mouse,
};

use super::grip::grip;
use super::icons::{self, Glyph};
use super::opaque::opaque;
use super::shift::shift;
use super::style::{self, Palette};
use crate::app::{Dock, Drawer, Message, OutputChoice, SEARCH, Target, Toggle};
use crate::backend::{SessionAction, Switch};
use crate::model::drawer;
use crate::model::mixer;
use crate::model::notify::{self, Notice};
use crate::model::{Identity, Orientation, Side};
use coquille_ipc::ScreenshotTarget;

const MIXER: f32 = 292.0;
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
    let foot = row![
        container(notifications(dock, palette)).width(half.floor()),
        container(control_center(dock, drawer, palette)).width(half.ceil()),
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

fn rule<'a>(palette: Palette) -> Element<'a, Message> {
    container(space().width(Length::Fill).height(1))
        .style(move |_| style::fill(palette.separator, 0.0))
        .into()
}

fn heading<'a>(title: &'a str, detail: String, palette: Palette) -> Row<'a, Message> {
    row![
        text(title)
            .size(15)
            .font(style::SEMIBOLD)
            .color(palette.text),
        text(detail).size(13).color(palette.muted)
    ]
    .spacing(10)
    .align_y(Alignment::Center)
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

/// A slider in the colors of the dock.
fn level<'a>(
    value: f32,
    palette: Palette,
    on_change: impl Fn(f32) -> Message + Clone + 'a,
) -> Element<'a, Message> {
    let scrolled = on_change.clone();
    let knob_bg = if palette.dark {
        palette.glass_strong
    } else {
        Color::WHITE
    };
    let slider = slider(0.0..=1.0, value, on_change)
        .step(0.01_f32)
        .style(move |_, _| slider::Style {
            rail: slider::Rail {
                backgrounds: (palette.accent.into(), palette.separator.into()),
                width: 4.0,
                border: Border {
                    radius: 2.0.into(),
                    ..Border::default()
                },
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle { radius: 7.0 },
                background: knob_bg.into(),
                border_width: 1.0,
                border_color: palette.rim,
            },
        });
    // The wheel over a slider moves it.
    mouse_area(slider)
        .on_scroll(move |delta| {
            let notches = match delta {
                mouse::ScrollDelta::Lines { y, .. } => y,
                mouse::ScrollDelta::Pixels { y, .. } => y / 15.0,
            };
            scrolled(mixer::scrolled(value, notches))
        })
        .into()
}

fn switch<'a>(
    glyph: Glyph,
    name: &'a str,
    detail: String,
    on: bool,
    available: bool,
    toggle: Toggle,
    palette: Palette,
) -> Element<'a, Message> {
    let (idle, color) = match (on, available) {
        (true, true) => (palette.accent, palette.on_accent()),
        (_, false) => (style::alpha(palette.card, 0.5), palette.muted),
        (false, true) => (palette.glass_strong, palette.text),
    };
    let hover = style::mix(idle, palette.text, 0.08);
    let border = if on && available {
        Border {
            radius: 10.0.into(),
            ..Border::default()
        }
    } else {
        Border {
            color: palette.rim,
            width: 1.0,
            radius: 10.0.into(),
        }
    };
    // Long names take two lines: at a readable size they do not fit on one.
    let name = text(name)
        .size(11)
        .font(style::SEMIBOLD)
        .color(color)
        .line_height(1.15);
    let detail = text(detail)
        .size(10)
        .color(style::alpha(color, 0.8))
        .wrapping(Wrapping::None);
    let texts = column![name, container(detail).width(Length::Fill).clip(true)]
        .spacing(1)
        .width(Length::Fill);
    let content = row![icons::glyph(glyph, 15.0, color), texts]
        .spacing(7)
        .align_y(Alignment::Center);
    let tile = button(container(content).center_y(Length::Fill))
        .width(Length::FillPortion(1))
        .height(54)
        .padding(Padding::from([0.0, 9.0]))
        .style(move |_, status| {
            let color = match status {
                button::Status::Hovered | button::Status::Pressed => hover,
                button::Status::Active | button::Status::Disabled => idle,
            };
            button::Style {
                background: Some(Background::Color(color)),
                border,
                ..button::Style::default()
            }
        });
    if available {
        tile.on_press(Message::Toggle(toggle)).into()
    } else {
        tile.into()
    }
}

/// The control center, the same beside an upright bar and above a lying
/// one: switches two by two, the sliders one under the other, then the
/// session.
fn control_center<'a>(
    dock: &'a Dock,
    drawer: &'a Drawer,
    palette: Palette,
) -> Element<'a, Message> {
    let state = |switch: &Switch| {
        let detail = if switch.detail.is_empty() {
            (if switch.on { "Activé" } else { "Désactivé" }).to_owned()
        } else {
            switch.detail.clone()
        };
        (detail, switch.on, switch.available)
    };
    let plain = |on: bool| {
        (
            (if on { "Activé" } else { "Désactivé" }).to_owned(),
            on,
            true,
        )
    };
    let (network, bluetooth) = (state(&dock.network), state(&dock.bluetooth));
    let (quiet, night, dark) = (
        plain(dock.do_not_disturb),
        plain(dock.night_light),
        plain(palette.dark),
    );
    let tile = |glyph, name, (detail, on, available): (String, bool, bool), toggle| {
        switch(glyph, name, detail, on, available, toggle, palette)
    };
    let mut tiles = vec![
        tile(Glyph::Network, "Réseau", network, Toggle::Network),
        tile(Glyph::Bluetooth, "Bluetooth", bluetooth, Toggle::Bluetooth),
        tile(
            Glyph::DoNotDisturb,
            "Ne pas déranger",
            quiet,
            Toggle::DoNotDisturb,
        ),
        tile(Glyph::Moon, "Éclairage nocturne", night, Toggle::NightLight),
        tile(Glyph::Contrast, "Thème sombre", dark, Toggle::DarkTheme),
    ]
    .into_iter();
    let mut switches: Column<'_, Message> = column![].spacing(6);
    loop {
        let mut line: Row<'_, Message> = row![].spacing(6);
        let mut filled = 0;
        for tile in tiles.by_ref().take(SWITCHES_PER_ROW) {
            line = line.push(tile);
            filled += 1;
        }
        if filled == 0 {
            break;
        }
        // A last row that is not full keeps the width of the others.
        for _ in filled..SWITCHES_PER_ROW {
            line = line.push(space().width(Length::FillPortion(1)));
        }
        switches = switches.push(line);
    }

    let speaker = if dock.audio.muted {
        Glyph::Muted
    } else {
        Glyph::Volume
    };
    let speaker_color = if dock.audio.muted {
        style::DANGER
    } else {
        palette.text
    };
    let mute = button(container(icons::glyph(speaker, 18.0, speaker_color)).center(Length::Fill))
        .width(30)
        .height(30)
        .padding(0)
        .on_press(Message::VolumeMute)
        .style(style::flat(palette.hover, 8.0));
    let (mixer_idle, mixer_fg) = if drawer.mixer {
        (palette.accent, palette.on_accent())
    } else {
        (palette.card, palette.text)
    };
    // The mixer opens beside the drawer, away from the bar: the arrow
    // points to it.
    let towards = match drawer.orientation {
        Orientation::Vertical(Side::Right) => Glyph::ChevronLeft,
        Orientation::Vertical(Side::Left) | Orientation::Horizontal => Glyph::ChevronRight,
    };
    let mixer_toggle =
        button(container(icons::glyph(towards, 14.0, mixer_fg)).center(Length::Fill))
            .width(30)
            .height(30)
            .padding(0)
            .on_press(Message::ToggleMixer)
            .style(style::tinted(
                mixer_idle,
                style::mix(mixer_idle, palette.text, 0.08),
                8.0,
            ));
    let screens = dock.snapshot.screens.len();
    let screens_label = match (dock.brightness, screens) {
        (None, _) => "indisponible".to_owned(),
        (Some(_), 1) => "1 écran".to_owned(),
        (Some(_), count) => format!("{count} écrans"),
    };
    let screens_pill = container(
        text(screens_label)
            .size(12)
            .font(style::MEDIUM)
            .color(palette.text)
            .wrapping(Wrapping::None),
    )
    .padding(Padding::from([6.0, 10.0]))
    .style(move |_| style::fill(palette.card, 8.0));
    let brightness: Element<'_, Message> = match dock.brightness {
        Some(value) => level(value, palette, Message::Brightness),
        None => space().width(Length::Fill).into(),
    };
    let mut volume = row![mute, level(dock.audio.volume, palette, Message::Volume)]
        .spacing(8)
        .align_y(Alignment::Center);
    // Above a lying bar, the mixer is always under the control center.
    if let Orientation::Vertical(_) = drawer.orientation {
        volume = volume.push(mixer_toggle);
    }
    let light = row![
        container(icons::glyph(Glyph::Sun, 18.0, palette.text)).center_x(30),
        brightness,
        screens_pill
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let foot = match (drawer.confirm, drawer.screenshot_prompt, &drawer.note) {
        (Some(action), _, _) => confirmation(action, palette),
        (None, true, _) => screenshot_selector(palette),
        (None, false, note) => session(dock, note.as_deref(), palette),
    };
    let parts = column![
        switches,
        column![volume, light].spacing(8),
        rule(palette),
        foot
    ]
    .spacing(12);
    container(parts)
        .padding(12)
        .style(move |_| style::fill(palette.card, 14.0))
        .into()
}

/// The notices still open, newest first, in a card as high as the control
/// center beside it: a click opens one, its cross closes it, « Tout
/// effacer » closes them all.
fn notifications<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
    let count = dock.notify.history.len();
    let detail = if count == 0 {
        String::new()
    } else {
        count.to_string()
    };
    let mut header = heading("Notifications", detail, palette).push(space().width(Length::Fill));
    if count > 0 {
        let clear = button(
            text("Tout effacer")
                .size(12)
                .font(style::MEDIUM)
                .color(palette.muted),
        )
        .padding(Padding::from([4.0, 8.0]))
        .on_press(Message::ClearNotices)
        .style(style::flat(palette.hover, 8.0));
        header = header.push(clear);
    }
    let header = container(header.align_y(Alignment::Center)).height(28);

    let body: Element<'_, Message> = if count == 0 {
        let mut nothing = column![
            icons::glyph(Glyph::Bell, 28.0, style::alpha(palette.muted, 0.7)),
            text("Aucune notification")
                .size(13)
                .font(style::MEDIUM)
                .color(palette.muted),
        ]
        .spacing(8)
        .align_x(Alignment::Center);
        if dock.do_not_disturb {
            nothing = nothing.push(
                text("Ne pas déranger : elles attendent ici.")
                    .size(11)
                    .color(palette.muted)
                    .align_x(Alignment::Center),
            );
        }
        container(nothing).center(Length::Fill).into()
    } else {
        let now = dock.now.timestamp();
        let items = dock
            .notify
            .history
            .iter()
            .map(|notice| notice_card(dock, notice, now, palette));
        scrollable(Column::with_children(items).spacing(6))
            .height(Length::Fill)
            .direction(scrollable::Direction::Vertical(
                scrollable::Scrollbar::new()
                    .width(4)
                    .scroller_width(4)
                    .spacing(4),
            ))
            .into()
    };
    container(column![header, body].spacing(8))
        .height(Length::Fill)
        .padding(12)
        .style(move |_| style::fill(palette.card, 14.0))
        .into()
}

/// One notice: its icon, its summary and a cross; under them the
/// application and when it came, two lines of its body and its actions.
fn notice_card<'a>(
    dock: &'a Dock,
    notice: &'a Notice,
    now: i64,
    palette: Palette,
) -> Element<'a, Message> {
    let id = notice.id;
    let elapsed = now.saturating_sub(notice.received).max(0);
    let title = if notice.summary.is_empty() {
        &notice.app
    } else {
        &notice.summary
    };
    let summary = text(title.as_str())
        .size(13)
        .font(style::SEMIBOLD)
        .color(palette.text)
        .wrapping(Wrapping::None);
    let source = text(format!(
        "{} · {}",
        notice.app,
        relative_time(elapsed as u64)
    ))
    .size(11)
    .color(palette.muted)
    .wrapping(Wrapping::None);
    let mut words: Column<'_, Message> = column![
        container(summary).width(Length::Fill).clip(true),
        container(source).width(Length::Fill).clip(true),
    ]
    .spacing(1);
    let body = notify::plain_text(&notice.body);
    let body = body.trim();
    if !body.is_empty() {
        // Two lines at most; the whole of it is in the application.
        let body = text(body.to_owned())
            .size(12)
            .color(palette.text)
            .line_height(1.3);
        words = words.push(
            container(body)
                .width(Length::Fill)
                .max_height(BODY_LINES * 12.0 * 1.3)
                .clip(true),
        );
    }
    let content = row![
        container(super::toasts::notice_icon(dock, notice, 20.0)).padding(Padding {
            top: 1.0,
            ..Padding::ZERO
        }),
        words
    ]
    .spacing(10);
    let open = button(content)
        .width(Length::Fill)
        .padding(Padding {
            top: 8.0,
            right: 28.0,
            bottom: 8.0,
            left: 10.0,
        })
        .on_press(Message::NoticeOpen(id))
        .style(style::tinted(palette.card, palette.hover, 10.0));
    let close = button(icons::glyph(Glyph::Close, 12.0, palette.muted))
        .padding(5)
        .on_press(Message::NoticeDismiss(id))
        .style(style::flat(palette.hover, 8.0));
    let close = container(close)
        .width(Length::Fill)
        .padding(4)
        .align_x(Alignment::End);
    let mut card: Column<'_, Message> = column![stack![open, close]].spacing(2);

    let actions: Vec<Element<'_, Message>> = notice
        .actions
        .iter()
        .filter(|(key, _)| key != "default")
        .take(ACTIONS)
        .map(|(key, label)| {
            button(
                text(label.as_str())
                    .size(12)
                    .font(style::MEDIUM)
                    .color(palette.accent)
                    .wrapping(Wrapping::None),
            )
            .padding(Padding::from([4.0, 10.0]))
            .on_press(Message::NoticeAction {
                id,
                key: key.clone(),
            })
            .style(style::flat(palette.hover, 8.0))
            .into()
        })
        .collect();
    if !actions.is_empty() {
        card = card.push(
            container(Row::with_children(actions).spacing(4)).padding(Padding {
                left: 36.0,
                ..Padding::ZERO
            }),
        );
    }
    card.into()
}

/// How many lines of its body a notice shows.
const BODY_LINES: f32 = 2.0;
/// How many of its actions a notice shows as buttons.
const ACTIONS: usize = 2;
/// The switches of the control center go two by two.
const SWITCHES_PER_ROW: usize = 2;

/// Format a duration in seconds as a French relative time string.
fn relative_time(secs: u64) -> String {
    let days = secs / 86400;
    match secs {
        0..=59 => "à l'instant".to_owned(),
        60..=3599 => format!("il y a {} min", secs / 60),
        3600..=86399 => format!("il y a {} h", secs / 3600),
        _ if days < 7 => format!("il y a {} j", days),
        _ => "plus tôt".to_owned(),
    }
}

/// The user on a line of their own, then settings and the ways out, as
/// wide as the control center.
fn session<'a>(dock: &'a Dock, note: Option<&'a str>, palette: Palette) -> Element<'a, Message> {
    let initials: String = dock
        .user
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect();
    let avatar_text = palette.on_accent();
    let avatar = container(text(initials).size(13).font(style::BOLD).color(avatar_text))
        .center(36)
        .style(move |_| style::fill(palette.accent, 18.0));
    // A laptop says how its battery is doing there.
    let battery = dock
        .battery
        .map(|battery| format!("Batterie {} · {}", battery.label(), battery.detail()));
    let detail = note
        .map(str::to_owned)
        .or(battery)
        .unwrap_or_else(|| "Compte local".to_owned());
    let who = column![
        text(dock.user.clone())
            .size(14)
            .font(style::SEMIBOLD)
            .color(palette.text)
            .wrapping(Wrapping::None),
        text(detail)
            .size(12)
            .color(palette.muted)
            .wrapping(Wrapping::None)
    ]
    .spacing(1);
    let who = row![avatar, container(who).width(Length::Fill).clip(true)]
        .spacing(10)
        .align_y(Alignment::Center);

    let action = |glyph: Glyph, message: Message, danger: bool| {
        let (idle, color) = if danger {
            (style::alpha(style::DANGER, 0.14), style::DANGER)
        } else {
            (palette.card, palette.text)
        };
        let hover = style::mix(idle, color, 0.12);
        button(container(icons::glyph(glyph, 18.0, color)).center(Length::Fill))
            .width(Length::FillPortion(1))
            .height(34)
            .padding(0)
            .on_press(message)
            .style(style::tinted(idle, hover, 10.0))
    };
    let actions = row![
        action(Glyph::Camera, Message::ScreenshotMenu, false),
        action(Glyph::Sliders, Message::Settings, false),
        action(
            Glyph::Logout,
            Message::Session(SessionAction::Logout),
            false
        ),
        action(Glyph::Moon, Message::Session(SessionAction::Suspend), false),
        action(
            Glyph::Restart,
            Message::Session(SessionAction::Restart),
            false
        ),
        action(
            Glyph::Power,
            Message::Session(SessionAction::PowerOff),
            true
        ),
    ]
    .spacing(6);
    column![who, actions].spacing(10).into()
}

/// Nothing ends the session without a second click: the question goes
/// above its two answers.
fn confirmation<'a>(action: SessionAction, palette: Palette) -> Element<'a, Message> {
    let question = text(action.question())
        .size(14)
        .font(style::SEMIBOLD)
        .color(palette.text);
    let cancel = button(
        text("Annuler")
            .size(13)
            .font(style::MEDIUM)
            .color(palette.text),
    )
    .padding(Padding::from([10.0, 16.0]))
    .on_press(Message::Confirm(false))
    .style(style::tinted(palette.card, palette.hover, 10.0));
    let confirm = button(
        text(action.verb())
            .size(13)
            .font(style::SEMIBOLD)
            .color(Color::WHITE),
    )
    .padding(Padding::from([10.0, 16.0]))
    .on_press(Message::Confirm(true))
    .style(style::tinted(
        style::DANGER,
        style::mix(style::DANGER, Color::BLACK, 0.12),
        10.0,
    ));
    let answers = row![space().width(Length::Fill), cancel, confirm].spacing(10);
    column![question, answers].spacing(8).into()
}

/// The screenshot selector: region or full screen, or cancel.
fn screenshot_selector<'a>(palette: Palette) -> Element<'a, Message> {
    let title = text("Capture d'écran")
        .size(14)
        .font(style::SEMIBOLD)
        .color(palette.text);
    let choice = |glyph: Glyph, label: &'a str, target: ScreenshotTarget| {
        button(
            row![
                icons::glyph(glyph, 16.0, palette.text),
                text(label)
                    .size(13)
                    .font(style::MEDIUM)
                    .color(palette.text)
                    .wrapping(Wrapping::None),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([10.0, 12.0]))
        .on_press(Message::Screenshot(target))
        .style(style::tinted(palette.card, palette.hover, 10.0))
    };
    let cancel = button(
        text("Annuler")
            .size(13)
            .font(style::MEDIUM)
            .color(palette.muted),
    )
    .padding(Padding::from([10.0, 12.0]))
    .on_press(Message::CancelScreenshot)
    .style(style::tinted(palette.card, palette.hover, 10.0));
    let answers = row![
        choice(Glyph::Crop, "Région", ScreenshotTarget::Region),
        choice(Glyph::Screens, "Écran entier", ScreenshotTarget::Screen),
        space().width(Length::Fill),
        cancel
    ]
    .spacing(8);
    column![title, answers].spacing(8).into()
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
        control_center(dock, drawer, palette),
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

fn output_picker<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
    let choices = dock.visible_sinks();
    let current = dock.current_output_choice();
    pick_list(choices, current, |choice: OutputChoice| {
        Message::SelectOutput(choice.name)
    })
    .padding(Padding::from([5.0, 8.0]))
    .text_size(12)
    .font(style::MEDIUM)
    .style(move |_, status| {
        let border_color = match status {
            pick_list::Status::Active => palette.separator,
            pick_list::Status::Hovered | pick_list::Status::Opened { .. } => palette.accent,
        };
        pick_list::Style {
            text_color: palette.text,
            placeholder_color: palette.muted,
            handle_color: palette.muted,
            background: palette.card.into(),
            border: Border {
                color: border_color,
                width: 1.0,
                radius: 8.0.into(),
            },
        }
    })
    .into()
}

/// The title of the mixer, with the choice of the audio output.
fn mixer_header<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
    let title = text("Volume par application")
        .size(14)
        .font(style::SEMIBOLD)
        .color(palette.text);
    row![
        title,
        space().width(Length::Fill),
        output_picker(dock, palette)
    ]
    .align_y(Alignment::Center)
    .into()
}

/// One line per application playing, or a word that none does.
fn mixer_lines<'a>(dock: &'a Dock, palette: Palette) -> Column<'a, Message> {
    let mut lines: Column<'_, Message> = column![].spacing(10);
    let playing = dock.mixer_lines();
    if playing.is_empty() {
        let nothing = text("Aucune application ne joue de son.")
            .size(12)
            .color(palette.muted);
        lines = lines.push(container(nothing).padding(Padding::from([8.0, 4.0])));
    }
    for line in playing {
        let key = line.identity.key.clone();
        let loudest = line
            .streams
            .iter()
            .filter_map(|id| dock.levels.get(id))
            .fold(0.0_f32, |loudest, level| loudest.max(*level));
        let state_color = if line.muted {
            style::DANGER
        } else {
            palette.muted
        };
        let name = text(line.identity.name.clone())
            .size(13)
            .font(style::SEMIBOLD)
            .color(palette.text);
        let state = text(line.state.clone()).size(11).color(state_color);
        let texts = container(column![name, state].spacing(1))
            .width(88)
            .clip(true);

        let volume_key = key.clone();
        let knob = level(line.volume, palette, move |volume| {
            Message::StreamVolume(volume_key.clone(), volume)
        });
        // How loud it is right now, in green under the slider.
        let meter = progress_bar(0.0..=1.0, loudest)
            .girth(2)
            .style(move |_| progress_bar::Style {
                background: Color::TRANSPARENT.into(),
                bar: style::LEVEL.into(),
                border: Border::default(),
            });
        let (glyph, idle, color) = if line.muted {
            (
                Glyph::Muted,
                style::alpha(style::DANGER, 0.14),
                style::DANGER,
            )
        } else {
            (Glyph::Volume, palette.card, palette.text)
        };
        let mute = button(container(icons::glyph(glyph, 14.0, color)).center(Length::Fill))
            .width(28)
            .height(28)
            .padding(0)
            .on_press(Message::StreamMute(key))
            .style(style::tinted(idle, style::mix(idle, color, 0.12), 8.0));
        let line = row![
            icons::app(&dock.icons, &line.identity, 28.0),
            texts,
            column![knob, meter].spacing(1).width(Length::Fill),
            mute
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        lines = lines.push(container(line).padding(Padding::from([0.0, 4.0])));
    }
    lines
}

/// The mixer under the control center of the launcher above a lying bar,
/// in a card like it, down to the foot of the launcher: the volume of each
/// application, the general one being just above.
fn mixer_card<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
    let lines = scrollable(mixer_lines(dock, palette))
        .height(Length::Fill)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(4)
                .scroller_width(4)
                .spacing(4),
        ));
    container(column![mixer_header(dock, palette), lines].spacing(10))
        .height(Length::Fill)
        .padding(12)
        .style(move |_| style::fill(palette.card, 14.0))
        .into()
}

/// The mixer beside an upright bar: the general volume, then one line per
/// application playing.
fn mixer_panel<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
    let percent = (dock.audio.volume * 100.0).round() as u32;
    let general = row![
        icons::glyph(Glyph::Volume, 18.0, palette.text),
        text("Général")
            .size(13)
            .font(style::SEMIBOLD)
            .color(palette.text)
            .width(64),
        level(dock.audio.volume, palette, Message::Volume),
        text(percent.to_string())
            .size(12)
            .color(palette.muted)
            .width(24)
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let general = container(general)
        .padding(Padding::from([10.0, 12.0]))
        .style(move |_| style::fill(palette.card, 10.0));

    let top = column![mixer_header(dock, palette), general].spacing(10);
    let mut lines = column![top, mixer_lines(dock, palette)].spacing(10);

    let note = row![
        icons::glyph(Glyph::Clock, 14.0, palette.muted),
        text("Chaque appli retrouve son volume à la prochaine ouverture. Molette sur une ligne pour régler.")
            .size(11)
            .color(palette.muted)
    ]
    .spacing(10);
    lines = lines.push(rule(palette)).push(note);

    container(lines)
        .width(MIXER)
        .padding(16)
        .style(move |_| container::Style {
            shadow: palette.shadow(),
            ..style::outlined(palette.glass_strong, palette.rim, 16.0)
        })
        .into()
}
