//! The drawer: search, pinned applications, every application by pages,
//! games, and at its foot the control center with its mixer.

use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Row, button, column, container, image, mouse_area, opaque, progress_bar, row, slider,
    space, stack, text, text_input,
};
use iced::{Alignment, Border, Color, ContentFit, Element, Length, Padding, border, mouse};

use super::icons::{self, Glyph};
use super::style::{self, Palette};
use crate::app::{Dock, Drawer, Message, SEARCH, Target, Toggle};
use crate::backend::{SessionAction, Switch};
use crate::model::Identity;
use crate::model::drawer::{self, COLUMNS};
use crate::model::mixer;

const PANEL: f32 = 640.0;
/// The room of the bar, drawn again inside the drawer.
const BAR: f32 = 60.0;
const MIXER: f32 = 292.0;
/// How many games fit in the row.
const GAMES: usize = 5;
/// How many pinned applications fit in the row.
const PINNED: usize = 7;

pub fn view<'a>(dock: &'a Dock, drawer: &'a Drawer) -> Element<'a, Message> {
    let palette = dock.palette();
    // What is left of the screen is dimmed, and a click there closes.
    let scrim = container(space())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| style::fill(palette.scrim, 0.0));
    let scrim = mouse_area(scrim).on_press(Message::CloseDrawer);

    let mut front: Row<'_, Message> = row![opaque(panel(dock, drawer, palette))].spacing(14);
    if drawer.mixer {
        // Next to the button that opens it, at the foot of the drawer.
        let aligned = column![
            space().height(Length::Fill),
            opaque(mixer_panel(dock, palette)),
            space().height(28)
        ];
        front = front.push(aligned);
    }
    stack![scrim, front].into()
}

