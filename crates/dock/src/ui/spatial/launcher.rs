//! Spatial launcher surface (permanent Bottom home or full-blur Overlay).

use coquille_ipc::Window;
use coquille_model::OutputRole;
use iced::mouse;
use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Row, button, column, container, mouse_area, row, space, text, text_input,
};
use iced::{Alignment, Border, Color, Element, Length, Padding, Size, window};

use super::cards;
use crate::app::{Dock, Message};
use crate::model::drawer;
use crate::model::spatial::{
    APP_ROW_GAP, APP_TILE_HEIGHT, MAX_APPS_WIDTH, MAX_CONTENT_WIDTH, SEARCH_MAX_WIDTH,
    SpatialLayout, filter_apps,
};
use crate::ui::icons::{self, Glyph};
use crate::ui::style::{self, Palette};

/// Whether the spatial launcher is rendered as a permanent `Layer::Bottom` home
/// or as a full-screen `Layer::Overlay` over a maximized window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LauncherMode {
    /// Permanent `Layer::Bottom` home shown on screens with no visible window.
    Home,
    /// Invoked `Layer::Overlay` over a maximized window: full Kawase blur backdrop.
    Overlay,
}

/// Unique `text_input` identifier for the spatial search field on each surface.
pub fn search_id(screen: Option<OutputRole>) -> &'static str {
    match screen {
        None => crate::app::SEARCH,
        Some(OutputRole::Left) => "spatial-search-left",
        Some(OutputRole::Center) => "spatial-search-center",
        Some(OutputRole::Right) => "spatial-search-right",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppPresence {
    Here,
    Elsewhere,
    None,
}

fn app_presence(
    windows: &[Window],
    names: &[String],
    key: &str,
    screen: OutputRole,
) -> AppPresence {
    let mut elsewhere = false;
    for win in windows {
        let matches =
            win.app == key || names.iter().any(|name| name.eq_ignore_ascii_case(&win.app));
        if !matches {
            continue;
        }
        if win.screen == screen {
            return AppPresence::Here;
        }
        elsewhere = true;
    }
    if elsewhere {
        AppPresence::Elsewhere
    } else {
        AppPresence::None
    }
}

/// Renders the spatial launcher: centered search bar, 2-column Liquid Glass
/// dashboard, and paginated GNOME-style floating application grid (no scroll).
#[allow(clippy::too_many_arguments)]
pub fn view<'a>(
    dock: &'a Dock,
    surface_id: window::Id,
    screen: OutputRole,
    query: &'a str,
    page: usize,
    surface_size: Option<Size>,
    mode: LauncherMode,
    palette: Palette,
) -> Element<'a, Message> {
    let fallback_screen = dock.snapshot.screens.iter().find(|s| s.role == screen);
    let usable_width = surface_size
        .map(|size| size.width)
        .or_else(|| fallback_screen.map(|s| s.width as f32))
        .unwrap_or(1920.0);
    let usable_height = surface_size
        .map(|size| size.height)
        .or_else(|| fallback_screen.map(|s| s.height as f32))
        .unwrap_or(1080.0);
    let layout = SpatialLayout::for_size(usable_width, usable_height);
    let target = match mode {
        LauncherMode::Overlay => None,
        LauncherMode::Home => Some(screen),
    };

    let search = search_bar(query, target, palette);
    let dashboard = dashboard_section(dock, &layout, palette);
    let apps_grid = apps_section(
        dock, surface_id, screen, target, query, page, &layout, palette,
    );

    let content = column![search, dashboard, apps_grid]
        .spacing(18)
        .width(Length::Fill)
        .align_x(Alignment::Center);

    let body = container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(Padding {
            top: 20.0,
            right: layout.padding_x,
            bottom: 18.0,
            left: layout.padding_x,
        });

    let backdrop = match mode {
        LauncherMode::Home => Color::TRANSPARENT,
        LauncherMode::Overlay => style::alpha(palette.glass, palette.glass.a.max(0.55)),
    };

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| style::fill(backdrop, 0.0))
        .into()
}

fn search_bar<'a>(
    query: &'a str,
    target: Option<OutputRole>,
    palette: Palette,
) -> Element<'a, Message> {
    let field = text_input("Rechercher une application ou un jeu…", query)
        .id(search_id(target))
        .on_input(move |q| Message::SpatialQuery(target, q))
        .on_submit(Message::SpatialSubmit(target))
        .size(14.5)
        .padding(0)
        .width(Length::Fill)
        .style(move |_, _| text_input::Style {
            background: Color::TRANSPARENT.into(),
            border: Border::default(),
            icon: palette.muted,
            placeholder: palette.muted,
            value: palette.text,
            selection: style::alpha(palette.accent, 0.35),
        });

    let mut line = row![icons::glyph(Glyph::Search, 16.0, palette.muted), field]
        .spacing(10)
        .align_y(Alignment::Center);

    if !query.is_empty() {
        let clear = button(icons::glyph(Glyph::Close, 14.0, palette.muted))
            .padding(Padding::from([4.0, 4.0]))
            .on_press(Message::SpatialQuery(target, String::new()))
            .style(style::flat(palette.hover, 8.0));
        line = line.push(clear);
    }

    container(line)
        .width(Length::Fill)
        .max_width(SEARCH_MAX_WIDTH)
        .padding(Padding::from([10.0, 14.0]))
        .style(move |_| style::outlined(palette.glass, palette.rim, 16.0))
        .into()
}

