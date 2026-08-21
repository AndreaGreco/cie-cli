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
//! the title it belongs to. A submenu is the same sum one level down, which
//! is why an entry has a fixed [`ENTRY`] height as well.

use iced::widget::{button, column, container, opaque, row, space, stack, text, MouseArea};
use iced::{Alignment, Element, Fill, Length};
use rust_i18n::t;

use crate::state::{Menu, Message, State, Submenu};
use crate::theme::{self, Mode};

/// Height of the application bar, and so the top of any open menu.
pub const BAR: f32 = 40.0;

/// Left padding of the bar: where the first title starts.
const PAD: f32 = 12.0;

/// Width of an open list. One number for all of them, so the menus line up
/// with each other instead of each being as wide as its own longest line.
const LIST: f32 = 250.0;

/// Width of a submenu. Narrower than [`LIST`]: it hangs off an entry rather
/// than off the bar, so it has nothing to line up with, and three one-word
/// alternatives in a list as wide as the menu that opened them look like a
/// panel that lost its contents.
const SUB: f32 = 170.0;

/// Height of one entry, and the gap the column leaves under it.
///
/// Fixed for the same reason the bar is: a submenu has to start level with the
/// entry that opened it, and no layer can ask another where anything ended up.
const ENTRY: f32 = 28.0;
const GAP: f32 = 1.0;

/// The panel's own padding, between its border and the first entry.
const INSET: f32 = 4.0;

/// The mark against the setting already in force, and the arrow on an entry
/// with a list of its own.
const TICK: &str = "✓";
const ARROW: &str = "▸";

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
    /// what you can do, so one that fails on press is a lie about that - which
    /// is why "Salva con nome…" is missing until there is a document whose
    /// destination there is any point in choosing.
    fn entries(self, state: &State) -> Vec<Entry> {
        match self {
            Menu::File => {
                let mut entries = vec![Entry::new(
                    t!("menu.open_document"),
                    "Ctrl+O",
                    Message::ChooseDocument,
                )];
                if state.path.is_some() {
                    entries.push(Entry::new(
                        t!("menu.save_as"),
                        "Ctrl+S",
                        Message::ChooseOutput,
                    ));
                }
                entries.push(Entry::new(t!("menu.quit"), "Ctrl+Q", Message::Quit));
                entries
            }
            Menu::Settings => vec![
                Entry::new(t!("template.title"), "Ctrl+T", Message::ShowTemplates),
                // The look used to be one button in the bar that cycled
                // through the three, which told nobody what the other two
                // were. A list of them says so, and says which one is on.
                Entry::new(
                    t!("theme.title"),
                    ARROW,
                    Message::SubmenuToggled(Submenu::Theme),
                )
                .opening(Submenu::Theme),
            ],
        }
    }
}

impl Submenu {
    /// The alternatives, with a tick against the one in force.
    fn entries(self, state: &State) -> Vec<Entry> {
        match self {
            Submenu::Theme => Mode::ALL
                .iter()
                .map(|&mode| {
                    Entry::new(
                        mode.label(),
                        if state.mode == mode { TICK } else { "" },
                        Message::ThemeChanged(mode),
                    )
                })
                .collect(),
        }
    }

    /// Where this list starts, given the menu it hangs off.
    ///
    /// To the right of that menu's panel, level with the entry that opens it -
    /// found by counting the entries above it, since that is the only way one
    /// layer of a stack learns anything about another.
    fn position(self, parent: Menu, state: &State) -> Option<(f32, f32)> {
        let above = parent
            .entries(state)
            .iter()
            .position(|entry| entry.opens == Some(self))?;
        // No inset in the sum: this panel's own padding puts its first entry
        // exactly where the parent's padding put the entry that opened it, so
        // leaving it out of both is what makes the two start level.
        Some((parent.offset() + LIST, BAR + above as f32 * (ENTRY + GAP)))
    }
}

/// One line of an open menu.
struct Entry {
    label: std::borrow::Cow<'static, str>,
    /// The right-hand slot: the keystroke that does the same thing, a tick
    /// where the entry is the setting already in force, or the arrow of an
    /// entry with a list behind it. Shortcuts are shown rather than hidden,
    /// because a menu is where you find out the program has any.
    hint: std::borrow::Cow<'static, str>,
    message: Message,
    /// The list this entry opens beside itself, if it opens one. Both the way
    /// the entry stays lit while that list is out and the way that list works
    /// out where to start are read from this.
    opens: Option<Submenu>,
}

impl Entry {
    fn new(
        label: impl Into<std::borrow::Cow<'static, str>>,
        hint: impl Into<std::borrow::Cow<'static, str>>,
        message: Message,
    ) -> Self {
        Self {
            label: label.into(),
            hint: hint.into(),
            message,
            opens: None,
        }
    }

    fn opening(mut self, submenu: Submenu) -> Self {
        self.opens = Some(submenu);
        self
    }
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

/// One list, drawn as a floating panel.
fn panel<'a>(entries: Vec<Entry>, width: f32, state: &State) -> Element<'a, Message> {
    let mut list = column![].spacing(GAP);

    for entry in entries {
        let out = entry.opens.is_some() && entry.opens == state.submenu;
        list = list.push(
            button(
                row![
                    text(entry.label.into_owned()).size(13),
                    space::horizontal(),
                    text(entry.hint.into_owned())
                        .size(11)
                        .style(iced::widget::text::secondary),
                ]
                .spacing(24)
                .height(Fill)
                .align_y(Alignment::Center),
            )
            .width(Fill)
            .height(Length::Fixed(ENTRY))
            .padding([0, 10])
            .style(if out {
                theme::menu_entry_open
            } else {
                theme::menu_entry
            })
            .on_press(entry.message),
        );
    }

    container(list)
        .style(theme::floating)
        .padding(INSET)
        .width(Length::Fixed(width))
        .into()
}

/// A panel put where the arithmetic says it goes, as its own layer.
fn placed<'a>(panel: Element<'a, Message>, (left, top): (f32, f32)) -> Element<'a, Message> {
    column![
        space::vertical().height(Length::Fixed(top)),
        row![space::horizontal().width(Length::Fixed(left)), panel],
    ]
    .into()
}

/// The open list, and whatever hangs off it, as a layer over the whole window.
pub fn dropdown(open: Menu, state: &State) -> Element<'_, Message> {
    // The catcher goes underneath the panels, not around them: around them, a
    // click on an entry would be a click on the catcher too, and the menu
    // would close on the way to deciding what the click meant.
    let mut layers = stack![
        MouseArea::new(space::vertical().width(Fill)).on_press(Message::MenuClosed),
        placed(panel(open.entries(state), LIST, state), (open.offset(), BAR)),
    ];

    if let Some((submenu, at)) = state
        .submenu
        .and_then(|sub| Some((sub, sub.position(open, state)?)))
    {
        layers = layers.push(placed(panel(submenu.entries(state), SUB, state), at));
    }

    opaque(layers)
}
