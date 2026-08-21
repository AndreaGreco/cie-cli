//! The column of cards: what is being signed, how, and where the box goes.

use iced::widget::{
    button, checkbox, column, container, pick_list, radio, row, scrollable, space, text,
    text_input, tooltip, MouseArea,
};
use iced::{Alignment, Border, Color, Element, Fill, Length};
use rust_i18n::t;

use cie_firma::Piece;

use crate::job::Kind;
use crate::state::{Message, State};
use crate::theme;

use super::SIDEBAR;

pub fn view(state: &State) -> Element<'_, Message> {
    let mut sections = column![document(state), kind(state)].spacing(18);

    if state.shows_preview() {
        sections = sections.push(stamp(state));
        sections = sections.push(certification(state));
    }
    if state.card.stored.is_some() {
        sections = sections.push(card(state));
    }
    if let Some(notice) = &state.notice {
        sections = sections.push(text(notice).size(12).style(iced::widget::text::secondary));
    }

    container(scrollable(sections.padding(16)))
        .width(SIDEBAR)
        .height(Fill)
        .style(theme::bar)
        .into()
}

/// A titled card. Every section in the column is one of these, which is what
/// makes the column read as a list of decisions instead of a wall of controls.
fn section<'a>(
    title: std::borrow::Cow<'a, str>,
    body: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    column![
        text(title.into_owned()).size(12).font(theme::UI_STRONG),
        container(body.into())
            .style(theme::card)
            .padding(12)
            .width(Fill),
    ]
    .spacing(6)
    .into()
}

fn document(state: &State) -> Element<'_, Message> {
    let body: Element<'_, Message> = match (&state.path, &state.bytes) {
        (Some(path), Some(bytes)) => {
            let mut lines = column![
                text(path.file_name().unwrap_or_default().to_string_lossy().into_owned()).size(14),
                text(size_and_pages(bytes.len(), state.page_count()))
                    .size(12)
                    .style(iced::widget::text::secondary),
            ]
            .spacing(4);

            if let Some(e) = &state.open_error {
                lines = lines.push(text(e).size(12).style(iced::widget::text::danger));
            }

            column![
                lines,
                button(text(t!("document.change")).size(13))
                    .style(theme::quiet)
                    .on_press(Message::ChooseDocument),
            ]
            .spacing(8)
            .into()
        }
        _ => button(text(t!("document.choose")))
            .style(theme::secondary)
            .on_press(Message::ChooseDocument)
            .into(),
    };

    section(t!("document.section"), body)
}

fn size_and_pages(bytes: usize, pages: usize) -> String {
    let size = if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    };
    match pages {
        0 => size,
        1 => format!("{size} · {}", t!("document.one_page")),
        n => format!("{size} · {}", t!("document.pages", count = n)),
    }
}

/// The circle of a radio, and the gap between it and its label.
///
/// Named because the second line has to start under the first, and the only
/// way to know where the first one starts is to have said how wide the circle
/// and the gap are.
const RADIO: f32 = 15.0;
const RADIO_GAP: f32 = 8.0;

fn kind(state: &State) -> Element<'_, Message> {
    // The name on one line and what it means on the next, quieter and smaller.
    // On one line the three of them read as a wall of prose in a column 340
    // pixels wide; split, the eye picks the three names out first and reads
    // the rest only if it needs to.
    let choice = |k: Kind,
                  name: std::borrow::Cow<'_, str>,
                  what: std::borrow::Cow<'_, str>| {
        let block = column![
            radio(name.into_owned(), k, Some(state.kind), Message::KindChanged)
                .size(RADIO)
                .spacing(RADIO_GAP)
                .text_size(14),
            row![
                space::horizontal().width(RADIO + RADIO_GAP),
                text(what.into_owned())
                    .size(11)
                    .style(iced::widget::text::secondary),
            ],
        ]
        .spacing(1);

        // The whole block answers to a click, not just the circle and the
        // name: a line of text sitting under a radio that does nothing when
        // you press it reads as broken.
        MouseArea::new(block)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_press(Message::KindChanged(k))
    };

    let mut body = column![].spacing(12);

    // PAdES only exists for a PDF we could actually open; offering it otherwise
    // would be a button that fails on press.
    if state.document.is_some() {
        body = body.push(choice(Kind::Pades, t!("kind.pades"), t!("kind.pades_what")));
    } else {
        body = body.push(
            text(t!("kind.pades_needs_pdf"))
                .size(12)
                .style(iced::widget::text::secondary),
        );
    }
    body = body
        .push(choice(Kind::P7m, t!("kind.p7m"), t!("kind.p7m_what")))
        .push(choice(
            Kind::Detached,
            t!("kind.detached"),
            t!("kind.detached_what"),
        ));

    section(t!("kind.section"), body)
}

/// `pick_list` needs `Display`, and a bare `u8` would offer "1 2 3" instead of
/// saying what each level allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Level(u8);

const LEVELS: [Level; 3] = [Level(1), Level(2), Level(3)];

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            1 => t!("stamp.certify_1"),
            3 => t!("stamp.certify_3"),
            _ => t!("stamp.certify_2"),
        })
    }
}

