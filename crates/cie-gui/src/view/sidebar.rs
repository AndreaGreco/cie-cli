//! The column of cards: what is being signed, how, and where the box goes.

use iced::widget::{
    button, checkbox, column, container, pick_list, radio, row, scrollable, text, text_input,
};
use iced::{Alignment, Element, Fill};
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
        sections = sections.push(template(state));
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

fn kind(state: &State) -> Element<'_, Message> {
    let choice = |k: Kind, label: std::borrow::Cow<'_, str>| {
        radio(label.into_owned(), k, Some(state.kind), Message::KindChanged).size(15)
    };

    let mut body = column![].spacing(10);

    // PAdES only exists for a PDF we could actually open; offering it otherwise
    // would be a button that fails on press.
    if state.document.is_some() {
        body = body.push(choice(Kind::Pades, t!("kind.pades")));
    } else {
        body = body.push(
            text(t!("kind.pades_needs_pdf"))
                .size(12)
                .style(iced::widget::text::secondary),
        );
    }
    body = body
        .push(choice(Kind::P7m, t!("kind.p7m")))
        .push(choice(Kind::Detached, t!("kind.detached")));

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

fn stamp(state: &State) -> Element<'_, Message> {
    let mut body = column![
        row![
            text(t!("stamp.reason")).size(13),
            text_input("", &state.reason)
                .on_input(Message::ReasonChanged)
                .style(theme::field)
                .size(14),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        checkbox(state.certify.is_some())
            .label(t!("stamp.certify").into_owned())
            .on_toggle(Message::CertifyToggled)
            .size(15)
            .text_size(14),
    ]
    .spacing(10);

    if let Some(level) = state.certify {
        body = body.push(
            pick_list(LEVELS, Some(Level(level)), |l| Message::CertifyLevel(l.0))
                .text_size(13)
                .padding(6),
        );
    }

    body = body
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

/// Which composed box signs this document, and the way into the editor.
///
/// The composing itself is not here. It needs room to drag things around in,
/// and a sidebar 340 pixels wide is not room.
fn template(state: &State) -> Element<'_, Message> {
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

    // A template whose picture has gone missing still signs — without the
    // picture. Saying so here is the only warning before it happens.
    if state.settings.active().holds(&Piece::Image) && state.signature.is_none() {
        body = body.push(
            text(t!("template.picture_missing"))
                .size(11)
                .style(iced::widget::text::danger),
        );
    }

    section(t!("template.section"), body)
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
