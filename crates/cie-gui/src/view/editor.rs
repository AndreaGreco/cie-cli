//! The template editor: the palette, the box, and the piece under the pointer.
//!
//! There is no OK and no Annulla. What is dragged is saved as it is dragged,
//! the way the rest of the settings in this program behave, and the way an
//! editor with a live preview has to behave to be worth having one.

use iced::widget::{
    button, checkbox, column, container, pick_list, row, space, text, text_input,
};
use iced::{Alignment, Element, Fill, Length};
use rust_i18n::t;

use cie_firma::{Align, Facts, Piece};

use crate::state::{Message, State};
use crate::theme;

const PANEL_W: f32 = 900.0;
const PANEL_H: f32 = 620.0;
const PALETTE: f32 = 170.0;

/// The signature box when nothing better is known — the same rectangle
/// `cie sign-pdf` uses from the command line. Editing with a document open
/// uses that document's box instead, so what is composed has the shape and the
/// scale it will really have.
const DEFAULT_BOX_PT: (f32, f32) = (230.0, 70.0);

pub fn panel(state: &State, selected: Option<usize>) -> Element<'_, Message> {
    let body = column![
        header(state),
        row![
            palette(),
            column![
                container(board(state, selected))
                    .style(theme::desk)
                    .padding(16)
                    .width(Fill)
                    .height(Fill),
                properties(state, selected),
            ]
            .spacing(12)
            .width(Fill),
        ]
        .spacing(16)
        .height(Fill),
        footer(state),
    ]
    .spacing(14);

    super::over(
        container(body)
            .style(theme::card)
            .padding(20)
            .width(Length::Fixed(PANEL_W))
            .height(Length::Fixed(PANEL_H))
            .into(),
    )
}

/// Which template, and what to do to it as a whole.
fn header(state: &State) -> Element<'_, Message> {
    let choices: Vec<Named> = state
        .settings
        .templates
        .iter()
        .enumerate()
        .map(|(index, t)| Named {
            index,
            name: t.name.clone(),
        })
        .collect();
    let chosen = choices.get(state.settings.active).cloned();
    // The last template never goes: with none there is nothing to sign with.
    let removable = state.settings.templates.len() > 1;

    column![
        text(t!("template.title")).size(17).font(theme::UI_STRONG),
        row![
            pick_list(choices, chosen, |c| Message::TemplateChosen(c.index))
                .text_size(13)
                .padding(6)
                .width(Length::Fixed(220.0)),
            text_input("", &state.settings.active().name)
                .on_input(Message::TemplateRenamed)
                .style(theme::field)
                .size(13)
                .padding(7)
                .width(Length::Fixed(220.0)),
            space::horizontal(),
            small(t!("template.add"), Some(Message::TemplateAdded)),
            small(t!("template.duplicate"), Some(Message::TemplateDuplicated)),
            small(
                t!("template.delete"),
                removable.then_some(Message::TemplateDeleted)
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(10)
    .into()
}

/// The pieces you can put in the box.
///
/// Clicking one drops it in the middle of the box, selected, ready to be
/// dragged where it belongs. Dragging it out of this list would be the
/// prettier gesture and a far more fragile one: iced has no drag source, and a
/// hand-rolled one would have to guess at every widget it passed over.
fn palette<'a>() -> Element<'a, Message> {
    let piece = |label: std::borrow::Cow<'a, str>, piece: Piece| {
        button(text(label.into_owned()).size(13))
            .style(theme::secondary)
            .width(Fill)
            .padding([7, 10])
            .on_press(Message::PieceAdded(piece))
    };

    container(
        column![
            text(t!("template.add_piece"))
                .size(12)
                .font(theme::UI_STRONG),
            piece(t!("template.piece_caption"), Piece::Caption),
            piece(t!("template.piece_holder"), Piece::Holder),
            piece(t!("template.piece_tax_code"), Piece::TaxCode),
            piece(t!("template.piece_date"), Piece::Date),
            piece(t!("template.piece_reason"), Piece::Reason),
            piece(
                t!("template.piece_text"),
                Piece::Text {
                    content: t!("template.piece_text_default").to_string()
                }
            ),
            piece(t!("template.piece_image"), Piece::Image),
            text(t!("template.piece_hint"))
                .size(11)
                .style(iced::widget::text::secondary),
        ]
        .spacing(8),
    )
    .width(Length::Fixed(PALETTE))
    .into()
}

/// The box, with what it will actually say in it.
fn board(state: &State, selected: Option<usize>) -> Element<'_, Message> {
    // The words come from the same resolution the signature uses, so the
    // editor shows the stamp rather than a diagram of it. Nobody is known
    // before the PIN, so the placeholders stand in at a realistic width.
    let facts = Facts::preview(
        state.known_holder().as_ref(),
        (!state.reason.trim().is_empty()).then(|| state.reason.trim()),
    );

    super::board::canvas(
        state.settings.active().resolve(&facts),
        selected,
        state.signature.as_ref().map(|s| super::board::Art {
            handle: s.handle.clone(),
            ratio: s.image.aspect_ratio(),
        }),
        state.box_size_pt().unwrap_or(DEFAULT_BOX_PT),
    )
}