const ROW_1_HEIGHT: f32 = 248.0;
const ROW_2_HEIGHT: f32 = 274.0;

fn dashboard_section<'a>(
    dock: &'a Dock,
    layout: &SpatialLayout,
    palette: Palette,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = if layout.two_columns {
        let top_row = row![
            container(cards::daily_tasks_card(dock, true, palette))
                .width(Length::FillPortion(5))
                .height(Length::Fill),
            container(cards::games_card(dock, layout.game_columns, true, palette))
                .width(Length::FillPortion(7))
                .height(Length::Fill),
        ]
        .spacing(18)
        .height(ROW_1_HEIGHT);

        let bottom_row = row![
            container(cards::project_tasks_card(dock, true, palette))
                .width(Length::FillPortion(5))
                .height(Length::Fill),
            container(cards::calendar_card(
                dock,
                layout.calendar_columns,
                true,
                palette
            ))
            .width(Length::FillPortion(7))
            .height(Length::Fill),
        ]
        .spacing(18)
        .height(ROW_2_HEIGHT);

        column![top_row, bottom_row].spacing(14).into()
    } else {
        column![
            cards::daily_tasks_card(dock, false, palette),
            cards::project_tasks_card(dock, false, palette),
            cards::games_card(dock, layout.game_columns, false, palette),
            cards::calendar_card(dock, layout.calendar_columns, false, palette),
        ]
        .spacing(14)
        .into()
    };

    container(body)
        .width(Length::Fill)
        .max_width(MAX_CONTENT_WIDTH)
        .into()
}

#[allow(clippy::too_many_arguments)]
fn apps_section<'a>(
    dock: &'a Dock,
    surface_id: window::Id,
    screen: OutputRole,
    target: Option<OutputRole>,
    query: &'a str,
    page: usize,
    layout: &SpatialLayout,
    palette: Palette,
) -> Element<'a, Message> {
    let filtered = filter_apps(&dock.apps, query);
    let count = filtered.len();
    let page_size = layout.page_size();
    let pages = drawer::page_count(count, page_size);
    let current_page = page.min(pages - 1);

    let summary = if query.trim().is_empty() {
        drawer::summary(count, current_page, page_size)
    } else if count == 0 {
        "Aucun résultat".to_owned()
    } else if count == 1 {
        "1 résultat".to_owned()
    } else {
        format!("{count} résultats · page {} sur {pages}", current_page + 1)
    };

    let arrow = |glyph: Glyph, forward: bool, enabled: bool| {
        let color = if enabled {
            palette.text
        } else {
            style::alpha(palette.muted, 0.45)
        };
        let btn = button(container(icons::glyph(glyph, 14.0, color)).center(Length::Fill))
            .width(28)
            .height(28)
            .padding(0)
            .style(style::flat(palette.hover, 8.0));
        if enabled {
            btn.on_press(Message::SpatialTurnPage(target, forward))
        } else {
            btn
        }
    };

    let mut pager: Row<'_, Message> =
        row![arrow(Glyph::ChevronLeft, false, current_page > 0)].spacing(4);
    for number in drawer::page_buttons(current_page, pages) {
        let active = number == current_page;
        let color = if active {
            palette.on_accent()
        } else {
            palette.text
        };
        let idle = if active {
            palette.accent
        } else {
            Color::TRANSPARENT
        };
        let hover = if active {
            palette.accent
        } else {
            palette.hover
        };
        let label = text((number + 1).to_string())
            .size(12.5)
            .font(style::SEMIBOLD)
            .color(color);
        pager = pager.push(
            button(container(label).center(Length::Fill))
                .width(28)
                .height(28)
                .padding(0)
                .on_press(Message::SpatialPage(target, number))
                .style(style::tinted(idle, hover, 8.0)),
        );
    }
    pager = pager.push(arrow(Glyph::ChevronRight, true, current_page + 1 < pages));

    let title = text("APPLICATIONS")
        .size(11.5)
        .font(style::SEMIBOLD)
        .color(palette.muted);
    let meta = text(summary).size(11.5).color(palette.muted);
    let header = row![
        title,
        meta,
        space().width(Length::Fill),
        container(pager)
            .padding(Padding::from([2.0, 4.0]))
            .style(move |_| style::outlined(palette.glass, palette.rim, 12.0)),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    let grid_height = layout.app_rows as f32 * APP_TILE_HEIGHT
        + layout.app_rows.saturating_sub(1) as f32 * APP_ROW_GAP;

    let body: Element<'a, Message> = if filtered.is_empty() {
        container(
            text("Aucune application ne correspond à cette recherche.")
                .size(14)
                .color(palette.muted),
        )
        .width(Length::Fill)
        .height(grid_height)
        .center_x(Length::Fill)
        .center_y(grid_height)
        .into()
    } else {
        let visible = drawer::page(&filtered, current_page, page_size);
        let cells: Vec<Element<'a, Message>> = visible
            .iter()
            .map(|app| app_tile(dock, surface_id, screen, &app.id, palette))
            .collect();

        let mut rows: Column<'_, Message> = column![].spacing(APP_ROW_GAP);
        let cols = layout.app_columns.max(1);
        let mut iter = cells.into_iter().peekable();
        while iter.peek().is_some() {
            let mut line: Row<'_, Message> = row![].spacing(10);
            for _ in 0..cols {
                line = line.push(match iter.next() {
                    Some(cell) => cell,
                    None => space().width(Length::FillPortion(1)).into(),
                });
            }
            rows = rows.push(line);
        }

        let area = container(rows).width(Length::Fill).height(grid_height);
        mouse_area(area)
            .on_scroll(move |delta| {
                let forward = match delta {
                    mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => {
                        y < 0.0
                    }
                };
                Message::SpatialTurnPage(target, forward)
            })
            .into()
    };

    container(column![header, body].spacing(10))
        .width(Length::Fill)
        .max_width(MAX_APPS_WIDTH)
        .into()
}

