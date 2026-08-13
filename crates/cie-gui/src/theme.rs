//! The palette, and the widget styles built on it.
//!
//! Colours come from the Italian public administration design system
//! (Designers Italia / Bootstrap Italia): institutional blue on white, squared
//! corners rather than rounded. The point is that a tool for signing with an
//! identity document should look like the paperwork it produces, not like a
//! debug panel.

use iced::font::Weight;
use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Font, Theme};

/// Titillium Web, the face of the Italian public administration design system.
///
/// Shipped inside the binary rather than looked up on the system: it is not
/// installed by default anywhere, and a tool that silently falls back to
/// whatever the desktop has is a tool that looks different on every machine.
/// SIL Open Font License 1.1 - `fonts/OFL.txt` travels with it.
pub const UI: Font = Font::with_name("Titillium Web");

/// The same face with weight, for titles. egui had no bold at all, which is
/// why the old stamp mock-up faked it by darkening the grey.
pub const UI_STRONG: Font = Font {
    weight: Weight::Semibold,
    ..Font::with_name("Titillium Web")
};

/// Which look the window wears.
///
/// `System` is the default: the desktop already knows what the user wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    System,
    Light,
    Dark,
}

impl Mode {
    /// Resolves to a real theme, asking the desktop when told to.
    ///
    /// A desktop that will not say is treated as light: the document in the
    /// middle of the window is white paper either way.
    /// Cycles system → light → dark → system.
    pub fn next(self) -> Self {
        match self {
            Mode::System => Mode::Light,
            Mode::Light => Mode::Dark,
            Mode::Dark => Mode::System,
        }
    }

    pub fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            Mode::System => rust_i18n::t!("theme.system"),
            Mode::Light => rust_i18n::t!("theme.light"),
            Mode::Dark => rust_i18n::t!("theme.dark"),
        }
    }

    pub fn theme(self) -> Theme {
        let dark = match self {
            Mode::Light => false,
            Mode::Dark => true,
            Mode::System => matches!(dark_light::detect(), Ok(dark_light::Mode::Dark)),
        };
        if dark { dark_theme() } else { light_theme() }
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
}

/// The canvas the document sits on - deliberately not the surface colour, so
/// the page reads as a sheet of paper rather than part of the chrome.
pub const DESK_LIGHT: Color = rgb(0xF2, 0xF7, 0xFC);
pub const DESK_DARK: Color = rgb(0x16, 0x22, 0x2E);

pub const SURFACE_LIGHT: Color = Color::WHITE;
pub const SURFACE_DARK: Color = rgb(0x22, 0x38, 0x4E);

pub const BORDER_LIGHT: Color = rgb(0xC5, 0xD2, 0xDE);
pub const BORDER_DARK: Color = rgb(0x35, 0x50, 0x6B);

/// Bootstrap Italia is square: 4 px, not the 8–12 px of a rounded design.
pub const RADIUS: f32 = 4.0;

fn light_theme() -> Theme {
    Theme::custom(
        "Designers Italia".to_string(),
        iced::theme::Palette {
            background: DESK_LIGHT,
            text: rgb(0x17, 0x32, 0x4D),
            primary: rgb(0x00, 0x66, 0xCC),
            success: rgb(0x00, 0x87, 0x58),
            warning: rgb(0xA6, 0x63, 0x00),
            danger: rgb(0xD9, 0x36, 0x4F),
        },
    )
}

fn dark_theme() -> Theme {
    Theme::custom(
        "Designers Italia scuro".to_string(),
        iced::theme::Palette {
            background: DESK_DARK,
            text: rgb(0xE8, 0xEE, 0xF4),
            primary: rgb(0x4F, 0xA3, 0xFF),
            success: rgb(0x00, 0xCC, 0x85),
            warning: rgb(0xE5, 0xA5, 0x3E),
            danger: rgb(0xFF, 0x5E, 0x73),
        },
    )
}

fn is_dark(theme: &Theme) -> bool {
    theme.extended_palette().is_dark
}