fn panel<'a>(dock: &'a Dock, drawer: &'a Drawer, palette: Palette) -> Element<'a, Message> {
    let bar = container(super::bar::bar(dock, 0.0, false, palette))
        .width(BAR)
        .center_y(Length::Fill);

    let mut content: Column<'_, Message> = column![search(drawer, palette)].spacing(12);
    if drawer.query.trim().is_empty() {
        content = content
            .push(pinned(dock, palette))
            .push(rule(palette))
            .push(applications(dock, drawer, palette))
            .push(rule(palette))
            .push(games(dock, palette));
    } else {
        content = content.push(results(dock, drawer, palette));
    }
    content = content
        .push(space().height(Length::Fill))
        .push(control_center(dock, drawer, palette));
    let content = container(content).padding(Padding {
        top: 16.0,
        right: 16.0,
        bottom: 16.0,
        left: 4.0,
    });

    container(row![bar, content])
        .width(PANEL)
        .height(Length::Fill)
        .style(move |_| container::Style {
            border: Border {
                radius: border::Radius::default().top_right(18).bottom_right(18),
                ..Border::default()
            },
            shadow: palette.shadow(),
            ..style::fill(palette.panel, 0.0)
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
        .padding(Padding::from([12.0, 16.0]))
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

/// The pinned applications, each with the place it opens in.
fn pinned<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
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
            text(place.label())
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
        let name = container(name).max_width(76).height(16).clip(true);
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
        .height(44)
        .padding(Padding::from([0.0, 5.0]))
        .on_press(Message::Open(target))
        .style(style::tinted(idle, palette.hover, 10.0))
        .into()
}

/// Cells in rows of six; the last row is filled with blanks so that the
/// cells keep their width.
fn grid<'a>(cells: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut rows: Column<'_, Message> = column![].spacing(4);
    let mut cells = cells.into_iter().peekable();
    while cells.peek().is_some() {
        let mut line: Row<'_, Message> = row![].spacing(4);
        for _ in 0..COLUMNS {
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
    let pages = drawer::page_count(count);
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
    for number in 0..pages {
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

    let header = heading("Applications", drawer::summary(count, page), palette)
        .push(space().width(Length::Fill))
        .push(pager);
    let cells = drawer::page(&dock.apps, page)
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
                palette,
            )
        });
    // Always three rows high, so that the games do not jump on the last
    // page.
    let rows = container(grid(cells.collect())).height(3 * 44 + 2 * 4);
    let body = mouse_area(rows).on_scroll(|delta| {
        let down = match delta {
            mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y < 0.0,
        };
        Message::TurnPage(down)
    });
    column![header, body].spacing(8).into()
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
    let cells =
        hits.into_iter()
            .take(4 * COLUMNS)
            .enumerate()
            .map(|(index, (target, identity))| {
                cell(dock, target, &identity, index == selected, palette)
            });
    column![heading("Résultats", detail, palette), grid(cells.collect())]
        .spacing(8)
        .into()
}

fn games<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
    let count = dock.games.len();
    let detail = if count == 0 {
        "aucun jeu Steam installé".to_owned()
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
        let cover: Element<'_, Message> = match &game.cover {
            Some(path) => image(image::Handle::from_path(path))
                .width(Length::Fill)
                .height(58)
                .content_fit(ContentFit::Cover)
                .border_radius(8)
                .into(),
            None => {
                let color = icons::tile_color(&game.name);
                container(space())
                    .width(Length::Fill)
                    .height(58)
                    .style(move |_| style::fill(color, 8.0))
                    .into()
            }
        };
        let play = container(icons::glyph(
            Glyph::Play,
            12.0,
            Color::from_rgb8(0x1c, 0x22, 0x30),
        ))
        .center(18)
        .style(|_| style::fill(Color::WHITE, 9.0));
        let play = container(play)
            .width(Length::Fill)
            .height(58)
            .padding(5)
            .align_x(Alignment::End)
            .align_y(Alignment::End);
        let name = text(game.name.clone())
            .size(12)
            .font(style::SEMIBOLD)
            .color(palette.text)
            .wrapping(Wrapping::None);
        let name = container(name).width(Length::Fill).height(16).clip(true);
        let content = column![stack![cover, play], name].spacing(6);
        cards = cards.push(
            button(content)
                .width(Length::FillPortion(1))
                .padding(4)
                .on_press(Message::Open(Target::Game(game.id)))
                .style(style::flat(palette.hover, 10.0)),
        );
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
    let slider = slider(0.0..=1.0, value, on_change)
        .step(0.01_f32)
        .style(move |_, _| slider::Style {
            rail: slider::Rail {
                backgrounds: (
                    palette.accent.into(),
                    style::alpha(palette.muted, 0.3).into(),
                ),
                width: 4.0,
                border: Border {
                    radius: 2.0.into(),
                    ..Border::default()
                },
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle { radius: 7.0 },
                background: Color::WHITE.into(),
                border_width: 1.0,
                border_color: style::alpha(palette.muted, 0.6),
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
        (false, true) => (palette.card, palette.text),
    };
    let hover = style::mix(idle, palette.text, 0.08);
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
        .style(style::tinted(idle, hover, 10.0));
    if available {
        tile.on_press(Message::Toggle(toggle)).into()
    } else {
        tile.into()
    }
}

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
    let switches = row![
        tile(Glyph::Network, "Réseau", network, Toggle::Network),
        tile(Glyph::Bluetooth, "Bluetooth", bluetooth, Toggle::Bluetooth),
        tile(
            Glyph::DoNotDisturb,
            "Ne pas déranger",
            quiet,
            Toggle::DoNotDisturb
        ),
        tile(Glyph::Moon, "Éclairage nocturne", night, Toggle::NightLight),
        tile(Glyph::Contrast, "Thème sombre", dark, Toggle::DarkTheme)
    ]
    .spacing(6);

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
    let per_app_color = palette.on_accent();
    let per_app = button(
        text("Par appli ›")
            .size(12)
            .font(style::SEMIBOLD)
            .color(per_app_color),
    )
    .padding(Padding::from([6.0, 10.0]))
    .on_press(Message::ToggleMixer)
    .style(style::tinted(
        palette.accent,
        style::mix(palette.accent, palette.text, 0.1),
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
            .color(palette.text),
    )
    .padding(Padding::from([6.0, 10.0]))
    .style(move |_| style::fill(palette.card, 8.0));
    let brightness: Element<'_, Message> = match dock.brightness {
        Some(value) => level(value, palette, Message::Brightness),
        None => space().width(Length::Fill).into(),
    };
    let sliders = row![
        mute,
        level(dock.audio.volume, palette, Message::Volume),
        per_app,
        space().width(6),
        icons::glyph(Glyph::Sun, 18.0, palette.text),
        brightness,
        screens_pill
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let foot = match (drawer.confirm, &drawer.note) {
        (Some(action), _) => confirmation(action, palette),
        (None, note) => session(dock, note.as_deref(), palette),
    };
    container(column![switches, sliders, rule(palette), foot].spacing(12))
        .padding(12)
        .style(move |_| style::fill(palette.block, 14.0))
        .into()
}

/// The user on the left; settings and the three ways out on the right.
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
    let detail = note.unwrap_or("Compte local");
    let who = column![
        text(dock.user.clone())
            .size(14)
            .font(style::SEMIBOLD)
            .color(palette.text),
        text(detail).size(12).color(palette.muted)
    ]
    .spacing(1);

    let action = |glyph: Glyph, message: Message, danger: bool| {
        let (idle, color) = if danger {
            (style::alpha(style::DANGER, 0.14), style::DANGER)
        } else {
            (palette.card, palette.text)
        };
        let hover = style::mix(idle, color, 0.12);
        button(container(icons::glyph(glyph, 18.0, color)).center(Length::Fill))
            .width(40)
            .height(40)
            .padding(0)
            .on_press(message)
            .style(style::tinted(idle, hover, 10.0))
    };
    row![
        avatar,
        who,
        space().width(Length::Fill),
        action(Glyph::Sliders, Message::Settings, false),
        action(
            Glyph::Logout,
            Message::Session(SessionAction::Logout),
            false
        ),
        action(
            Glyph::Restart,
            Message::Session(SessionAction::Restart),
            false
        ),
        action(
            Glyph::Power,
            Message::Session(SessionAction::PowerOff),
            true
        )
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// Nothing ends the session without a second click.
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
    row![question, space().width(Length::Fill), cancel, confirm]
        .spacing(10)
        .height(40)
        .align_y(Alignment::Center)
        .into()
}

/// The mixer: the general volume, then one line per application playing.
fn mixer_panel<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
    let output = if dock.audio.output.is_empty() {
        "Sortie audio".to_owned()
    } else {
        dock.audio.output.clone()
    };
    let output = row![
        icons::glyph(Glyph::Headphones, 14.0, palette.text),
        container(
            text(output)
                .size(12)
                .font(style::MEDIUM)
                .color(palette.text)
                .wrapping(Wrapping::None)
        )
        .max_width(96)
        .height(16)
        .clip(true)
    ]
    .spacing(6)
    .align_y(Alignment::Center);
    let output = container(output)
        .padding(Padding::from([6.0, 10.0]))
        .style(move |_| style::fill(palette.block, 8.0));
    let title = text("Volume par application")
        .size(14)
        .font(style::SEMIBOLD)
        .color(palette.text);
    let header = row![title, space().width(Length::Fill), output].align_y(Alignment::Center);

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
        .style(move |_| style::fill(palette.block, 10.0));

    let mut lines: Column<'_, Message> = column![header, general].spacing(10);
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
            (Glyph::Volume, palette.block, palette.text)
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
            ..style::fill(
                Color {
                    a: 1.0,
                    ..palette.panel
                },
                16.0,
            )
        })
        .into()
}
