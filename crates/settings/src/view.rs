//! What the settings screen draws: the pages on the left, the settings of
//! the chosen one on the right, each in a card.

use coquille_config::{Change, DockPlace, MouseAccelProfile, PerScreen, WallpaperMode};
use coquille_ipc::ThemeName;
use coquille_model::OutputRole;
use coquille_ui::glyph::{Glyph, glyph};
use coquille_ui::style::{self, Palette, parse_color};
use iced::overlay::menu;
use iced::widget::{
    Column, Row, button, column, container, pick_list, row, scrollable, slider, space, text,
    text_input, toggler,
};
use iced::{Alignment, Border, Color, Element, Length, Padding};

use crate::app::{App, Message};
use crate::model::{self, Choice, Page, ScreenCard};

/// The width of the list of pages.
const SIDEBAR: f32 = 232.0;
/// The widest the settings of a page get.
const PAGE: f32 = 720.0;
/// The width of the drawing of the three screens, without its gaps.
const DRAWING: f32 = 420.0;

/// What is behind everything: translucent glass inside coquille, opaque when standalone.
pub fn backdrop(palette: Palette, connected: bool) -> Color {
    if connected {
        palette.glass
    } else {
        style::alpha(palette.glass_strong, 1.0)
    }
}

pub fn window(app: &App) -> Element<'_, Message> {
    let palette = app.palette();
    let vrule = container(space().width(1).height(Length::Fill))
        .style(move |_| style::fill(palette.separator, 0.0));
    row![sidebar(app, palette), vrule, page(app, palette)].into()
}

fn sidebar<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    let title = text("Paramètres")
        .size(20)
        .font(style::SEMIBOLD)
        .color(palette.text);
    let mut pages: Column<'_, Message> = column![].spacing(4);
    for page in Page::ALL {
        pages = pages.push(page_button(page, page == app.page, palette));
    }
    let note = if app.connected() {
        "Chaque changement est enregistré et appliqué aussitôt."
    } else {
        "Hors de coquille : les changements sont enregistrés, et appliqués à son prochain \
         démarrage."
    };
    let note = text(note).size(12).color(palette.muted);
    let content = column![
        container(title).padding(Padding::from([6.0, 10.0])),
        pages,
        space().height(Length::Fill),
        container(note).padding(Padding::from([4.0, 10.0])),
    ]
    .spacing(14);
    container(content)
        .width(SIDEBAR)
        .height(Length::Fill)
        .padding(14)
        .style(move |_| style::fill(palette.glass, 0.0))
        .into()
}

fn page_button<'a>(page: Page, current: bool, palette: Palette) -> Element<'a, Message> {
    let icon = match page {
        Page::Appearance => Glyph::Brush,
        Page::Screens => Glyph::Screens,
        Page::Audio => Glyph::Volume,
        Page::Game => Glyph::Gamepad,
        Page::Mouse => Glyph::Mouse,
        Page::System => Glyph::Keyboard,
    };
    let (color, font) = if current {
        (palette.text, style::SEMIBOLD)
    } else {
        (palette.muted, style::MEDIUM)
    };
    let mark_color = if current {
        palette.accent
    } else {
        Color::TRANSPARENT
    };
    let mark = container(space().width(3).height(16)).style(move |_| style::fill(mark_color, 2.0));
    let line = row![
        mark,
        glyph(icon, 18.0, color),
        text(page.name()).size(14).font(font).color(color)
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let (idle, hover) = if current {
        (palette.card, palette.card)
    } else {
        (Color::TRANSPARENT, palette.hover)
    };
    button(line)
        .width(Length::Fill)
        .padding(Padding::from([9.0, 8.0]))
        .on_press(Message::Page(page))
        .style(style::tinted(idle, hover, 8.0))
        .into()
}

fn page<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    let heading = column![
        text(app.page.name())
            .size(24)
            .font(style::SEMIBOLD)
            .color(palette.text),
        text(app.page.summary()).size(13).color(palette.muted)
    ]
    .spacing(4);
    let mut content: Column<'_, Message> = column![heading].spacing(18).max_width(PAGE);
    if let Some(error) = &app.error {
        content = content.push(banner(error, palette));
    }
    let settings = match app.page {
        Page::Appearance => appearance(app, palette),
        Page::Screens => screens(app, palette),
        Page::Audio => audio(app, palette),
        Page::Game => game(app, palette),
        Page::Mouse => mouse(app, palette),
        Page::System => system(app, palette),
    };
    let content = content.push(settings);
    scrollable(
        container(content)
            .width(Length::Fill)
            .padding(Padding::from([24.0, 28.0])),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Why the last change was not written.
fn banner<'a>(error: &'a str, palette: Palette) -> Element<'a, Message> {
    container(text(error).size(13).color(palette.text))
        .width(Length::Fill)
        .padding(Padding::from([10.0, 14.0]))
        .style(move |_| {
            style::outlined(
                style::alpha(style::DANGER, 0.14),
                style::alpha(style::DANGER, 0.5),
                10.0,
            )
        })
        .into()
}

