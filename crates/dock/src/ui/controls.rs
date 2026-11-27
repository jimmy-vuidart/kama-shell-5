//! Reusable control center, notifications history, and per-app audio mixer
//! views shared between the classic launcher drawer and spatial popovers.

use iced::widget::text::Wrapping;
use iced::widget::{
    Column, Row, button, column, container, mouse_area, pick_list, progress_bar, row, scrollable,
    slider, space, stack, text,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, mouse};

use super::icons::{self, Glyph};
use super::style::{self, Palette};
use crate::app::{Dock, Message, OutputChoice, Toggle};
use crate::backend::{SessionAction, Switch};
use crate::model::Side;
use crate::model::mixer;
use crate::model::notify::{self, Notice};
use coquille_ipc::ScreenshotTarget;

pub const MIXER_WIDTH: f32 = 292.0;

/// How many lines of its body a notice shows.
const BODY_LINES: f32 = 2.0;
/// How many of its actions a notice shows as buttons.
const ACTIONS: usize = 2;
/// The switches of the control center go two by two.
const SWITCHES_PER_ROW: usize = 2;

/// What the control center needs beyond `Dock`'s own state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlView<'a> {
    /// Which side of the screen the parent bar sits on (determines chevron direction).
    pub side: Side,
    /// Show the chevron button beside volume that opens the floating mixer panel.
    pub show_mixer_toggle: bool,
    pub mixer_open: bool,
    pub confirm: Option<SessionAction>,
    pub screenshot_prompt: bool,
    pub note: Option<&'a str>,
}

pub fn rule<'a>(palette: Palette) -> Element<'a, Message> {
    container(space().width(Length::Fill).height(1))
        .style(move |_| style::fill(palette.separator, 0.0))
        .into()
}

pub fn heading<'a>(title: &'a str, detail: String, palette: Palette) -> Row<'a, Message> {
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

/// The control center: switches two by two, the sliders one under the other,
/// then the session.
pub fn control_center<'a>(
    dock: &'a Dock,
    view: ControlView<'a>,
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
    if view.show_mixer_toggle {
        let (mixer_idle, mixer_fg) = if view.mixer_open {
            (palette.accent, palette.on_accent())
        } else {
            (palette.card, palette.text)
        };
        // The mixer opens beside the drawer, away from the bar: the arrow
        // points to it.
        let towards = match view.side {
            Side::Right => Glyph::ChevronLeft,
            Side::Left => Glyph::ChevronRight,
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
        volume = volume.push(mixer_toggle);
    }
    let light = row![
        container(icons::glyph(Glyph::Sun, 18.0, palette.text)).center_x(30),
        brightness,
        screens_pill
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let foot = match (view.confirm, view.screenshot_prompt, view.note) {
        (Some(action), _, _) => confirmation(action, palette),
        (None, true, _) => screenshot_selector(palette),
        (None, false, note) => session(dock, note, palette),
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
pub fn notifications<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
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
pub fn mixer_card<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
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
pub fn mixer_panel<'a>(dock: &'a Dock, palette: Palette) -> Element<'a, Message> {
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
        .width(MIXER_WIDTH)
        .padding(16)
        .style(move |_| container::Style {
            shadow: palette.shadow(),
            ..style::outlined(palette.glass_strong, palette.rim, 16.0)
        })
        .into()
}