/// The box that is about to be signed on to this document: which one it is,
/// what it will say, and where it sits.
///
/// The template picker is the first thing in it. It used to be a card of its
/// own next to this one, which put the box and the box's contents on screen as
/// two decisions instead of the one they are: the template *is* what is in
/// that rectangle, and choosing another one changes what the preview draws
/// inside it a foot away.
///
/// The composing itself is still not here. It needs room to drag things around
/// in, and a sidebar 340 pixels wide is not room.
fn stamp(state: &State) -> Element<'_, Message> {
    let choices: Vec<Choice> = state
        .settings
        .templates
        .iter()
        .enumerate()
        .map(|(index, t)| Choice {
            index,
            name: t.name.clone(),
        })
        .collect();
    let chosen = choices.get(state.settings.active).cloned();

    let mut body = column![
        pick_list(choices, chosen, |c| Message::TemplateChosen(c.index))
            .text_size(13)
            .padding(6)
            .width(Fill),
        button(text(t!("template.edit")).size(13))
            .style(theme::secondary)
            .on_press(Message::ShowTemplates),
    ]
    .spacing(10);

    // A template whose picture has gone missing still signs - without the
    // picture. Saying so here is the only warning before it happens.
    if state.settings.active().holds(&Piece::Image) && state.signature.is_none() {
        body = body.push(
            text(t!("template.picture_missing"))
                .size(11)
                .style(iced::widget::text::danger),
        );
    }

    body = body
        .push(rule())
        .push(
            row![
                text(t!("stamp.reason")).size(13),
                text_input("", &state.reason)
                    .on_input(Message::ReasonChanged)
                    .style(theme::field)
                    .size(14),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .push(
            button(text(t!("stamp.reset")).size(13))
                .style(theme::quiet)
                .on_press(Message::StampReset),
        )
        .push(
            text(t!("stamp.facsimile"))
                .size(11)
                .style(iced::widget::text::secondary),
        );

    section(t!("stamp.section"), body)
}

/// A `?` that says what a setting is, a moment after the pointer stops on it.
///
/// A caption rather than a line of prose under the control. What certification
/// is takes three lines to explain, and three lines sitting there for good
/// would crowd out the one decision the card is asking for - while a setting
/// nobody can find out the meaning of is a setting nobody touches.
fn explain<'a>(what: std::borrow::Cow<'a, str>) -> Element<'a, Message> {
    let badge = container(
        text("?")
            .size(11)
            .font(theme::UI_STRONG)
            .style(iced::widget::text::secondary),
    )
    .center(Length::Fixed(17.0))
    .style(|t: &iced::Theme| container::Style {
        border: Border {
            // Round: a circle is the shape this means everywhere, and a square
            // one reads as a button that does something when pressed.
            radius: 999.0.into(),
            width: 1.0,
            color: Color {
                a: 0.35,
                ..t.extended_palette().background.base.text
            },
        },
        ..Default::default()
    });

    tooltip(
        badge,
        container(text(what.into_owned()).size(12))
            .style(theme::floating)
            .padding([7, 10])
            .max_width(280.0),
        // Left, not below: the badge sits at the right edge of a sidebar that
        // is itself at the right edge of the window, and a caption dropped
        // below it would be a caption half off the screen.
        tooltip::Position::Left,
    )
    .into()
}

/// A hairline across a card, between two things that belong in it for
/// different reasons.
fn rule<'a>() -> Element<'a, Message> {
    container(space::vertical().height(1))
        .width(Fill)
        .style(|t: &iced::Theme| container::Style {
            background: Some(iced::Background::Color(iced::Color {
                a: 0.15,
                ..t.extended_palette().background.base.text
            })),
            ..Default::default()
        })
        .into()
}

/// Certification, which is not about the box at all.
///
/// It rode along in the box's card for as long as the box's card was where the
/// PAdES-only settings went. It is a permission written into the document -
/// what anybody may still change after this signature - and it is no more part
/// of the rectangle than the file name is.
fn certification(state: &State) -> Element<'_, Message> {
    let mut body = column![
        row![
            checkbox(state.certify.is_some())
                .label(t!("stamp.certify").into_owned())
                .on_toggle(Message::CertifyToggled)
                .size(15)
                .text_size(14),
            space::horizontal(),
            explain(t!("stamp.certify_what")),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(10);

    if let Some(level) = state.certify {
        body = body.push(
            pick_list(LEVELS, Some(Level(level)), |l| Message::CertifyLevel(l.0))
                .text_size(13)
                .padding(6),
        );
    }

    section(t!("stamp.certify_section"), body)
}

/// `pick_list` needs `Display`, and a `Template` has a name but no opinion
/// about how a list should show it.
#[derive(Debug, Clone, PartialEq)]
struct Choice {
    index: usize,
    name: String,
}

impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// Only shown once the keyring knows this card: an empty "no PIN saved" card
/// would be a section that exists to say nothing.
fn card(state: &State) -> Element<'_, Message> {
    let stored = state.card.stored.as_ref().expect("checked by the caller");
    let who = stored
        .holder()
        .unwrap_or_else(|| t!("keyring.stored").into_owned());

    section(
        t!("keyring.stored"),
        column![
            text(who).size(13),
            button(text(t!("keyring.forget")).size(13))
                .style(theme::quiet)
                .on_press(Message::ForgetCard),
        ]
        .spacing(6),
    )
}