// ---- The pages ----

fn appearance<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    let custom = app.file.accent.as_deref().and_then(parse_color);
    let current = app.theme();
    let themes = row![
        theme_tile(ThemeName::Light, "Clair", current, custom, palette),
        theme_tile(ThemeName::Dark, "Sombre", current, custom, palette)
    ]
    .spacing(12);
    let theme = column![
        words("Thème", "Celui des fenêtres et du dock.", palette),
        themes
    ]
    .spacing(12);

    // The accent of the theme first, then the ones offered.
    let usual = Palette::new(current, None).accent;
    let mut swatches: Row<'_, Message> =
        row![usual_accent(usual, app.file.accent.is_none(), palette)]
            .spacing(8)
            .align_y(Alignment::Center);
    for accent in model::ACCENTS {
        let chosen = app
            .file
            .accent
            .as_deref()
            .is_some_and(|set| set.eq_ignore_ascii_case(accent));
        let change = Change::Accent(Some(accent.to_owned()));
        if let Some(color) = parse_color(accent) {
            swatches = swatches.push(swatch(color, chosen, change, palette));
        }
    }
    let typed = &app.accent_text;
    let wrong = !typed.is_empty() && parse_color(typed).is_none();
    let field = text_input("#rrggbb", typed)
        .on_input(Message::AccentText)
        .size(13)
        .font(style::MEDIUM)
        .width(104)
        .padding(Padding::from([7.0, 10.0]))
        .style(move |_, status| {
            let line = match status {
                _ if wrong => style::DANGER,
                text_input::Status::Focused { .. } => palette.accent,
                _ => palette.rim,
            };
            text_input::Style {
                background: palette.card.into(),
                border: Border {
                    color: line,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                icon: palette.muted,
                placeholder: palette.muted,
                value: palette.text,
                selection: style::alpha(palette.accent, 0.35),
            }
        });
    let accent = column![
        words(
            "Couleur d'accent",
            "L'anneau de la fenêtre active et les repères du dock.",
            palette
        ),
        row![swatches, space().width(Length::Fill), field].align_y(Alignment::Center)
    ]
    .spacing(12);

    let wallpaper_modes = [
        (WallpaperMode::Fill, "Remplir"),
        (WallpaperMode::Fit, "Ajuster"),
        (WallpaperMode::Center, "Centrer"),
        (WallpaperMode::Tile, "Mosaïque"),
    ];
    let mode_choice = segmented(
        &wallpaper_modes,
        app.file.wallpaper_mode(),
        |mode| Message::Change(Change::WallpaperMode(mode)),
        palette,
    );

    let image = app.file.wallpaper_path.as_deref().or_else(|| {
        app.file
            .wallpapers
            .center
            .as_deref()
            .or(app.file.wallpapers.left.as_deref())
            .or(app.file.wallpapers.right.as_deref())
    });

    let image_control: Element<'a, Message> = match image {
        Some(path) => {
            let display_path = model::short_path(
                std::path::Path::new(path),
                std::env::var_os("HOME").as_deref(),
            );
            let reset_btn = button(
                text("Rétablir")
                    .size(12)
                    .font(style::MEDIUM)
                    .color(palette.text),
            )
            .padding(Padding::from([6.0, 12.0]))
            .on_press(Message::Change(Change::WallpaperPath(None)))
            .style(style::tinted(palette.card, palette.hover, 6.0));

            row![
                text(display_path)
                    .size(13)
                    .color(palette.muted)
                    .width(Length::Fill),
                reset_btn
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        }
        None => text("Panoramique « verre » par défaut")
            .size(13)
            .color(palette.muted)
            .into(),
    };

    let wallpaper = vec![
        setting(
            "Mode d'affichage",
            "Comment l'image s'ajuste à la taille de l'écran.",
            None,
            mode_choice,
            palette,
        ),
        setting(
            "Image",
            "Image personnalisée (définie dans config.toml) ou fond par défaut.",
            None,
            image_control,
            palette,
        ),
    ];

    column![
        section(
            "Thème",
            card(vec![theme.into(), accent.into()], palette),
            palette
        ),
        section("Fond d'écran", card(wallpaper, palette), palette),
    ]
    .spacing(18)
    .into()
}

