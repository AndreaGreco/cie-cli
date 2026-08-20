//! The menu bar, and the list that drops out of it.
//!
//! Hand-rolled, after `pick_list` turned out to be the wrong widget: with
//! `Length::Shrink` it takes the width of its widest entry, so the title "File"
//! came out as wide as "Apri documento…", and the list it opens inherits the
//! title's width, so there is no narrowing one without clipping the other.
//!
//! What is left is arithmetic. A stack lays its layers out independently and
//! none of them can ask where another one ended up, so the bar has a fixed
//! height and each title a fixed width, and the open list works out its own
//! position from those. That is why [`BAR`] and [`Menu::width`] are constants
//! rather than measurements: they are the only thing holding the list under
//! the title it belongs to.

use iced::widget::{button, column, container, opaque, row, space, stack, text, MouseArea};
use iced::{Alignment, Element, Fill, Length};
use rust_i18n::t;

use crate::state::{Menu, Message, State};
use crate::theme;

/// Height of the application bar, and so the top of any open menu.
pub const BAR: f32 = 40.0;

/// Left padding of the bar: where the first title starts.
const PAD: f32 = 12.0;

/// Width of an open list. One number for all of them, so the menus line up
/// with each other instead of each being as wide as its own longest line.
const LIST: f32 = 250.0;

impl Menu {
    /// The menus, left to right. The order is the one in the bar, and
    /// [`Menu::offset`] reads it to know where each title starts.
    pub const ALL: [Menu; 2] = [Menu::File, Menu::Settings];

    /// Width of the title in the bar.
    ///
    /// Fixed, and wide enough for the longest translation of the label: too
    /// narrow and the next title starts on top of this one's list.
    fn width(self) -> f32 {
        match self {
            Menu::File => 62.0,
            Menu::Settings => 110.0,
        }
    }

    fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            Menu::File => t!("menu.file"),
            Menu::Settings => t!("menu.settings"),
        }
    }

    /// How far from the left edge of the window this title starts.
    fn offset(self) -> f32 {
        PAD + Self::ALL
            .iter()
            .take_while(|m| **m != self)
            .map(|m| m.width())
            .sum::<f32>()
    }

    /// What this menu offers right now.
    ///
    /// Entries appear only when they would do something. A menu is a list of
    /// what you can do, so one that fails on press is a lie about that — which
    /// is why "Salva con nome…" is missing until there is a document whose
    /// destination there is any point in choosing.
    fn entries(self, state: &State) -> Vec<Entry> {
        match self {
            Menu::File => {
                let mut entries = vec![Entry {
                    label: t!("menu.open_document"),
                    shortcut: "Ctrl+O",
                    message: Message::ChooseDocument,
                }];
                if state.path.is_some() {
                    entries.push(Entry {
                        label: t!("menu.save_as"),
                        shortcut: "Ctrl+S",
                        message: Message::ChooseOutput,
                    });
                }
                entries.push(Entry {
                    label: t!("menu.quit"),
                    shortcut: "Ctrl+Q",
                    message: Message::Quit,
                });
                entries
            }
            Menu::Settings => vec![Entry {
                label: t!("template.title"),
                shortcut: "Ctrl+T",
                message: Message::ShowTemplates,
            }],
        }
    }
}

/// One line of an open menu.
struct Entry {
    label: std::borrow::Cow<'static, str>,
    /// The keystroke that does the same thing. Shown rather than hidden: a
    /// menu is where you find out the program has shortcuts at all.
    shortcut: &'static str,
    message: Message,
}

/// The titles, for the application bar.
pub fn titles(state: &State) -> Element<'_, Message> {
    let mut bar = row![].align_y(Alignment::Center);

    for menu in Menu::ALL {
        let open = state.menu == Some(menu);
        bar = bar.push(
            button(text(menu.label().into_owned()).size(14))
                .width(Length::Fixed(menu.width()))
                .padding([7, 10])
                .style(if open {
                    theme::menu_title_open
                } else {
                    theme::menu_title
                })
                .on_press(Message::MenuToggled(menu)),
        );
    }

    bar.into()
}

/// The open list, as a layer over the whole window.
pub fn dropdown(open: Menu, state: &State) -> Element<'_, Message> {
    let mut list = column![].spacing(1);

    for entry in open.entries(state) {
        list = list.push(
            button(
                row![
                    text(entry.label.into_owned()).size(13),
                    space::horizontal(),
                    text(entry.shortcut)
                        .size(11)
                        .style(iced::widget::text::secondary),
                ]
                .spacing(24)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .padding([6, 10])
            .style(theme::menu_entry)
            .on_press(entry.message),
        );
    }

    let panel = container(list)
        .style(theme::floating)
        .padding(4)
        .width(Length::Fixed(LIST));

    // The catcher goes underneath the panel, not around it: around it, a click
    // on an entry would be a click on the catcher too, and the menu would
    // close on the way to deciding what the click meant.
    opaque(stack![
        MouseArea::new(space::vertical().width(Fill)).on_press(Message::MenuClosed),
        column![
            space::vertical().height(Length::Fixed(BAR)),
            row![space::horizontal().width(Length::Fixed(open.offset())), panel],
        ],
    ])
}