/// A raised panel: what the sidebar sections and the modal are made of.
pub fn card(theme: &Theme) -> container::Style {
    let (bg, border) = if is_dark(theme) {
        (SURFACE_DARK, BORDER_DARK)
    } else {
        (SURFACE_LIGHT, BORDER_LIGHT)
    };
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: RADIUS.into(),
            width: 1.0,
            color: border,
        },
        text_color: None,
        ..Default::default()
    }
}

/// The desk the page lies on.
pub fn desk(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(if is_dark(theme) {
            DESK_DARK
        } else {
            DESK_LIGHT
        })),
        ..Default::default()
    }
}

/// The application bar and the action bar: same surface as a card, no radius,
/// one hairline separating them from the desk.
pub fn bar(theme: &Theme) -> container::Style {
    let (bg, border) = if is_dark(theme) {
        (SURFACE_DARK, BORDER_DARK)
    } else {
        (SURFACE_LIGHT, BORDER_LIGHT)
    };
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: 0.0.into(),
            width: 1.0,
            color: border,
        },
        ..Default::default()
    }
}

/// The veil that dims the window while the modal is up.
pub fn veil(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.55,
            ..Color::BLACK
        })),
        ..Default::default()
    }
}

/// The one primary action in the window.
pub fn primary(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: Some(Background::Color(palette.primary.base.color)),
        text_color: palette.primary.base.text,
        border: Border {
            radius: RADIUS.into(),
            ..Default::default()
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(palette.primary.strong.color)),
            ..base
        },
        // Disabled has to read as unavailable without vanishing: a Firma button
        // you cannot see is a bug report waiting to happen.
        button::Status::Disabled => button::Style {
            background: Some(Background::Color(Color {
                a: 0.35,
                ..palette.primary.base.color
            })),
            text_color: Color {
                a: 0.6,
                ..palette.primary.base.text
            },
            ..base
        },
        button::Status::Active => base,
    }
}

/// Everything that is not the primary action: outlined, never filled.
pub fn secondary(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let base = button::Style {
        background: None,
        text_color: palette.primary.base.color,
        border: Border {
            radius: RADIUS.into(),
            width: 1.0,
            color: palette.primary.base.color,
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Background::Color(Color {
                a: 0.10,
                ..palette.primary.base.color
            })),
            ..base
        },
        button::Status::Disabled => button::Style {
            text_color: Color {
                a: 0.4,
                ..palette.primary.base.color
            },
            border: Border {
                radius: RADIUS.into(),
                width: 1.0,
                color: Color {
                    a: 0.3,
                    ..palette.primary.base.color
                },
            },
            ..base
        },
        button::Status::Active => base,
    }
}

/// A button that reads as a link: for the third-rank actions in a card.
pub fn quiet(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    button::Style {
        background: None,
        text_color: match status {
            button::Status::Disabled => Color {
                a: 0.4,
                ..palette.background.base.text
            },
            button::Status::Hovered | button::Status::Pressed => palette.primary.strong.color,
            button::Status::Active => palette.primary.base.color,
        },
        border: Border::default(),
        ..Default::default()
    }
}

pub fn field(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let palette = theme.extended_palette();
    let (bg, border) = if is_dark(theme) {
        (DESK_DARK, BORDER_DARK)
    } else {
        (SURFACE_LIGHT, BORDER_LIGHT)
    };

    text_input::Style {
        background: Background::Color(bg),
        border: Border {
            radius: RADIUS.into(),
            width: if matches!(status, text_input::Status::Focused { .. }) {
                2.0
            } else {
                1.0
            },
            color: if matches!(status, text_input::Status::Focused { .. }) {
                palette.primary.base.color
            } else {
                border
            },
        },
        icon: palette.background.weak.text,
        placeholder: Color {
            a: 0.5,
            ..palette.background.base.text
        },
        value: palette.background.base.text,
        selection: Color {
            a: 0.3,
            ..palette.primary.base.color
        },
    }
}