/// A theme to choose, shown as a small screen in its colors.
fn theme_tile<'a>(
    theme: ThemeName,
    name: &'a str,
    current: ThemeName,
    accent: Option<Color>,
    palette: Palette,
) -> Element<'a, Message> {
    let look = Palette::new(theme, accent);
    let chosen = theme == current;
    let bar = |width: f32, color: Color| {
        container(space().width(width).height(4)).style(move |_| style::fill(color, 2.0))
    };
    let lines = column![
        bar(44.0, style::alpha(look.text, 0.8)),
        bar(70.0, style::alpha(look.muted, 0.6)),
        bar(56.0, style::alpha(look.muted, 0.6))
    ]
    .spacing(6);
    let pane = container(lines)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(10)
        .style(move |_| container::Style {
            border: Border {
                color: look.accent,
                width: 2.0,
                radius: 6.0.into(),
            },
            ..style::fill(look.card, 6.0)
        });
    let dock = container(space().width(6).height(Length::Fill))
        .style(move |_| style::fill(style::alpha(look.muted, 0.45), 3.0));
    let scene = container(row![dock, pane].spacing(6))
        .width(176)
        .height(96)
        .padding(8)
        .style(move |_| style::fill(style::alpha(look.block, 1.0), 8.0));
    let font = if chosen {
        style::SEMIBOLD
    } else {
        style::MEDIUM
    };
    let label = text(name).size(13).font(font).color(palette.text);
    let tile = column![scene, label].spacing(8).align_x(Alignment::Center);
    button(tile)
        .padding(8)
        .on_press(Message::Change(Change::Theme(theme)))
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let (line, width) = match (chosen, hovered) {
                (true, _) => (palette.accent, 2.0),
                (false, true) => (palette.muted, 1.0),
                (false, false) => (palette.rim, 1.0),
            };
            button::Style {
                border: Border {
                    color: line,
                    width,
                    radius: 12.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// The choice that leaves the accent to the theme.
fn usual_accent<'a>(color: Color, chosen: bool, palette: Palette) -> Element<'a, Message> {
    let dot = container(space().width(14).height(14)).style(move |_| style::fill(color, 7.0));
    let name = text("Celle du thème")
        .size(13)
        .font(style::MEDIUM)
        .color(palette.text);
    let line = row![dot, name].spacing(8).align_y(Alignment::Center);
    button(container(line).center_y(Length::Fill))
        .height(30)
        .padding(Padding::from([0.0, 12.0]))
        .on_press(Message::Change(Change::Accent(None)))
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let line = match (chosen, hovered) {
                (true, _) => palette.accent,
                (false, true) => palette.muted,
                (false, false) => palette.rim,
            };
            button::Style {
                background: Some(palette.card.into()),
                border: Border {
                    color: line,
                    width: if chosen { 2.0 } else { 1.0 },
                    radius: 15.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// A color to choose as the accent.
fn swatch<'a>(
    color: Color,
    chosen: bool,
    change: Change,
    palette: Palette,
) -> Element<'a, Message> {
    let mark: Element<'a, Message> = if chosen {
        glyph(Glyph::Check, 16.0, Color::WHITE)
    } else {
        space().into()
    };
    button(container(mark).center(Length::Fill))
        .width(30)
        .height(30)
        .padding(0)
        .on_press(Message::Change(change))
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let line = if chosen || hovered {
                palette.text
            } else {
                Color::TRANSPARENT
            };
            button::Style {
                background: Some(color.into()),
                border: Border {
                    color: line,
                    width: 2.0,
                    radius: 15.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

fn screens<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    let snapshot = app.snapshot.as_ref();
    let cards = model::screens(&app.file, snapshot);
    let choices = model::connector_choices(&app.file, snapshot);
    let simulated = snapshot.is_some_and(|snapshot| snapshot.simulated);

    let mut content: Column<'_, Message> = column![drawing(app, &cards, palette)].spacing(18);
    for role in PerScreen::<()>::ROLES {
        let screen = cards.get(role);
        let connector: Element<'a, Message> = if simulated {
            text("Écrans simulés").size(13).color(palette.muted).into()
        } else if choices.is_empty() {
            text("Aucun écran connu")
                .size(13)
                .color(palette.muted)
                .into()
        } else {
            let chosen = model::chosen(&choices, screen.connector.as_deref());
            let pick = move |choice| Message::Connector(role, choice);
            picker(choices.clone(), chosen, pick, 180.0, palette)
        };
        let places = [
            (DockPlace::Vertical, "À gauche"),
            (DockPlace::VerticalRight, "À droite"),
            (DockPlace::Horizontal, "En bas"),
            (DockPlace::Off, "Aucun"),
        ];
        let dock = segmented(
            &places,
            app.file.dock(role),
            move |place| Message::Change(Change::Dock(role, place)),
            palette,
        );
        let rows = vec![
            setting(
                "Connecteur",
                "La prise sur laquelle cet écran est branché.",
                Some("Au prochain démarrage"),
                connector,
                palette,
            ),
            setting(
                "Dock",
                "Une colonne sur le bord gauche ou droit, ou une rangée sur le bord bas.",
                None,
                dock,
                palette,
            ),
        ];
        let title = row![
            text(model::screen_name(role))
                .size(14)
                .font(style::SEMIBOLD)
                .color(palette.text),
            text(screen.detail()).size(13).color(palette.muted)
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        content = content.push(column![title, card(rows, palette)].spacing(8));
    }

    let icon_size = row![
        slider(
            model::DOCK_ICON_SIZE_RANGE,
            app.dock_icon_size,
            Message::DockIconSize,
        )
        .step(model::DOCK_ICON_SIZE_STEP)
        .on_release(Message::DockIconSizeReleased)
        .width(200)
        .style(move |_, _| slider_style(palette)),
        text(format!("{} px", app.dock_icon_size))
            .size(13)
            .font(style::MEDIUM)
            .color(palette.text)
            .width(52)
            .align_x(Alignment::End),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    let dock_settings = vec![setting(
        "Taille des icônes",
        "La taille des icônes dans la barre du dock (en pixels).",
        None,
        icon_size,
        palette,
    )];
    let dock_title = text("Dock")
        .size(14)
        .font(style::SEMIBOLD)
        .color(palette.text);
    content = content.push(column![dock_title, card(dock_settings, palette)].spacing(8));

    content.into()
}

/// The three screens side by side, in their proportions, each with its dock.
fn drawing<'a>(
    app: &'a App,
    cards: &PerScreen<ScreenCard>,
    palette: Palette,
) -> Element<'a, Message> {
    let sizes = model::drawing(cards, DRAWING);
    let mut line: Row<'_, Message> = row![].spacing(8).align_y(Alignment::Start);
    for role in PerScreen::<()>::ROLES {
        let (width, height) = *sizes.get(role);
        let name = match role {
            OutputRole::Left => "Gauche",
            OutputRole::Center => "Centre",
            OutputRole::Right => "Droite",
        };
        let label = container(text(name).size(12).font(style::MEDIUM).color(palette.muted))
            .center(Length::Fill);
        let dock = |width: Length, height: Length| {
            container(space().width(width).height(height))
                .style(move |_| style::fill(palette.accent, 2.0))
        };
        let inside: Element<'a, Message> = match app.file.dock(role) {
            DockPlace::Vertical => row![dock(4.0.into(), Length::Fill), label].into(),
            DockPlace::VerticalRight => row![label, dock(4.0.into(), Length::Fill)].into(),
            DockPlace::Horizontal => {
                let lying =
                    container(dock((width * 0.45).into(), 4.0.into())).center_x(Length::Fill);
                column![label, lying].into()
            }
            DockPlace::Off => label.into(),
        };
        let screen = container(inside)
            .width(width)
            .height(height)
            .padding(5)
            .style(move |_| style::outlined(palette.card, palette.rim, 6.0));
        line = line.push(screen);
    }
    container(line)
        .center_x(Length::Fill)
        .padding(Padding::from([18.0, 16.0]))
        .style(move |_| style::outlined(palette.card, palette.rim, 10.0))
        .into()
}

fn audio<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    let mut content: Column<'_, Message> = column![].spacing(18);

    if app.audio_sinks.is_empty() {
        let empty = card(
            vec![
                words(
                    "Aucun périphérique de sortie audio détecté",
                    "Vérifiez que PipeWire ou le serveur audio est actif.",
                    palette,
                )
                .into(),
            ],
            palette,
        );
        content = content.push(empty);
        return content.into();
    }

    for sink in &app.audio_sinks {
        let is_headphones = sink.description.to_lowercase().contains("headphone")
            || sink.description.to_lowercase().contains("casque");
        let icon = if is_headphones {
            Glyph::Headphones
        } else {
            Glyph::Volume
        };

        let display_name = sink.nick.as_deref().unwrap_or(&sink.description);

        let mut title_items: Row<'_, Message> = row![
            glyph(
                icon,
                16.0,
                if sink.active {
                    palette.accent
                } else {
                    palette.muted
                }
            ),
            text(display_name)
                .size(14)
                .font(style::SEMIBOLD)
                .color(palette.text),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        if sink.active {
            let active_pill = container(
                text("Actif")
                    .size(11)
                    .font(style::SEMIBOLD)
                    .color(palette.on_accent()),
            )
            .padding(Padding::from([2.0, 7.0]))
            .style(move |_| style::fill(palette.accent, 6.0));

            title_items = title_items.push(active_pill);
        }

        let subtitle: Element<'a, Message> = match &sink.nick {
            Some(_) => text(format!("{} — {}", sink.description, sink.name))
                .size(11)
                .color(palette.muted)
                .into(),
            None => text(&sink.name).size(11).color(palette.muted).into(),
        };

        let title_block = column![title_items, row![space().width(24), subtitle]].spacing(2);

        let visibility = setting(
            "Visible dans le sélecteur rapide",
            "Affiche ce périphérique dans la liste du volume du lanceur.",
            None,
            switch(
                sink.visible,
                {
                    let device = sink.name.clone();
                    move |on| Message::AudioVisibility(device.clone(), on)
                },
                palette,
            ),
            palette,
        );

        let current_input = app
            .audio_nick_inputs
            .get(&sink.name)
            .map(String::as_str)
            .unwrap_or("");
        let saved_nick = sink.nick.as_deref().unwrap_or("");
        let is_dirty = current_input != saved_nick;

        let field = text_input("Ex : Casque, Enceintes…", current_input)
            .on_input({
                let device = sink.name.clone();
                move |text| Message::AudioNickChanged(device.clone(), text)
            })
            .on_submit(Message::AudioNickSubmit(sink.name.clone()))
            .size(13)
            .font(style::MEDIUM)
            .width(200)
            .padding(Padding::from([7.0, 10.0]))
            .style(move |_, status| {
                let line = match status {
                    text_input::Status::Focused { .. } => palette.accent,
                    _ => palette.rim,
                };
                text_input::Style {
                    background: palette.card.into(),
                    border: Border {
                        color: line,
                        width: 1.0,
                        radius: 8.0.into(),
                    },
                    icon: palette.muted,
                    placeholder: palette.muted,
                    value: palette.text,
                    selection: style::alpha(palette.accent, 0.35),
                }
            });

        let mut control = row![field].spacing(8).align_y(Alignment::Center);
        if is_dirty {
            let save_btn = button(
                text("Enregistrer")
                    .size(12)
                    .font(style::MEDIUM)
                    .color(palette.on_accent()),
            )
            .padding(Padding::from([6.0, 12.0]))
            .on_press(Message::AudioNickSubmit(sink.name.clone()))
            .style(style::tinted(palette.accent, palette.accent, 6.0));
            control = control.push(save_btn);
        } else if !saved_nick.is_empty() {
            let reset_btn = button(
                text("Effacer")
                    .size(12)
                    .font(style::MEDIUM)
                    .color(palette.muted),
            )
            .padding(Padding::from([6.0, 10.0]))
            .on_press(Message::Change(Change::AudioNick {
                device: sink.name.clone(),
                nick: None,
            }))
            .style(style::tinted(palette.card, palette.hover, 6.0));
            control = control.push(reset_btn);
        }

        let nick_setting = setting(
            "Surnom personnalisé",
            "Nom court affiché dans le sélecteur rapide (Appuyer sur Entrée pour valider).",
            None,
            control,
            palette,
        );

        let rows = vec![visibility, nick_setting];
        content = content.push(column![title_block, card(rows, palette)].spacing(8));
    }

    content.into()
}

