//! The menu opened by a right click on an application of the bar.

use coquille_model::OutputRole;
use iced::widget::text::Wrapping;
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

/// How many characters of the name of a desktop action fit on its line.
const ACTION_NAME: usize = 25;

/// `name`, cut with an ellipsis past `room` characters.
fn shortened(name: &str, room: usize) -> String {
    if name.chars().count() <= room {
        return name.to_owned();
    }
    let kept: String = name.chars().take(room.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Whether a line of the menu comes after a hairline: forgetting and
/// closing each stand apart.
fn after_rule(action: &MenuAction) -> bool {
    matches!(action, MenuAction::Forget | MenuAction::Close(_))
}

/// The screens of the "open on" tiles; none on a lone screen.
fn tiles(menu: &Menu) -> Vec<OutputRole> {
    let screen = |action: &MenuAction| match action {
        MenuAction::OpenOn(screen) => Some(*screen),
        _ => None,
    };
    menu.actions.iter().filter_map(screen).collect()
}

fn lines(menu: &Menu) -> impl Iterator<Item = &MenuAction> {
    menu.actions
        .iter()
        .filter(|action| !matches!(action, MenuAction::OpenOn(_)))
}

/// The height of the popup for this menu.
pub fn height(menu: &Menu) -> u32 {
    let mut height = 2.0 * PADDING + HEADER + GAP;
    if !tiles(menu).is_empty() {
        height += TITLE + TILES + GAP;
    }
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
    let mut menu = bar::menu(entry, &dock.present_screens());
    if dock.is_spatial() {
        menu.actions
            .retain(|action| !matches!(action, MenuAction::Forget));
    }

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
    let screens = tiles(&menu);
    let mut content: Column<'_, Message> =
        column![container(header).padding(Padding::from([0.0, 4.0]))].spacing(GAP);
    if !screens.is_empty() {
        let mut line = row![].spacing(6).height(TILES);
        for &screen in &screens {
            line = line.push(screen_tile(
                screen,
                &screens,
                menu.current == Some(screen),
                palette,
            ));
        }
        content = content.push(open_on).push(line);
    }
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
        .style(move |_| style::outlined(palette.glass_strong, palette.rim, 14.0))
        .into()
}

/// One of the screens an application can be opened on: a small picture of
/// the screens, with that one lit.
fn screen_tile<'a>(
    screen: OutputRole,
    screens: &[OutputRole],
    current: bool,
    palette: Palette,
) -> Element<'a, Message> {
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
    let mut pictures = row![].spacing(2).align_y(Alignment::Center);
    for &role in screens {
        pictures = pictures.push(mini(role));
    }
    let label = text(label).size(12).font(style::MEDIUM).color(palette.text);
    let content = column![pictures, label]
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
    let name;
    let (glyph, label, hint, color) = match &action {
        MenuAction::NewWindow => (Glyph::Plus, "Nouvelle fenêtre", "", palette.text),
        MenuAction::Run { name: full, .. } => {
            name = shortened(full, ACTION_NAME);
            (Glyph::Play, name.as_str(), "", palette.text)
        }
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
    let label = text(label.to_owned())
        .size(13)
        .font(style::MEDIUM)
        .color(color)
        .wrapping(Wrapping::None);
    let content = row![
        icons::glyph(glyph, 16.0, color),
        container(label).clip(true),
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
    fn a_long_action_name_is_cut_with_an_ellipsis() {
        assert_eq!(shortened("Nouvelle fenêtre", 25), "Nouvelle fenêtre");
        assert_eq!(
            shortened("Nouvelle fenêtre de navigation privée", 25),
            "Nouvelle fenêtre de navi…"
        );
        assert_eq!(shortened("Ouvrir la bibliothèque", 10), "Ouvrir la…");
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
