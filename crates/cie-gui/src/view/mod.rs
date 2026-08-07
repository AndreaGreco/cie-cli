//! The window: application bar, desk, sidebar, action bar — and the signing
//! panel laid over the lot.

mod sidebar;
mod sign;

use iced::widget::{
    button, column, container, image, opaque, row, space, stack, text,
};
use iced::{Alignment, Element, Fill, Length};
use rust_i18n::t;

use crate::state::{Message, Phase, State};
use crate::theme;

/// Width of the sidebar. Fixed on purpose: a resizable split is one more thing
/// to get wrong, and the page has all the elasticity that matters.
const SIDEBAR: f32 = 340.0;

pub fn view(state: &State) -> Element<'_, Message> {
    let window = column![
        app_bar(state),
        row![desk(state), sidebar::view(state)].height(Fill),
        action_bar(state),
    ];

    match &state.phase {
        Phase::Idle => window.into(),
        phase => stack![window, sign::panel(phase, state)].into(),
    }
}

/// The bar carries the name and who the card says you are. It is not a client
/// side decoration: on a tiling compositor the frame is not ours to draw.
fn app_bar(state: &State) -> Element<'_, Message> {
    let card = match (&state.card.id, &state.card.stored) {
        (Some(id), Some(stored)) => match stored.holder() {
            Some(who) => text(format!("● {who} · {id}")),
            None => text(format!("● {id}")),
        },
        (Some(id), None) => text(format!("● {id}")),
        (None, _) => text("○").style(iced::widget::text::secondary),
    };

    container(
        row![
            text(t!("app.title")).size(17).font(theme::UI_STRONG),
            space::horizontal(),
            card.size(13),
            button(text(state.mode.label().into_owned()).size(12))
                .style(theme::quiet)
                .on_press(Message::ThemeChanged(state.mode.next())),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .padding([10, 16]),
    )
    .style(theme::bar)
    .width(Fill)
    .into()
}

/// The desk: the page, or the reason there is not one.
fn desk(state: &State) -> Element<'_, Message> {
    let content: Element<'_, Message> = match (&state.bytes, state.shows_preview()) {
        (None, _) => empty_state(),
        (Some(_), true) => page(state),
        (Some(_), false) => container(
            text(if state.document.is_none() && state.open_error.is_some() {
                t!("document.unreadable")
            } else {
                t!("document.not_a_pdf")
            })
            .style(iced::widget::text::secondary),
        )
        .center(Fill)
        .into(),
    };

    container(content)
        .style(theme::desk)
        .width(Fill)
        .height(Fill)
        .into()
}

fn empty_state<'a>() -> Element<'a, Message> {
    container(
        column![
            text(t!("document.drop_here")).size(19).font(theme::UI_STRONG),
            button(text(t!("document.or_choose")))
                .style(theme::secondary)
                .on_press(Message::ChooseDocument),
        ]
        .spacing(16)
        .align_x(Alignment::Center),
    )
    .center(Fill)
    .into()
}

/// The page, with the pager under it.
///
/// The image is left to scale itself inside whatever room is left: iced keeps
/// the aspect ratio, so nothing here has to do the arithmetic egui needed.
fn page(state: &State) -> Element<'_, Message> {
    let doc = state.document.as_ref().expect("checked by the caller");
    let handle = doc.render(state.page, 1400.0);

    let pager = row![
        button(text("◀"))
            .style(theme::quiet)
            .on_press_maybe((state.page > 0).then_some(Message::PageBack)),
        text(t!(
            "stamp.page",
            current = state.page + 1,
            total = state.page_count()
        ))
        .size(13),
        button(text("▶"))
            .style(theme::quiet)
            .on_press_maybe(
                (state.page + 1 < state.page_count()).then_some(Message::PageForward)
            ),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    column![
        container(image(handle).width(Fill))
            .center_x(Fill)
            .height(Fill)
            .padding(20),
        container(pager).center_x(Fill).padding([0, 0]),
    ]
    .spacing(8)
    .padding([16, 0])
    .into()
}

/// Where the file will land, and the one button that matters.
fn action_bar(state: &State) -> Element<'_, Message> {
    let destination: Element<'_, Message> = if state.path.is_some() {
        row![
            text("→").size(14),
            text(state.output.display().to_string()).size(13),
            button(text(t!("document.change")))
                .style(theme::quiet)
                .on_press(Message::ChooseOutput),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    } else {
        space::horizontal().into()
    };

    container(
        row![
            destination,
            space::horizontal(),
            button(text(t!("sign.action")).size(15).font(theme::UI_STRONG))
                .style(theme::primary)
                .padding([10, 28])
                .on_press_maybe(state.can_sign().then_some(Message::SignPressed)),
        ]
        .align_y(Alignment::Center)
        .padding([12, 16]),
    )
    .style(theme::bar)
    .width(Fill)
    .height(Length::Fixed(64.0))
    .into()
}

/// Wraps a panel so the layers underneath stop receiving the mouse.
pub fn over<'a>(panel: Element<'a, Message>) -> Element<'a, Message> {
    opaque(container(panel).center(Fill).style(theme::veil))
}