fn game<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    let sticky = app.file.sticky_edges_ms() > 0;
    let remembered = u64::from(app.sticky);
    let mut edges = vec![setting(
        "Bords collants",
        "Pour sortir de l'écran d'un jeu, il faut pousser le curseur contre le bord un court \
         instant.",
        None,
        switch(
            sticky,
            move |on| Message::Change(Change::StickyEdges(if on { remembered } else { 0 })),
            palette,
        ),
        palette,
    )];
    if sticky {
        let delay = row![
            level(app.sticky, palette),
            text(format!("{} ms", app.sticky))
                .size(13)
                .font(style::MEDIUM)
                .color(palette.text)
                .width(58)
                .align_x(Alignment::End)
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        edges.push(setting(
            "Délai",
            "Le temps de poussée avant de passer.",
            None,
            delay,
            palette,
        ));
    }
    let display = vec![
        setting(
            "Affichage direct",
            "L'image du jeu part telle quelle à l'écran, sans être redessinée. À couper en cas \
             de souci d'affichage en jeu.",
            None,
            switch(
                app.file.direct_scanout(),
                |on| Message::Change(Change::DirectScanout(on)),
                palette,
            ),
            palette,
        ),
        setting(
            "Fréquence variable (VRR)",
            "G-Sync ou FreeSync sur l'écran du jeu, si l'écran et le pilote le proposent. À \
             couper en cas de scintillement.",
            None,
            switch(
                app.file.vrr(),
                |on| Message::Change(Change::Vrr(on)),
                palette,
            ),
            palette,
        ),
    ];
    column![
        section("Curseur", card(edges, palette), palette),
        section("Affichage", card(display, palette), palette)
    ]
    .spacing(18)
    .into()
}

