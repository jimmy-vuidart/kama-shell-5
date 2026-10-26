//! What the dialog looks like: a card of glass in the middle of the
//! screen, over a light veil. Title, what the action does, the account,
//! the password field, a message under it, « Annuler » and
//! « Authentifier ».

use coquille_ui::glyph::{Glyph, glyph};
use coquille_ui::style::{self, Palette};
use iced::widget::{Column, button, column, container, row, space, text, text_input};
use iced::{Alignment, Border, Color, Element, Length, Padding};

use crate::agent::{Dialog, Note, Phase};
use crate::app::{FIELD, Message, Prompt};
use crate::helper::Password;

const WIDTH: f32 = 440.0;
const RADIUS: f32 = 18.0;

pub fn dialog<'a>(prompt: &'a Prompt, dialog: &'a Dialog) -> Element<'a, Message> {
    let palette = prompt.palette();
    let busy = dialog.busy();

    let badge = container(glyph(Glyph::Lock, 22.0, palette.accent))
        .padding(11)
        .style(move |_| style::fill(style::alpha(palette.accent, 0.16), 14.0));
    let title = text("Authentification requise")
        .size(17)
        .font(style::SEMIBOLD)
        .color(palette.text);
    let action = text(&dialog.action_id).size(12).color(palette.muted);
    let header = row![badge, column![title, action].spacing(3)]
        .spacing(14)
        .align_y(Alignment::Center);
    let message = text(&dialog.message).size(14).color(palette.text);

    let mut body = Column::new()
        .push(header)
        .push(message)
        .push(accounts(dialog, palette, busy))
        .push(field(prompt.password(), dialog, palette, busy))
        .spacing(16);
    if let Some(note) = &dialog.note {
        let (line, color) = match note {
            Note::Error(line) => (line, style::DANGER),
            Note::Info(line) => (line, palette.muted),
        };
        body = body.push(text(line).size(13).color(color));
    }
    body = body.push(answers(palette, busy));
    if dialog.waiting > 0 {
        let waiting = match dialog.waiting {
            1 => "Une autre demande attend son tour".to_owned(),
            count => format!("{count} autres demandes attendent leur tour"),
        };
        body = body.push(text(waiting).size(12).color(palette.muted));
    }

    let card = container(body)
        .width(WIDTH)
        .padding(24)
        .style(move |_| container::Style {
            shadow: palette.shadow(),
            ..style::outlined(palette.glass_strong, palette.rim, RADIUS)
        });
    container(card)
        .center(Length::Fill)
        .style(move |_| style::fill(palette.scrim, 0.0))
        .into()
}

/// The account that authenticates: named when there is one, to choose
/// among several otherwise.
fn accounts<'a>(dialog: &'a Dialog, palette: Palette, busy: bool) -> Element<'a, Message> {
    if let [only] = dialog.users.as_slice() {
        return text(format!("Compte : {only}"))
            .size(13)
            .color(palette.muted)
            .into();
    }
    let choice = |index: usize, label: &'a String| {
        let chosen = index == dialog.user;
        let dot = container(space().width(8).height(8))
            .padding(3)
            .style(move |_| container::Style {
                border: Border {
                    color: if chosen {
                        palette.accent
                    } else {
                        palette.muted
                    },
                    width: 1.5,
                    radius: 8.0.into(),
                },
                ..style::fill(
                    if chosen {
                        palette.accent
                    } else {
                        Color::TRANSPARENT
                    },
                    8.0,
                )
            });
        let line = row![dot, text(label).size(13).color(palette.text)]
            .spacing(10)
            .align_y(Alignment::Center);
        let idle = if chosen {
            palette.card
        } else {
            Color::TRANSPARENT
        };
        button(line)
            .width(Length::Fill)
            .padding(Padding::from([8.0, 12.0]))
            .on_press_maybe((!busy && !chosen).then_some(Message::Choose(index)))
            .style(style::tinted(idle, palette.hover, 10.0))
            .into()
    };
    let title = text("S'authentifier en tant que")
        .size(13)
        .color(palette.muted);
    let choices = dialog
        .users
        .iter()
        .enumerate()
        .map(|(index, label)| choice(index, label));
    column![title, Column::with_children(choices).spacing(2)]
        .spacing(6)
        .into()
}

fn field<'a>(
    password: &Password,
    dialog: &Dialog,
    palette: Palette,
    busy: bool,
) -> Element<'a, Message> {
    let (placeholder, echo) = match &dialog.phase {
        Phase::Asking { prompt, echo } => (placeholder(prompt), *echo),
        _ => ("Mot de passe".to_owned(), false),
    };
    let input = text_input(&placeholder, password.as_str())
        .id(FIELD)
        .secure(!echo)
        .on_input_maybe((!busy).then_some(|typed| Message::Typed(Password::new(typed))))
        .on_submit(Message::Submit)
        .size(14)
        .padding(Padding::from([11.0, 14.0]))
        .style(move |_, status| text_input::Style {
            background: palette.card.into(),
            border: Border {
                color: match status {
                    text_input::Status::Focused { .. } => palette.accent,
                    _ => palette.separator,
                },
                width: 1.5,
                radius: 12.0.into(),
            },
            icon: palette.muted,
            placeholder: palette.muted,
            value: palette.text,
            selection: style::alpha(palette.accent, 0.35),
        });
    input.into()
}

/// What PAM asks, as the field says it: its usual question in French.
fn placeholder(prompt: &str) -> String {
    let prompt = prompt.trim().trim_end_matches(':').trim_end();
    if prompt.is_empty() || prompt.eq_ignore_ascii_case("password") {
        "Mot de passe".to_owned()
    } else {
        prompt.to_owned()
    }
}

fn answers<'a>(palette: Palette, busy: bool) -> Element<'a, Message> {
    let cancel = button(
        text("Annuler")
            .size(13)
            .font(style::MEDIUM)
            .color(palette.text),
    )
    .padding(Padding::from([10.0, 16.0]))
    .on_press(Message::Cancel)
    .style(style::tinted(palette.card, palette.hover, 10.0));
    let verb = if busy {
        "Vérification…"
    } else {
        "Authentifier"
    };
    let accent = palette.accent;
    let confirm = button(
        text(verb)
            .size(13)
            .font(style::SEMIBOLD)
            .color(palette.on_accent()),
    )
    .padding(Padding::from([10.0, 16.0]))
    .on_press_maybe((!busy).then_some(Message::Submit))
    .style(style::tinted(
        accent,
        style::mix(accent, Color::BLACK, 0.12),
        10.0,
    ));
    row![space().width(Length::Fill), cancel, confirm]
        .spacing(10)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_usual_question_of_pam_is_asked_in_french() {
        assert_eq!(placeholder("Password: "), "Mot de passe");
        assert_eq!(placeholder(""), "Mot de passe");
        assert_eq!(placeholder("Mot de passe : "), "Mot de passe");
        assert_eq!(placeholder("Code PIN :"), "Code PIN");
    }
}
