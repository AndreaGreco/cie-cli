//! Who wrote this, which version it is, and what it is allowed to promise.
//!
//! The last part is the one that matters. A CIE carries an *authentication*
//! certificate, not a qualified signing one, and a signature made with it is
//! not a firma qualificata however much the PDF looks like one. That sentence
//! belongs somewhere permanent, not only in the panel that flashes up after a
//! signature.

use iced::widget::{button, column, container, row, space, text};
use iced::{Alignment, Element, Length};
use rust_i18n::t;

use crate::state::Message;
use crate::theme;

const PANEL: f32 = 480.0;

/// What Cargo already knows about us. Kept out of the strings file: a version
/// that has to be updated by hand is a version that goes stale.
const VERSION: &str = env!("CARGO_PKG_VERSION");
const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

pub fn panel<'a>() -> Element<'a, Message> {
    let line = |label: std::borrow::Cow<'a, str>, value: String| {
        row![
            text(label.into_owned())
                .size(12)
                .style(iced::widget::text::secondary)
                .width(Length::Fixed(110.0)),
            text(value).size(13),
        ]
        .spacing(8)
    };

    let body = column![
        text(t!("app.title")).size(19).font(theme::UI_STRONG),
        text(t!("about.tagline"))
            .size(13)
            .style(iced::widget::text::secondary),
        column![
            line(t!("about.version"), VERSION.to_string()),
            line(t!("about.author"), AUTHORS.to_string()),
            line(t!("about.licence"), "MIT".to_string()),
            line(t!("about.source"), REPOSITORY.to_string()),
        ]
        .spacing(4),
        text(t!("sign.eku_warning"))
            .size(12)
            .style(iced::widget::text::danger),
        // Three crates do the work this program takes credit for.
        text(t!("about.credits"))
            .size(11)
            .style(iced::widget::text::secondary),
        row![
            space::horizontal(),
            button(text(t!("sign.close")))
                .style(theme::primary)
                .padding([9, 22])
                .on_press(Message::OverlayDismissed),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(14);

    super::over(
        container(body)
            .style(theme::card)
            .padding(24)
            .width(Length::Fixed(PANEL))
            .into(),
    )
}