fn mouse<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    let speed_label = match app.mouse_speed {
        0 => "Par défaut".to_string(),
        val if val > 0 => format!("+{val}"),
        val => format!("{val}"),
    };
    let speed = row![
        text("Lente").size(12).color(palette.muted),
        slider(
            model::MOUSE_SPEED_RANGE,
            app.mouse_speed,
            Message::MouseSpeed,
        )
        .step(model::MOUSE_SPEED_STEP)
        .on_release(Message::MouseSpeedReleased)
        .width(180)
        .style(move |_, _| slider_style(palette)),
        text("Rapide").size(12).color(palette.muted),
        text(speed_label)
            .size(13)
            .font(style::MEDIUM)
            .color(palette.text)
            .width(78)
            .align_x(Alignment::End),
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    let profiles = [
        (MouseAccelProfile::Adaptive, "Adaptatif"),
        (MouseAccelProfile::Flat, "Plat"),
    ];
    let profile = segmented(
        &profiles,
        app.file.mouse_accel_profile(),
        |profile| Message::Change(Change::MouseAccelProfile(profile)),
        palette,
    );

    let pointer = vec![
        setting(
            "Vitesse du pointeur",
            "La vitesse de déplacement du curseur à l'écran.",
            None,
            speed,
            palette,
        ),
        setting(
            "Profil d'accélération",
            "L'accélération adaptative ajuste la vitesse au geste ; le profil plat garde une \
             vitesse constante.",
            None,
            profile,
            palette,
        ),
    ];

    let buttons_and_scroll = vec![
        setting(
            "Défilement naturel",
            "Le contenu suit la direction des doigts sur la molette.",
            None,
            switch(
                app.file.mouse_natural_scroll(),
                |on| Message::Change(Change::MouseNaturalScroll(on)),
                palette,
            ),
            palette,
        ),
        setting(
            "Inverser les boutons",
            "Permute le clic gauche et le clic droit pour les gauchers.",
            None,
            switch(
                app.file.mouse_left_handed(),
                |on| Message::Change(Change::MouseLeftHanded(on)),
                palette,
            ),
            palette,
        ),
    ];

    column![
        section("Pointeur", card(pointer, palette), palette),
        section(
            "Boutons et défilement",
            card(buttons_and_scroll, palette),
            palette
        ),
    ]
    .spacing(18)
    .into()
}

