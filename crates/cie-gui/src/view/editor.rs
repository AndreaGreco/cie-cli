//! The template editor: the palette, the box, and the piece under the pointer.
//!
//! There is no OK and no Annulla. What is dragged is saved as it is dragged,
//! the way the rest of the settings in this program behave, and the way an
//! editor with a live preview has to behave to be worth having one.

use iced::widget::{
    button, checkbox, column, container, pick_list, row, space, text, text_input, tooltip,
};
use iced::{Alignment, Element, Fill, Length};
use rust_i18n::t;

use cie_firma::{Align, Facts, Piece, Vertical};

use crate::icon;
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
                toolbar(state, selected),
                container(board(state, selected))
                    .style(theme::desk)
                    .padding(16)
                    .width(Fill)
                    .height(Fill),
            ]
            .spacing(6)
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
            chooser(icon::ADD, t!("template.add"), false, Some(Message::TemplateAdded)),
            chooser(
                icon::DUPLICATE,
                t!("template.duplicate"),
                false,
                Some(Message::TemplateDuplicated)
            ),
            chooser(
                icon::REMOVE,
                t!("template.delete"),
                false,
                removable.then_some(Message::TemplateDeleted)
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
        // What goes behind the stamp is the template's, not the piece's, so it
        // belongs up here with the name rather than in the panel that changes
        // whatever happens to be selected.
        row![
            checkbox(state.settings.active().background)
                .label(t!("template.background").into_owned())
                .on_toggle(Message::TemplateBackgroundToggled)
                .size(14)
                .text_size(12),
            checkbox(state.settings.active().border)
                .label(t!("template.border").into_owned())
                .on_toggle(Message::TemplateBorderToggled)
                .size(14)
                .text_size(12),
            text(t!("template.paper_hint"))
                .size(11)
                .style(iced::widget::text::secondary),
        ]
        .spacing(16)
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
        super::board::Paper {
            background: state.settings.active().background,
            border: state.settings.active().border,
        },
    )
}

/// What can be changed about the piece under the pointer.
///
/// A fixed row above the box rather than a panel below it. Below, it appeared
/// and disappeared with the selection and the box resized under the hand that
/// was dragging something in it; here the row is always the same height and
/// the controls that do not apply are simply dead.
fn toolbar(state: &State, selected: Option<usize>) -> Element<'_, Message> {
    let placed = selected.and_then(|i| state.settings.active().pieces.get(i));
    let picture = placed.is_some_and(|p| p.piece == Piece::Image);

    // Across the rectangle, which every piece has an opinion about.
    let across = |glyph, align: Align, label| {
        chooser(
            glyph,
            label,
            placed.is_some_and(|p| p.align == align),
            placed.map(|_| Message::PieceAligned(align)),
        )
    };

    // Up and down it, which only the picture does. See `cie_firma::Vertical`.
    //
    // Lit only when it also applies: a text piece carries a value on this axis
    // and nothing reads it, so showing it lit would say a setting is in force
    // when it does nothing at all.
    let up = |glyph, vertical: Vertical, label| {
        chooser(
            glyph,
            label,
            picture && placed.is_some_and(|p| p.vertical == vertical),
            picture.then_some(Message::PieceRaised(vertical)),
        )
    };

    let mut bar = row![
        across(icon::ALIGN_LEFT, Align::Left, t!("template.left")),
        across(icon::ALIGN_CENTRE, Align::Centre, t!("template.centre")),
        across(icon::ALIGN_RIGHT, Align::Right, t!("template.right")),
        separator(),
        up(icon::ALIGN_TOP, Vertical::Top, t!("template.top")),
        up(icon::ALIGN_MIDDLE, Vertical::Middle, t!("template.middle")),
        up(icon::ALIGN_BOTTOM, Vertical::Bottom, t!("template.bottom")),
        separator(),
        chooser(
            icon::BOLD,
            t!("template.bold"),
            !picture && placed.is_some_and(|p| p.bold),
            placed
                .filter(|p| p.piece != Piece::Image)
                .map(|p| Message::PieceBoldToggled(!p.bold)),
        ),
    ]
    .spacing(2)
    .align_y(Alignment::Center);

    // The type size is a number, and a number wants a field. The picture is
    // sized by its rectangle, so it gets neither this nor the weight above.
    let size = placed.filter(|p| p.piece != Piece::Image);
    bar = bar
        .push(
            tip(
                text(icon::SIZE.to_string())
                    .font(icon::FONT)
                    .size(15)
                    .style(if size.is_some() {
                        iced::widget::text::default
                    } else {
                        iced::widget::text::secondary
                    }),
                t!("template.size"),
            )
        )
        .push(
            text_input("", &size.map(|p| format!("{:.1}", p.size)).unwrap_or_default())
                .on_input_maybe(size.map(|p| {
                    let current = p.size;
                    move |raw: String| Message::PieceResized(raw.parse().unwrap_or(current))
                }))
                .style(theme::field)
                .size(13)
                .padding(6)
                .width(Length::Fixed(56.0)),
        );

    // Only free text has words of its own; everything else says what the card
    // says. The field is there either way so the row keeps its shape.
    let free = placed.and_then(|p| match &p.piece {
        Piece::Text { content } => Some(content.clone()),
        _ => None,
    });
    bar = bar
        .push(separator())
        .push(
            text_input(&t!("template.content"), free.as_deref().unwrap_or(""))
                .on_input_maybe(free.as_ref().map(|_| Message::PieceTextChanged))
                .style(theme::field)
                .size(13)
                .padding(6),
        )
        .push(chooser(
            icon::REMOVE,
            t!("template.remove_piece"),
            false,
            placed.map(|_| Message::PieceRemoved),
        ));

    container(bar).padding([6, 8]).width(Fill).into()
}

/// An icon button that shows whether what it stands for is in force.
///
/// `None` for the message greys it out, which is how the row says a control
/// does not apply to what is selected — or that nothing is.
fn chooser<'a>(
    glyph: char,
    label: std::borrow::Cow<'a, str>,
    chosen: bool,
    message: Option<Message>,
) -> Element<'a, Message> {
    let face = button(
        text(glyph.to_string())
            .font(icon::FONT)
            .size(15)
            .align_x(Alignment::Center),
    )
    .style(if chosen { theme::tool_chosen } else { theme::tool })
    .padding([6, 8])
    .width(Length::Fixed(32.0))
    .on_press_maybe(message);

    tip(face, label)
}

/// An icon says nothing to somebody who has not met it. Every one of these
/// carries the words it stands for, a moment behind.
fn tip<'a>(
    control: impl Into<Element<'a, Message>>,
    label: std::borrow::Cow<'a, str>,
) -> Element<'a, Message> {
    tooltip(
        control,
        container(text(label.into_owned()).size(12))
            .style(theme::floating)
            .padding([4, 8]),
        tooltip::Position::Bottom,
    )
    .into()
}

/// A hairline between two groups of controls.
///
/// Two containers, not one: a background paints a container's padding too, so
/// the line and the room around it have to be separate boxes or the rule comes
/// out a grey block as wide as its own margins.
fn separator<'a>() -> Element<'a, Message> {
    container(
        container(space::vertical())
            .width(Length::Fixed(1.0))
            .height(Length::Fixed(18.0))
            .style(|t: &iced::Theme| container::Style {
                background: Some(iced::Background::Color(iced::Color {
                    a: 0.25,
                    ..t.extended_palette().background.base.text
                })),
                ..Default::default()
            }),
    )
    .padding([0, 6])
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
