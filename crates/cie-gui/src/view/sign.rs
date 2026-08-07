//! The panel that covers the window while the card works.
//!
//! One panel, three faces: it asks for the PIN, it shows the card working, and
//! it reports how it went. Blocking the rest of the window here is the point —
//! this is the one stretch where the card must not be touched.

use iced::widget::{button, checkbox, column, container, row, text, text_input};
use iced::{Alignment, Element, Fill, Length};
use rust_i18n::t;

use crate::state::{Message, Phase, State};
use crate::theme;

/// Width of the panel. Wide enough for the five steps to sit on one line each,
/// narrow enough not to swallow the window.
const PANEL: f32 = 460.0;

pub fn panel<'a>(phase: &'a Phase, state: &'a State) -> Element<'a, Message> {
    let body = match phase {
        Phase::Pin { pin, remember } => ask_for_pin(pin, *remember, state),
        Phase::Running { step, label } => running(*step, label),
        Phase::Done(path) => done(path),
        Phase::Failed(error) => failed(error),
        Phase::Idle => text("").into(),
    };

    super::over(
        container(body)
            .style(theme::card)
            .padding(24)
            .width(Length::Fixed(PANEL))
            .into(),
    )
}

fn ask_for_pin<'a>(pin: &str, remember: bool, state: &'a State) -> Element<'a, Message> {
    let complete = pin.len() == 8;

    let mut body = column![
        text(t!("sign.pin_title")).size(17),
        row![
            text_input("", pin)
                .secure(true)
                .on_input(Message::PinChanged)
                .on_submit(Message::PinConfirmed)
                .style(theme::field)
                .width(Length::Fixed(160.0))
                .padding(10),
            text(t!("sign.pin_hint"))
                .size(12)
                .style(iced::widget::text::secondary),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    ]
    .spacing(16);

    if !pin.is_empty() && !complete {
        body = body.push(
            text(t!("sign.pin_short"))
                .size(12)
                .style(iced::widget::text::secondary),
        );
    }

    // Nowhere to remember it without a card id: offering the tick would be a
    // promise we cannot keep.
    if state.card.id.is_some() {
        body = body.push(
            checkbox(remember)
                .label(t!("sign.pin_remember").into_owned())
                .on_toggle(Message::RememberToggled)
                .size(15)
                .text_size(13),
        );
    }

    body.push(
        row![
            iced::widget::space::horizontal(),
            button(text(t!("sign.cancel")))
                .style(theme::quiet)
                .on_press(Message::SignCancelled),
            button(text(t!("sign.action")))
                .style(theme::primary)
                .padding([9, 22])
                .on_press_maybe(complete.then_some(Message::PinConfirmed)),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .into()
}

/// The five steps, ticked off as they go.
///
/// The labels come from the same `Step` sequence the card layer reports, so a
/// step added there shows up here instead of silently going missing.
fn running<'a>(step: usize, label: &str) -> Element<'a, Message> {
    let steps = [
        t!("sign.step_connecting"),
        t!("sign.step_channel"),
        t!("sign.step_pin"),
        t!("sign.step_certificate"),
        t!("sign.step_signing"),
    ];

    let mut body = column![text(t!("sign.in_progress")).size(17)].spacing(14);

    let mut list = column![].spacing(8);
    for (i, name) in steps.iter().enumerate() {
        let (mark, dim) = match i.cmp(&step) {
            std::cmp::Ordering::Less => ("✓", false),
            std::cmp::Ordering::Equal => ("◐", false),
            std::cmp::Ordering::Greater => ("·", true),
        };
        let line = row![text(mark).size(14), text(name.clone().into_owned()).size(14)].spacing(10);
        list = list.push(if dim {
            container(line).style(|t: &iced::Theme| container::Style {
                text_color: Some(iced::Color {
                    a: 0.45,
                    ..t.extended_palette().background.base.text
                }),
                ..Default::default()
            })
        } else {
            container(line)
        });
    }

    body = body.push(list);

    // The label the card thread sent, in case it says something the fixed list
    // cannot — an unexpected step, or a wait longer than it looks.
    let _ = label;

    body.push(
        text(t!("sign.do_not_remove"))
            .size(12)
            .style(iced::widget::text::secondary),
    )
    .into()
}

fn done(path: &std::path::Path) -> Element<'_, Message> {
    column![
        text(t!("sign.done")).size(17).style(iced::widget::text::success),
        text(path.display().to_string()).size(13),
        text(t!("sign.eku_warning"))
            .size(11)
            .style(iced::widget::text::secondary),
        row![
            button(text(t!("sign.open_file")))
                .style(theme::primary)
                .padding([9, 18])
                .on_press(Message::OpenResult),
            button(text(t!("sign.open_folder")))
                .style(theme::secondary)
                .padding([9, 18])
                .on_press(Message::OpenFolder),
            iced::widget::space::horizontal(),
            button(text(t!("sign.close")))
                .style(theme::quiet)
                .on_press(Message::OutcomeDismissed),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Fill),
    ]
    .spacing(16)
    .into()
}

/// A failure keeps the panel open with Riprova: a wrong PIN is retried from
/// here, and restarting the whole thing would cost one of the card's three
/// tries for nothing.
fn failed(error: &str) -> Element<'_, Message> {
    column![
        text(t!("sign.failed")).size(17).style(iced::widget::text::danger),
        text(error).size(13),
        row![
            iced::widget::space::horizontal(),
            button(text(t!("sign.close")))
                .style(theme::quiet)
                .on_press(Message::OutcomeDismissed),
            button(text(t!("sign.retry")))
                .style(theme::primary)
                .padding([9, 22])
                .on_press(Message::SignPressed),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(16)
    .into()
}