fn app_tile<'a>(
    dock: &'a Dock,
    surface_id: window::Id,
    screen: OutputRole,
    app_id: &str,
    palette: Palette,
) -> Element<'a, Message> {
    let identity = dock.identify(app_id);
    let icon = icons::tile(&dock.icons, &identity, 52.0, (44.0, 11.0), palette, false);
    let label = container(
        text(identity.name.clone())
            .size(12)
            .font(style::MEDIUM)
            .color(palette.text)
            .wrapping(Wrapping::None),
    )
    .center_x(Length::Fill)
    .max_height(18)
    .clip(true);

    let presence = app_presence(
        &dock.snapshot.windows,
        &identity.names,
        &identity.key,
        screen,
    );
    let dot: Element<'a, Message> = match presence {
        AppPresence::Here => container(space().width(6).height(6))
            .style(move |_| style::fill(palette.accent, 3.0))
            .into(),
        AppPresence::Elsewhere => container(space().width(6).height(6))
            .style(move |_| container::Style {
                background: Some(Color::TRANSPARENT.into()),
                border: Border {
                    color: palette.accent,
                    width: 1.5,
                    radius: 3.0.into(),
                },
                ..container::Style::default()
            })
            .into(),
        AppPresence::None => space().width(6).height(6).into(),
    };

    let tile_content = column![icon, label, dot]
        .spacing(5)
        .align_x(Alignment::Center)
        .width(Length::Fill);

    let key = identity.key.clone();
    let launch_btn = button(tile_content)
        .width(Length::Fill)
        .height(APP_TILE_HEIGHT)
        .padding(Padding::from([8.0, 6.0]))
        .on_press(Message::SpatialLaunchApp(screen, key.clone()))
        .style(style::flat(palette.hover, 16.0));

    mouse_area(launch_btn)
        .on_right_press(Message::SpatialAppMenu(surface_id, screen, key))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u64, app: &str, screen: OutputRole) -> Window {
        Window {
            id,
            app: app.into(),
            title: String::new(),
            screen,
            slot: None,
            active: false,
            minimized: false,
        }
    }

    #[test]
    fn search_id_is_unique_per_target() {
        let overlay = search_id(None);
        let left = search_id(Some(OutputRole::Left));
        let center = search_id(Some(OutputRole::Center));
        let right = search_id(Some(OutputRole::Right));

        assert_eq!(overlay, crate::app::SEARCH);
        assert_ne!(overlay, left);
        assert_ne!(left, center);
        assert_ne!(center, right);
        assert_ne!(left, right);
    }

    #[test]
    fn app_presence_detects_here_elsewhere_and_none() {
        let windows = [
            window(1, "firefox", OutputRole::Left),
            window(2, "Code", OutputRole::Right),
        ];
        let ff_names = vec!["firefox".to_owned(), "Navigator".to_owned()];
        let code_names = vec!["code-oss".to_owned(), "code".to_owned()];
        let term_names = vec!["kitty".to_owned()];

        assert_eq!(
            app_presence(&windows, &ff_names, "firefox", OutputRole::Left),
            AppPresence::Here
        );
        assert_eq!(
            app_presence(&windows, &ff_names, "firefox", OutputRole::Center),
            AppPresence::Elsewhere
        );
        assert_eq!(
            app_presence(&windows, &code_names, "code-oss", OutputRole::Right),
            AppPresence::Here
        );
        assert_eq!(
            app_presence(&windows, &term_names, "kitty", OutputRole::Left),
            AppPresence::None
        );
    }
}
