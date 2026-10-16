//! The contextual popup menu for a system tray item, populated from its DBusMenu.

use iced::widget::{Column, button, column, container, row, space, text};
use iced::{Alignment, Element, Length, Padding};
use system_tray::menu::{MenuItem, MenuType, ToggleState, ToggleType};

use super::icons::{self, Glyph};
use super::style::{self, Palette};
use crate::app::{Dock, Message};
use crate::model::tray::TrayItem;

pub const WIDTH: u32 = 220;
const PADDING: f32 = 8.0;
const HEADER: f32 = 32.0;
const ITEM_HEIGHT: f32 = 30.0;
const RULE_HEIGHT: f32 = 7.0;
const GAP: f32 = 2.0;

/// Height of the popup for a tray item's menu.
pub fn height(item: &TrayItem) -> u32 {
    let mut total = 2.0 * PADDING + HEADER + GAP;
    if let Some(menu) = &item.menu {
        for sub in &menu.submenus {
            if !sub.visible {
                continue;
            }
            if sub.menu_type == MenuType::Separator {
                total += RULE_HEIGHT;
            } else {
                total += ITEM_HEIGHT + GAP;
            }
        }
    } else {
        // Fallback item: single "Ouvrir" action
        total += ITEM_HEIGHT + GAP;
    }
    total.ceil() as u32
}

pub fn view<'a>(dock: &'a Dock, item: &'a TrayItem) -> Element<'a, Message> {
    let palette = dock.palette();

    let title = text(item.title.clone())
        .size(13)
        .font(style::SEMIBOLD)
        .color(palette.text);

    let header = row![crate::ui::bar::tray_icon_element(dock, item, 20.0), title]
        .spacing(8)
        .align_y(Alignment::Center)
        .height(HEADER);

    let rule = || -> Element<'a, Message> {
        let line = container(space().width(Length::Fill).height(1))
            .style(move |_: &iced::Theme| style::fill(palette.separator, 0.0));
        container(line)
            .height(RULE_HEIGHT)
            .center_y(RULE_HEIGHT)
            .into()
    };

    let mut content: Column<'_, Message> =
        column![container(header).padding(Padding::from([0.0, 4.0])), rule()].spacing(GAP);

    if let (Some(menu), Some(menu_path)) = (&item.menu, &item.menu_path) {
        for sub in &menu.submenus {
            if !sub.visible {
                continue;
            }
            if sub.menu_type == MenuType::Separator {
                content = content.push(rule());
            } else {
                content = content.push(menu_item_row(
                    item.address.clone(),
                    menu_path.clone(),
                    sub,
                    palette,
                ));
            }
        }
    } else {
        // Default action if no DBusMenu
        let open_btn = button(
            container(text("Ouvrir").size(13).color(palette.text)).align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .height(ITEM_HEIGHT)
        .padding(Padding::from([0.0, 8.0]))
        .style(style::flat(palette.hover, 6.0))
        .on_press(Message::TrayClick(item.address.clone()));

        content = content.push(open_btn);
    }

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(PADDING)
        .style(move |_| style::outlined(palette.glass_strong, palette.rim, 12.0))
        .into()
}

fn menu_item_row<'a>(
    address: String,
    menu_path: String,
    item: &'a MenuItem,
    palette: Palette,
) -> Element<'a, Message> {
    let raw_label = item.label.as_deref().unwrap_or_default();
    let label_text = clean_label(raw_label);

    let text_color = if item.enabled {
        palette.text
    } else {
        palette.muted
    };

    let check_icon: Option<Element<'_, Message>> = match item.toggle_type {
        ToggleType::Checkmark if item.toggle_state == ToggleState::On => {
            Some(icons::glyph(Glyph::Check, 12.0, palette.accent))
        }
        ToggleType::Radio if item.toggle_state == ToggleState::On => {
            Some(icons::glyph(Glyph::Check, 12.0, palette.accent))
        }
        _ => None,
    };

    let mut row_items = Vec::new();
    if let Some(icon) = check_icon {
        row_items.push(icon);
    }
    row_items.push(text(label_text).size(13).color(text_color).into());

    let label_row = Row::with_children(row_items)
        .spacing(6)
        .align_y(Alignment::Center);

    let mut btn = button(container(label_row).align_y(Alignment::Center))
        .width(Length::Fill)
        .height(ITEM_HEIGHT)
        .padding(Padding::from([0.0, 8.0]))
        .style(style::flat(palette.hover, 6.0));

    if item.enabled {
        btn = btn.on_press(Message::TrayMenuItemClick {
            address,
            menu_path,
            id: item.id,
        });
    }

    btn.into()
}

use iced::widget::Row;

/// Removes accelerator underscores like in GTK/Qt ("_Quit" -> "Quit", "__" -> "_").
fn clean_label(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '_' {
            if chars.peek() == Some(&'_') {
                chars.next();
                out.push('_');
            }
            // Otherwise skip accelerator underscore
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_accelerator_label() {
        assert_eq!(clean_label("_Fichier"), "Fichier");
        assert_eq!(clean_label("Enregistrer sous..."), "Enregistrer sous...");
        assert_eq!(clean_label("A__B"), "A_B");
        assert_eq!(clean_label("_Quitter"), "Quitter");
    }
}
