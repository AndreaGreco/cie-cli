//! Signing with the CIE, with the document in front of you.
//!
//! A separate binary from `cie`: whoever uses the command line should not have
//! to carry a GUI toolkit around.

mod job;
mod preview;
mod state;
mod theme;
mod view;

use iced::{Size, Subscription, Task, event, time, window};
use std::time::Duration;

use state::{Message, State};

rust_i18n::i18n!("../../locales", fallback = "it");

/// How often the card thread's channel gets drained.
///
/// ponytail: a timer instead of adapting the `mpsc` to an async stream. The
/// handshake takes seconds, so 100 ms is invisible; `iced::stream::channel`
/// goes here the day a smooth progress bar matters.
const TICK: Duration = Duration::from_millis(100);

fn main() -> iced::Result {
    // The desktop's language, falling back to Italian: this is a tool for an
    // Italian identity document, so that is the sensible default.
    if let Some(lang) = std::env::var("LANG").ok().and_then(|l| {
        let code = l.split(['_', '.']).next()?.to_string();
        (!code.is_empty() && code != "C").then_some(code)
    }) {
        rust_i18n::set_locale(&lang);
    }

    iced::application(State::new, update, view::view)
        .title(|_: &State| rust_i18n::t!("app.title").into_owned())
        .theme(|s: &State| s.mode.theme())
        .subscription(subscription)
        .font(include_bytes!("../fonts/TitilliumWeb-Regular.ttf").as_slice())
        .font(include_bytes!("../fonts/TitilliumWeb-SemiBold.ttf").as_slice())
        .default_font(theme::UI)
        .window(window::Settings {
            size: Size::new(1100.0, 760.0),
            min_size: Some(Size::new(900.0, 600.0)),
            // The compositor places the window; all we can do is say who we are.
            platform_specific: window::settings::PlatformSpecific {
                application_id: "cie-gui".to_string(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run()
}

/// Everything that touches the world lives here; `State::update` stays pure.
fn update(state: &mut State, message: Message) -> Task<Message> {
    match &message {
        Message::ChooseDocument => {
            return Task::perform(
                async { rfd::AsyncFileDialog::new().pick_file().await },
                |f| Message::DocumentChosen(f.map(|f| f.path().to_path_buf())),
            );
        }
        Message::ChooseOutput => {
            let mut dialog = rfd::AsyncFileDialog::new();
            if let Some(dir) = state.output.parent() {
                dialog = dialog.set_directory(dir);
            }
            if let Some(name) = state.output.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
            return Task::perform(async move { dialog.save_file().await }, |f| {
                Message::OutputChosen(f.map(|f| f.path().to_path_buf()))
            });
        }
        // A failure to open is not worth a dialog of its own: the file is
        // written either way, and the desktop already complains on its own.
        Message::OpenResult => {
            if let state::Phase::Done(path) = &state.phase {
                let _ = opener::open(path);
            }
        }
        Message::OpenFolder => {
            if let state::Phase::Done(path) = &state.phase
                && let Some(dir) = path.parent()
            {
                let _ = opener::open(dir);
            }
        }
        _ => {}
    }

    state.update(message);
    Task::none()
}

fn subscription(state: &State) -> Subscription<Message> {
    let dropped = event::listen_with(|event, _status, _window| match event {
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
        _ => None,
    });

    // The timer only exists while a thread has something to say.
    if state.busy() {
        Subscription::batch([dropped, time::every(TICK).map(|_| Message::Tick)])
    } else {
        dropped
    }
}
