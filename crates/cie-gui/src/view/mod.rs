//! The window: application bar, desk, sidebar, action bar — and the signing
//! panel laid over the lot.

mod about;
pub mod board;
mod editor;
mod menu;
mod sidebar;
mod sign;
mod stamp;

use iced::widget::{
    button, column, container, image, opaque, responsive, row, scrollable, space, stack, text,
    tooltip,
};
use iced::{Alignment, ContentFit, Element, Fill, Length};
use rust_i18n::t;

use cie_lib::Presence;

use crate::icon;
use crate::state::{Message, Overlay, Phase, State};
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

    // The signing panel goes on last: while the card is working it has to be
    // the thing in front, whatever else was open when Firma was pressed.
    let mut layers = stack![window];
    if let Some(open) = state.menu {
        layers = layers.push(menu::dropdown(open, state));
    }
    match state.overlay {
        Overlay::None => {}
        Overlay::About => layers = layers.push(about::panel()),
        Overlay::Templates { selected } => {
            layers = layers.push(editor::panel(state, selected))
        }
    }
    if let phase @ (Phase::Pin { .. } | Phase::Running { .. } | Phase::Done(_) | Phase::Failed(_)) =
        &state.phase
    {
        layers = layers.push(sign::panel(phase, state));
    }
    layers.into()
}

/// The menus on the left, and on the right who the card says you are.
///
/// The program's name used to sit here. It has moved into About: the bar is
/// now the place you act from, and a title that only repeats what the window
/// manager already shows was the least useful thing on it.
fn app_bar(state: &State) -> Element<'_, Message> {
    container(
        row![
            menu::titles(state),
            // About is a button, not a menu: a list that drops open to offer
            // one thing is a click spent on nothing.
            button(text(t!("menu.about")).size(14))
                .style(theme::menu_title)
                .padding([7, 10])
                .on_press(Message::ShowAbout),
            space::horizontal(),
            card_badge(state),
            button(text(state.mode.label().into_owned()).size(12))
                .style(theme::quiet)
                .on_press(Message::ThemeChanged(state.mode.next())),
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .padding([0, 12]),
    )
    .style(theme::bar)
    .width(Fill)
    .height(Length::Fixed(menu::BAR))
    .into()
}

/// Who the card says you are, and the way to ask the reader again.
///
/// The whole badge is a button, because the reader is looked at when the
/// window opens and never again on its own: a reader plugged in afterwards, or
/// a card put down after the window was already up, is the ordinary way round
/// and not the odd one. Whatever the badge says has to be one click from being
/// said afresh.
fn card_badge(state: &State) -> Element<'_, Message> {
    // Presence first, because it is the one that is never out of date. The id
    // underneath it is whatever the last look found, and while a look is out
    // the badge says so rather than showing an answer being replaced as it is
    // read.
    let (label, known) = match (state.card.presence, &state.card.id) {
        (_, Some(id)) => {
            let who = state.card.stored.as_ref().and_then(|s| s.holder());
            let label = match who {
                Some(who) => format!("{who} · {id}"),
                None => id.clone(),
            };
            (label.into(), true)
        }
        _ if state.looking() => (t!("card.looking"), false),
        // Something is on the reader and the look came back empty-handed: it
        // is a card, but not one this program can sign with.
        (Presence::Card, None) => (t!("card.foreign"), false),
        (Presence::Empty, _) => (t!("card.no_card"), false),
        (Presence::NoReader, _) => (t!("card.no_reader"), false),
        (Presence::Unknown, _) => (t!("card.unknown"), false),
    };

    // Lit only with a card actually on the reader: this is the ● the bar used
    // to draw, and it has to keep meaning exactly that.
    let dim = |t: &iced::Theme| iced::widget::text::Style {
        color: Some(t.extended_palette().background.base.text.scale_alpha(0.55)),
    };
    let glyph = text(if state.looking() { icon::REREAD } else { icon::CARD }.to_string())
        .font(icon::FONT)
        .size(14);

    let face = button(
        row![
            if known { glyph } else { glyph.style(dim) },
            text(label.into_owned()).size(13),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .style(theme::quiet)
    .padding([5, 9])
    .on_press_maybe(state.can_look().then_some(Message::LookForCard));

    tip(face, t!("card.reread"), tooltip::Position::Bottom)
}

/// A control and the words it does not have room for, a moment behind.
fn tip<'a>(
    control: impl Into<Element<'a, Message>>,
    label: std::borrow::Cow<'a, str>,
    at: tooltip::Position,
) -> Element<'a, Message> {
    tooltip(
        control,
        container(text(label.into_owned()).size(12))
            .style(theme::floating)
            .padding([4, 8]),
        at,
    )
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
    let handle = state.page_image.clone();
    let (ratio, zoom) = (doc.aspect_ratio(state.page), state.zoom);
    let (page, stamp) = (state.page, state.stamp);

    // Resolved here, where the card and the settings are both in reach. The
    // canvas gets words, not the means of working them out.
    let facts = cie_firma::Facts::preview(
        state.known_holder().as_ref(),
        (!state.reason.trim().is_empty()).then(|| state.reason.trim()),
    );
    let pieces = state.settings.active().resolve(&facts);
    let art = state.signature.as_ref().map(|s| board::Art {
        handle: s.handle.clone(),
        ratio: s.image.aspect_ratio(),
    });
    let paper = board::Paper {
        background: state.settings.active().background,
        border: state.settings.active().border,
    };

    // `responsive` reports the room actually left, which is what "fits the
    // window" has to mean. It sits outside the scrollable on purpose: inside,
    // the available height is whatever the content asked for.
    let sheet = responsive(move |room| {
        let (mut w, mut h) = (room.width, room.width / ratio);
        if h > room.height {
            h = room.height;
            w = h * ratio;
        }
        let (w, h) = ((w * zoom).max(1.0), (h * zoom).max(1.0));

        // Both layers get exactly this box and fill it, so the draggable
        // rectangle lands on the page it claims to point at.
        let mut layers = stack![];
        if let Some(bitmap) = handle.clone() {
            layers = layers.push(
                image(bitmap)
                    .width(w)
                    .height(h)
                    .content_fit(ContentFit::Fill),
            );
        }
        if let Some(d) = state.document.as_ref() {
            layers = layers.push(stamp::canvas(
                d,
                page,
                stamp,
                pieces.clone(),
                art.clone(),
                paper,
                state.settings.snapping,
            ));
        }

        scrollable(container(layers.width(w).height(h)).center_x(Fill))
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::default(),
                horizontal: scrollable::Scrollbar::default(),
            })
            .width(Fill)
            .height(Fill)
            .into()
    });

    column![
        container(sheet).height(Fill).padding(20),
        container(pager(state)).center_x(Fill),
    ]
    .spacing(8)
    .padding([16, 0])
    .into()
}