/// What can be changed about the piece under the pointer.
fn properties(state: &State, selected: Option<usize>) -> Element<'_, Message> {
    let Some(placed) = selected.and_then(|i| state.settings.active().pieces.get(i)) else {
        return text(t!("template.nothing_selected"))
            .size(12)
            .style(iced::widget::text::secondary)
            .into();
    };

    // The picture is sized by its rectangle, so type and weight would be
    // controls that do nothing to it.
    let mut controls = row![].spacing(10).align_y(Alignment::Center);

    if placed.piece != Piece::Image {
        controls = controls
            .push(text(t!("template.size")).size(12))
            .push(
                text_input("", &format!("{:.1}", placed.size))
                    .on_input(|raw| {
                        Message::PieceResized(raw.parse().unwrap_or(placed.size))
                    })
                    .style(theme::field)
                    .size(13)
                    .padding(6)
                    .width(Length::Fixed(56.0)),
            )
            .push(
                checkbox(placed.bold)
                    .label(t!("template.bold").into_owned())
                    .on_toggle(Message::PieceBoldToggled)
                    .size(14)
                    .text_size(12),
            )
            .push(
                pick_list(Side::ALL.to_vec(), Some(Side(placed.align)), |s| {
                    Message::PieceAligned(s.0)
                })
                .text_size(12)
                .padding(6),
            );
    }

    let mut body = column![controls].spacing(8);

    if let Piece::Text { content } = &placed.piece {
        body = body.push(
            row![
                text(t!("template.content")).size(12),
                text_input("", content)
                    .on_input(Message::PieceTextChanged)
                    .style(theme::field)
                    .size(13)
                    .padding(6),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }

    container(
        body.push(
            row![
                space::horizontal(),
                small(t!("template.remove_piece"), Some(Message::PieceRemoved)),
            ]
            .align_y(Alignment::Center),
        ),
    )
    .style(theme::card)
    .padding(12)
    .width(Fill)
    .into()
}

/// The picture this template uses, and the way out.
fn footer(state: &State) -> Element<'_, Message> {
    let template = state.settings.active();

    let picture: Element<'_, Message> = match (&template.image, &state.signature) {
        (Some(path), loaded) => row![
            text(t!("template.picture")).size(12),
            text(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            )
            .size(12)
            .style(if loaded.is_some() {
                iced::widget::text::default
            } else {
                iced::widget::text::danger
            }),
            small(t!("document.change"), Some(Message::ChooseSignatureImage)),
            small(
                t!("signature.remove"),
                Some(Message::RemoveSignatureImage)
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        (None, _) => small(t!("signature.choose"), Some(Message::ChooseSignatureImage)),
    };

    row![
        picture,
        space::horizontal(),
        button(text(t!("sign.close")))
            .style(theme::primary)
            .padding([9, 22])
            .on_press(Message::OverlayDismissed),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn small<'a>(
    label: std::borrow::Cow<'a, str>,
    message: Option<Message>,
) -> Element<'a, Message> {
    button(text(label.into_owned()).size(12))
        .style(theme::quiet)
        .on_press_maybe(message)
        .into()
}

/// `pick_list` needs `Display`, and `Align` lives in `cie-firma`, which has no
/// business knowing what language the window speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Side(Align);

impl Side {
    const ALL: [Side; 3] = [Side(Align::Left), Side(Align::Centre), Side(Align::Right)];
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self.0 {
            Align::Left => t!("template.left"),
            Align::Centre => t!("template.centre"),
            Align::Right => t!("template.right"),
        })
    }
}

/// `pick_list` needs `Display`; a template has a name but no opinion on lists.
#[derive(Debug, Clone, PartialEq)]
struct Named {
    index: usize,
    name: String,
}

impl std::fmt::Display for Named {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}