fn system<'a>(app: &'a App, palette: Palette) -> Element<'a, Message> {
    let file = &app.file;
    let layouts = app.layouts.layouts(file.keyboard_layout.as_deref());
    let layout = model::chosen(&layouts, file.keyboard_layout.as_deref());
    let mut keyboard = vec![setting(
        "Disposition",
        "Sans choix, celle que le système donne au clavier.",
        None,
        picker(layouts, layout, Message::Layout, 280.0, palette),
        palette,
    )];
    if let Some(name) = &file.keyboard_layout {
        let variants = app.layouts.variants(name, file.keyboard_variant.as_deref());
        let variant = model::chosen(&variants, file.keyboard_variant.as_deref());
        keyboard.push(setting(
            "Variante",
            "",
            None,
            picker(variants, variant, Message::Variant, 280.0, palette),
            palette,
        ));
    }

    let installed = |name: &str| app.terminals.contains(&name);
    let terminals = model::terminals(installed, file.terminal.as_deref());
    let terminal = model::chosen(&terminals, file.terminal.as_deref());
    let applications = vec![
        setting(
            "Terminal",
            "Celui qu'ouvre Super+Entrée.",
            None,
            picker(terminals, terminal, Message::Terminal, 280.0, palette),
            palette,
        ),
        setting(
            "Applications X11 (XWayland)",
            "Beaucoup de jeux en ont besoin, par Steam et Proton.",
            Some("Au prochain démarrage"),
            switch(
                file.xwayland(),
                |on| Message::Change(Change::Xwayland(on)),
                palette,
            ),
            palette,
        ),
    ];

    let path = model::short_path(&app.options.config, std::env::var_os("HOME").as_deref());
    let file_note = column![
        text(format!("Ces réglages sont enregistrés dans {path}"))
            .size(12)
            .color(palette.muted),
        text(
            "Les favoris du dock et la taille de son lanceur se règlent à la souris, dans le dock."
        )
        .size(12)
        .color(palette.muted)
    ]
    .spacing(4);
    column![
        section("Clavier", card(keyboard, palette), palette),
        section("Applications", card(applications, palette), palette),
        file_note
    ]
    .spacing(18)
    .into()
}

