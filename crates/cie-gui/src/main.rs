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

use iced::futures::StreamExt;
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
        // The bundle gives the icon to the launcher, not to the window: without
        // this the taskbar shows the generic one.
        icon: window::icon::from_file_data(include_bytes!("../icons/256x256.png"), None).ok(),
        ..Default::default()
    };
    // The compositor places the window; all we can do is say who we are, and
    // only X11/Wayland have somewhere to say it.
    #[cfg(target_os = "linux")]
    {
        window.platform_specific.application_id = "cie-gui".to_string();
    }

    iced::application(State::new, update, view::view)
        // The close button asks rather than closes: see `Message::Quit`.
        .exit_on_close_request(false)
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
        // The way out is a message like any other, so the change in hand is
        // written down before the window goes: `update` settles the run it is
        // in the middle of.
        Message::Quit => {
            state.update(Message::Quit);
            return iced::exit();
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

/// Turns raw input into intent. Deliberately dumb: whether an intent is
/// allowed right now is `State::update`'s call, where it can be tested.
fn input(event: iced::Event, status: event::Status, _window: window::Id) -> Option<Message> {
    use keyboard::key::Named;

    match event {
        iced::Event::Window(window::Event::FileDropped(path)) => Some(Message::FileDropped(path)),
        // The window's own close button leaves the same way Ctrl+Q does. It
        // has to: iced would otherwise take the window down without the
        // settings being written, and half a typed name would go with it.
        iced::Event::Window(window::Event::CloseRequested) => Some(Message::Quit),

        // `Captured` means a widget already dealt with it - a scrollbar, most
        // often. Only untouched scrolls are ours to interpret.
        iced::Event::Mouse(mouse::Event::WheelScrolled { delta })
            if status == event::Status::Ignored =>
        {
            let y = match delta {
                mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => y,
            };
            (y != 0.0).then_some(Message::Wheel { up: y > 0.0 })
        }

        // Only keys no widget wanted are shortcuts. Without this a field with
        // the cursor in it shares its arrows with the pages behind the panel,
        // and Canc takes a piece out of the template while a name is being
        // typed. `Ignored` is the only signal there is that something has the
        // keyboard, and it is the same rule `iced::keyboard::listen` applies.
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
            if status == event::Status::Ignored =>
        {
            match key {
                keyboard::Key::Named(Named::ArrowLeft | Named::PageUp) => Some(Message::PageBack),
                keyboard::Key::Named(Named::ArrowRight | Named::PageDown) => {
                    Some(Message::PageForward)
                }
                // The selected piece, out of the box. Backspace as well as
                // Canc: on a laptop keyboard it is often the only one there.
                keyboard::Key::Named(Named::Delete | Named::Backspace) => {
                    Some(Message::PieceRemoved)
                }
                // The shortcuts the menus advertise, plus the zoom ones that
                // were here before there were any menus to advertise them.
                keyboard::Key::Character(ref c) if modifiers.command() => {
                    match c.to_lowercase().as_str() {
                        "+" | "=" => Some(Message::ZoomIn),
                        "-" => Some(Message::ZoomOut),
                        "0" => Some(Message::ZoomReset),
                        "o" => Some(Message::ChooseDocument),
                        "s" => Some(Message::ChooseOutput),
                        "q" => Some(Message::Quit),
                        "t" => Some(Message::ShowTemplates),
                        // Both spellings of putting a change back, because
                        // both are somebody's habit.
                        "z" if modifiers.shift() => Some(Message::Redo),
                        "y" => Some(Message::Redo),
                        "z" => Some(Message::Undo),
                        _ => None,
                    }
                }
                // Whatever is laid over the window goes away on Escape. The
                // card is not interrupted this way: the signing panel does not
                // listen.
                keyboard::Key::Named(Named::Escape) => Some(Message::OverlayDismissed),
                _ => None,
            }
        }
        _ => None,
    }
}

fn subscription(state: &State) -> Subscription<Message> {
    // The reader pushes: the runtime holds the stream open for as long as the
    // window lives and wakes up only when something is actually plugged in.
    let mut subscriptions = vec![
        event::listen_with(input),
        Subscription::run(|| job::watch().map(Message::CardChanged)),
    ];

    // The timer only exists while a thread has something to say. The reader
    // needs none: it pushes.
    if state.busy() {
        subscriptions.push(time::every(TICK).map(|_| Message::Tick));
    }

    Subscription::batch(subscriptions)
}
