//! Signing with the CIE, with the document in front of you.
//!
//! A separate binary from `cie`: whoever uses the command line should not have
//! to carry a GUI toolkit around.

// On Windows a plain binary opens a console behind the window. In debug that
// console is where the logs go, so only the shipped build hides it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod icon;
mod job;
mod preview;
mod settings;
mod snap;
mod state;
mod theme;
mod view;

use iced::{Size, Subscription, Task, event, keyboard, mouse, time, window};
use std::time::Duration;

use state::{Message, State};

rust_i18n::i18n!("../../locales", fallback = "it");

/// How often the card thread's channel gets drained.
///
/// ponytail: a timer instead of adapting the `mpsc` to an async stream. The
/// handshake takes seconds, so 100 ms is invisible; `iced::stream::channel`
/// goes here the day a smooth progress bar matters.
const TICK: Duration = Duration::from_millis(100);

/// And how often the watch on the readers is drained, with nothing else going
/// on.
///
/// The wait itself is PC/SC's, not ours: the thread is asleep in the driver
/// until a card lands, and this is only how long the news then sits in the
/// channel. A third of a second is quicker than a hand can put a card down and
/// look up, at a third of the wake-ups of the tick above.
const WATCH: Duration = Duration::from_millis(300);

fn main() -> iced::Result {
    // The desktop's language, falling back to Italian: this is a tool for an
    // Italian identity document, so that is the sensible default.
    if let Some(lang) = std::env::var("LANG").ok().and_then(|l| {
        let code = l.split(['_', '.']).next()?.to_string();
        (!code.is_empty() && code != "C").then_some(code)
    }) {
        rust_i18n::set_locale(&lang);
    }

    #[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
    let mut window = window::Settings {
        size: Size::new(1280.0, 880.0),
        min_size: Some(Size::new(1000.0, 680.0)),
        ..Default::default()
    };
    // The compositor places the window; all we can do is say who we are, and
    // only X11/Wayland have somewhere to say it.
    #[cfg(target_os = "linux")]
    {
        window.platform_specific.application_id = "cie-gui".to_string();
    }

    iced::application(State::new, update, view::view)
        .title(|_: &State| rust_i18n::t!("app.title").into_owned())
        .theme(|s: &State| s.mode.theme())
        .subscription(subscription)
        .font(include_bytes!("../fonts/TitilliumWeb-Regular.ttf").as_slice())
        .font(include_bytes!("../fonts/TitilliumWeb-SemiBold.ttf").as_slice())
        .font(include_bytes!("../fonts/lucide.ttf").as_slice())
        .default_font(theme::UI)
        .window(window)
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
        // Only PNG and JPEG: they are what a PDF can carry without us
        // re-encoding anything, and the dialog is the friendlier place to say
        // so than an error after the file was chosen.
        Message::ChooseSignatureImage => {
            return Task::perform(
                async {
                    rfd::AsyncFileDialog::new()
                        .add_filter("PNG, JPEG", &["png", "jpg", "jpeg"])
                        .pick_file()
                        .await
                },
                |f| Message::SignatureImageChosen(f.map(|f| f.path().to_path_buf())),
            );
        }
        // Ctrl+S can arrive with nothing open; the menu hides the entry in
        // that case, and the shortcut has to agree with it.
        Message::ChooseOutput if state.path.is_some() => {
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
        Message::Quit => return iced::exit(),
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

/// Turns raw input into intent. Deliberately dumb: whether an intent is
/// allowed right now is `State::update`'s call, where it can be tested.
fn input(event: iced::Event, status: event::Status, _window: window::Id) -> Option<Message> {
    use keyboard::key::Named;

    match event {
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),

        // `Captured` means a widget already dealt with it — a scrollbar, most
        // often. Only untouched scrolls are ours to interpret.
        iced::Event::Mouse(mouse::Event::WheelScrolled { delta })
            if status == event::Status::Ignored =>
        {
            let y = match delta {
                mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y,
            };
            (y != 0.0).then_some(Message::Wheel { up: y > 0.0 })
        }

        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => match key {
            keyboard::Key::Named(Named::ArrowLeft | Named::PageUp) => Some(Message::PageBack),
            keyboard::Key::Named(Named::ArrowRight | Named::PageDown) => Some(Message::PageForward),
            // The shortcuts the menus advertise, plus the zoom ones that were
            // here before there were any menus to advertise them.
            keyboard::Key::Character(ref c) if modifiers.command() => match c.as_str() {
                "+" | "=" => Some(Message::ZoomIn),
                "-" => Some(Message::ZoomOut),
                "0" => Some(Message::ZoomReset),
                "o" => Some(Message::ChooseDocument),
                "s" => Some(Message::ChooseOutput),
                "q" => Some(Message::Quit),
                "t" => Some(Message::ShowTemplates),
                _ => None,
            },
            // Whatever is laid over the window goes away on Escape. The card
            // is not interrupted this way: the signing panel does not listen.
            keyboard::Key::Named(Named::Escape) => Some(Message::OverlayDismissed),
            _ => None,
        },
        _ => None,
    }
}

fn subscription(state: &State) -> Subscription<Message> {
    let input = event::listen_with(input);

    // The watch on the readers runs for as long as the window does, so there
    // is always a channel to drain; how often depends on whether anything is
    // waiting on the other end of it.
    let every = if state.busy() { TICK } else { WATCH };
    Subscription::batch([input, time::every(every).map(|_| Message::Tick)])
}
