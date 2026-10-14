//! The menu opened by a right click on an application of the bar.

use coquille_model::OutputRole;
use iced::widget::{Column, button, column, container, row, space, text};
use iced::{Alignment, Color, Element, Length, Padding};

use super::icons::{self, Glyph};
use super::style::{self, Palette};
use crate::app::{Dock, Message};
use crate::model::bar::{self, Entry, Menu, MenuAction};

pub const WIDTH: u32 = 248;
const PADDING: f32 = 10.0;
const HEADER: f32 = 40.0;
const TITLE: f32 = 26.0;
const TILES: f32 = 58.0;
const ITEM: f32 = 36.0;
const RULE: f32 = 9.0;
const GAP: f32 = 4.0;

/// Whether a line of the menu comes after a hairline: forgetting and
/// closing each stand apart.
fn after_rule(action: &MenuAction) -> bool {
    matches!(action, MenuAction::Forget | MenuAction::Close(_))
}

fn lines(menu: &Menu) -> impl Iterator<Item = &MenuAction> {
    menu.actions
        .iter()
        .filter(|action| !matches!(action, MenuAction::OpenOn(_)))
}

/// The height of the popup for this menu.
pub fn height(menu: &Menu) -> u32 {
    let mut height = 2.0 * PADDING + HEADER + GAP + TITLE + TILES + GAP;
    for action in lines(menu) {
        height += ITEM + GAP;
        if after_rule(action) {
            height += RULE;
        }
    }
    height.ceil() as u32
}

pub fn view<'a>(dock: &'a Dock, entry: &'a Entry) -> Element<'a, Message> {
    let palette = dock.palette();
    let menu = bar::menu(entry);

    let title = text(menu.title.clone())
        .size(14)
        .font(style::SEMIBOLD)
        .color(palette.text);
    let subtitle = text(menu.subtitle.clone()).size(12).color(palette.muted);
    let header = row![
        icons::app(&dock.icons, &entry.identity, 28.0),
        column![title, subtitle].spacing(1)
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .height(HEADER);

    let open_on = container(
        text("OUVRIR SUR")
            .size(10)
            .font(style::SEMIBOLD)
            .color(palette.muted),
    )
    .height(TITLE)
    .align_y(Alignment::End)
    .padding(Padding::from([4.0, 2.0]));
    let tile = |screen: OutputRole| screen_tile(screen, menu.current == Some(screen), palette);
    let tiles = row![
        tile(OutputRole::Left),
        tile(OutputRole::Center),
        tile(OutputRole::Right)
    ]
    .spacing(6)
    .height(TILES);

    let mut content: Column<'_, Message> = column![
        container(header).padding(Padding::from([0.0, 4.0])),
        open_on,
        tiles
    ]
    .spacing(GAP);
    for action in lines(&menu) {
        if after_rule(action) {
            let rule = container(space().width(Length::Fill).height(1))
                .style(move |_| style::fill(palette.separator, 0.0));
            content = content.push(container(rule).height(RULE).center_y(RULE));
        }
        content = content.push(item(action.clone(), palette));
    }

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(PADDING)
        .style(move |_| {
            style::outlined(
                Color {
                    a: 1.0,
                    ..palette.bar
                },
                palette.bar_border,
                14.0,
            )
        })
        .into()
}

/// One of the three screens an application can be opened on: a small
/// picture of the three screens, with that one lit.
fn screen_tile<'a>(screen: OutputRole, current: bool, palette: Palette) -> Element<'a, Message> {
    let label = match screen {
        OutputRole::Left => "Gauche",
        OutputRole::Center => "Centre",
        OutputRole::Right => "Droite",
    };
    let mini = |role: OutputRole| {
        let lit = role == screen;
        let color = if lit {
            palette.accent
        } else {
            style::alpha(palette.muted, 0.45)
        };
        let (width, height) = if role == OutputRole::Center {
            (13, 12)
        } else {
            (9, 9)
        };
        container(space().width(width).height(height)).style(move |_| style::fill(color, 2.0))
    };
    let screens = row![
        mini(OutputRole::Left),
        mini(OutputRole::Center),
        mini(OutputRole::Right)
    ]
    .spacing(2)
    .align_y(Alignment::Center);
    let label = text(label).size(12).font(style::MEDIUM).color(palette.text);
    let content = column![screens, label]
        .spacing(7)
        .align_x(Alignment::Center);
    let content = container(content)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    let idle = if current {
        style::alpha(palette.accent, 0.16)
    } else {
        palette.hover
    };
    let hover = style::alpha(palette.accent, 0.26);
    let outline = if current {
        palette.accent
    } else {
        Color::TRANSPARENT
    };
    button(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(0)
        .on_press(Message::Menu(MenuAction::OpenOn(screen)))
        .style(move |theme, status| {
            let mut look = style::tinted(idle, hover, 10.0)(theme, status);
            look.border.color = outline;
            look.border.width = 1.5;
            look
        })
        .into()
}

fn item<'a>(action: MenuAction, palette: Palette) -> Element<'a, Message> {
    let (glyph, label, hint, color) = match &action {
        MenuAction::NewWindow => (Glyph::Plus, "Nouvelle fenêtre", "", palette.text),
        MenuAction::Pin => (Glyph::Bookmark, "Épingler aux favoris", "", palette.text),
        MenuAction::Unpin => (Glyph::Bookmark, "Retirer des favoris", "", palette.text),
        MenuAction::Forget => (
            Glyph::History,
            "Oublier sa place",
            "rouvrira en flottant",
            palette.text,
        ),
        MenuAction::Close(_) => (Glyph::Close, "Fermer la fenêtre", "", style::DANGER),
        MenuAction::OpenOn(_) => (Glyph::Plus, "", "", palette.text),
    };
    let content = row![
        icons::glyph(glyph, 16.0, color),
        text(label).size(13).font(style::MEDIUM).color(color),
        space().width(Length::Fill),
        text(hint).size(11).color(palette.muted)
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    button(container(content).center_y(Length::Fill))
        .width(Length::Fill)
        .height(ITEM)
        .padding(Padding::from([0.0, 10.0]))
        .on_press(Message::Menu(action))
        .style(style::flat(palette.hover, 9.0))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(actions: Vec<MenuAction>) -> Menu {
        Menu {
            title: String::new(),
            subtitle: String::new(),
            current: None,
            actions,
        }
    }

    #[test]
    fn the_menu_is_as_tall_as_its_lines() {
        let tiles =
            [OutputRole::Left, OutputRole::Center, OutputRole::Right].map(MenuAction::OpenOn);
        let short = menu(tiles.to_vec());
        let mut longer = tiles.to_vec();
        longer.extend([MenuAction::NewWindow, MenuAction::Close(1)]);

        // A plain line, then one that stands apart behind a hairline.
        let grown = height(&menu(longer)) - height(&short);
        assert_eq!(grown, (2.0 * (ITEM + GAP) + RULE) as u32);
    }
}