/// Pages on the left, zoom on the right: the two things you do to a document
/// you are not editing.
fn pager(state: &State) -> Element<'_, Message> {
    let step = |glyph: &'static str, message: Option<Message>| {
        button(text(glyph)).style(theme::quiet).on_press_maybe(message)
    };

    row![
        step("◀", (state.page > 0).then_some(Message::PageBack)),
        text(t!(
            "stamp.page",
            current = state.page + 1,
            total = state.page_count()
        ))
        .size(13),
        step(
            "▶",
            (state.page + 1 < state.page_count()).then_some(Message::PageForward)
        ),
        space::horizontal().width(24),
        step("−", (state.zoom > 1.0).then_some(Message::ZoomOut)),
        button(text(format!("{:.0}%", state.zoom * 100.0)).size(12))
            .style(theme::quiet)
            .on_press(Message::ZoomReset),
        step("+", (state.zoom < 8.0).then_some(Message::ZoomIn)),
        space::horizontal().width(24),
        // The same setting the editor's toolbar carries: the box latches on to
        // the page here, and being sent into a modal to say otherwise would be
        // a setting hidden from the only place it applies.
        button(text(crate::icon::SNAP.to_string()).font(crate::icon::FONT).size(14))
            .style(if state.settings.snapping {
                theme::tool_chosen
            } else {
                theme::tool
            })
            .padding([5, 8])
            .on_press(Message::SnappingToggled(!state.settings.snapping)),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
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
            sign_button(state),
        ]
        .align_y(Alignment::Center)
        .padding([12, 16]),
    )
    .style(theme::bar)
    .width(Fill)
    .height(Length::Fixed(64.0))
    .into()
}

/// Firma, and — when it is dead — what it is waiting for.
///
/// A grey button with nothing to say is a puzzle, and now that a document on
/// its own is not enough to press it, the commonest reason it is grey is a
/// reader with nothing on it. That belongs under the cursor that has just
/// found the button dead, not in a corner of the window.
fn sign_button(state: &State) -> Element<'_, Message> {
    let face = button(text(t!("sign.action")).size(15).font(theme::UI_STRONG))
        .style(theme::primary)
        .padding([10, 28])
        .on_press_maybe(state.can_sign().then_some(Message::SignPressed));

    // Nothing open is its own explanation — the desk says so in the middle of
    // the window — and a signature already running has a panel over the lot.
    if state.can_sign() || state.bytes.is_none() || state.card.here() {
        return face.into();
    }

    let why = match state.card.presence {
        Presence::NoReader => t!("sign.needs_reader"),
        _ => t!("sign.needs_card"),
    };
    tip(face, why, tooltip::Position::Top)
}

/// Wraps a panel so the layers underneath stop receiving the mouse.
pub fn over<'a>(panel: Element<'a, Message>) -> Element<'a, Message> {
    opaque(container(panel).center(Fill).style(theme::veil))
}