// ---- The shapes every page shares ----

/// A card under its title.
fn section<'a>(
    title: &'a str,
    card: Element<'a, Message>,
    palette: Palette,
) -> Element<'a, Message> {
    let title = text(title)
        .size(13)
        .font(style::SEMIBOLD)
        .color(palette.muted);
    column![title, card].spacing(8).into()
}

/// Settings one under the other, a hairline between them.
fn card<'a>(rows: Vec<Element<'a, Message>>, palette: Palette) -> Element<'a, Message> {
    let mut lines: Column<'_, Message> = column![];
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            let rule = container(space().width(Length::Fill).height(1))
                .style(move |_| style::fill(palette.separator, 0.0));
            lines = lines.push(rule);
        }
        lines = lines.push(container(row).padding(Padding::from([14.0, 16.0])));
    }
    container(lines)
        .width(Length::Fill)
        .style(move |_| style::outlined(palette.card, palette.rim, 10.0))
        .into()
}

/// The name of a setting and what it does.
fn words<'a>(title: &'a str, detail: &'a str, palette: Palette) -> Column<'a, Message> {
    let mut words =
        column![text(title).size(14).font(style::MEDIUM).color(palette.text)].spacing(3);
    if !detail.is_empty() {
        words = words.push(text(detail).size(12).color(palette.muted));
    }
    words
}

/// A setting: its words on the left, what changes it on the right. `tag`
/// says when the change takes effect, when that is not at once.
fn setting<'a>(
    title: &'a str,
    detail: &'a str,
    tag: Option<&'a str>,
    control: impl Into<Element<'a, Message>>,
    palette: Palette,
) -> Element<'a, Message> {
    let mut words = words(title, detail, palette);
    if let Some(tag) = tag {
        let pill = container(text(tag).size(11).font(style::MEDIUM).color(palette.muted))
            .padding(Padding::from([2.0, 7.0]))
            .style(move |_| style::fill(palette.card, 6.0));
        words = words.push(container(pill).padding(Padding::from([3.0, 0.0])));
    }
    row![words.width(Length::Fill), control.into()]
        .spacing(18)
        .align_y(Alignment::Center)
        .into()
}

fn switch<'a>(
    on: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
    palette: Palette,
) -> Element<'a, Message> {
    toggler(on)
        .on_toggle(on_toggle)
        .size(22)
        .style(move |_, status| {
            let (toggled, hovered) = match status {
                toggler::Status::Hovered { is_toggled } => (is_toggled, true),
                toggler::Status::Active { is_toggled }
                | toggler::Status::Disabled { is_toggled } => (is_toggled, false),
            };
            let track = if toggled {
                palette.accent
            } else {
                style::alpha(palette.muted, 0.45)
            };
            let track = if hovered {
                style::mix(track, palette.text, 0.14)
            } else {
                track
            };
            let (knob, knob_border, knob_border_width) = if toggled {
                (palette.on_accent(), Color::TRANSPARENT, 0.0)
            } else if palette.dark {
                (palette.glass_strong, palette.rim, 1.0)
            } else {
                (Color::WHITE, palette.rim, 1.0)
            };
            toggler::Style {
                background: track.into(),
                background_border_width: 0.0,
                background_border_color: Color::TRANSPARENT,
                foreground: knob.into(),
                foreground_border_width: knob_border_width,
                foreground_border_color: knob_border,
                text_color: None,
                border_radius: None,
                padding_ratio: 0.14,
            }
        })
        .into()
}

/// A few choices side by side, one of them taken.
fn segmented<'a, T: Copy + PartialEq>(
    options: &[(T, &'a str)],
    chosen: T,
    on_pick: impl Fn(T) -> Message,
    palette: Palette,
) -> Element<'a, Message> {
    let mut line: Row<'_, Message> = row![].spacing(2);
    for (value, name) in options {
        let taken = *value == chosen;
        let (idle, hover, color) = if taken {
            (palette.accent, palette.accent, palette.on_accent())
        } else {
            (Color::TRANSPARENT, palette.hover, palette.text)
        };
        let choice = button(text(*name).size(13).font(style::MEDIUM).color(color))
            .padding(Padding::from([6.0, 12.0]))
            .on_press(on_pick(*value))
            .style(style::tinted(idle, hover, 6.0));
        line = line.push(choice);
    }
    container(line)
        .padding(3)
        .style(move |_| style::fill(palette.card, 9.0))
        .into()
}

/// A list that unfolds.
fn picker<'a>(
    choices: Vec<Choice>,
    chosen: Option<Choice>,
    on_pick: impl Fn(Choice) -> Message + 'a,
    width: f32,
    palette: Palette,
) -> Element<'a, Message> {
    pick_list(choices, chosen, on_pick)
        .placeholder("Choisir…")
        .width(width)
        .padding(Padding::from([8.0, 12.0]))
        .text_size(13)
        .font(style::MEDIUM)
        .style(move |_, status| {
            let line = match status {
                pick_list::Status::Active => palette.rim,
                pick_list::Status::Hovered | pick_list::Status::Opened { .. } => palette.accent,
            };
            pick_list::Style {
                text_color: palette.text,
                placeholder_color: palette.muted,
                handle_color: palette.muted,
                background: palette.card.into(),
                border: Border {
                    color: line,
                    width: 1.0,
                    radius: 8.0.into(),
                },
            }
        })
        .menu_style(move |_| menu::Style {
            background: palette.glass_strong.into(),
            border: Border {
                color: palette.rim,
                width: 1.0,
                radius: 8.0.into(),
            },
            text_color: palette.text,
            selected_text_color: palette.on_accent(),
            selected_background: palette.accent.into(),
            shadow: palette.shadow(),
        })
        .into()
}

/// The slider of the sticky edges: written when it is let go.
fn level<'a>(delay: u32, palette: Palette) -> Element<'a, Message> {
    slider(model::STICKY_RANGE, delay, Message::Sticky)
        .step(model::STICKY_STEP)
        .on_release(Message::StickyReleased)
        .width(200)
        .style(move |_, _| slider_style(palette))
        .into()
}

/// A slider filled with the accent up to its handle.
fn slider_style(palette: Palette) -> slider::Style {
    let knob_bg = if palette.dark {
        palette.glass_strong
    } else {
        Color::WHITE
    };
    slider::Style {
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
    }
}
