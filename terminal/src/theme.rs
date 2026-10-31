//! Semantic roles shared by every widget; Auto preserves terminal foreground/background.

use ratatui::style::{Color, Modifier, Style};

/// Presentation preference. Execution and model settings are not part of a theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Auto,
    Dark,
    Light,
    Contrast,
    NoColor,
}

impl Theme {
    /// Picker choices in display order.
    pub const ALL: [Self; 5] = [
        Self::Auto,
        Self::Dark,
        Self::Light,
        Self::Contrast,
        Self::NoColor,
    ];

    /// Human-readable preference name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Auto (terminal)",
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Contrast => "High contrast (ANSI)",
            Self::NoColor => "No color",
        }
    }

    /// Resolves roles once for a frame without terminal palette queries.
    pub fn palette(self) -> Palette {
        let (foreground, background, muted, accent) = match self {
            Self::Auto => (Color::Reset, Color::Reset, Color::Gray, Color::Cyan),
            Self::Dark => (Color::White, Color::Black, Color::Gray, Color::Cyan),
            Self::Light => (Color::Black, Color::White, Color::DarkGray, Color::Blue),
            Self::Contrast => (Color::White, Color::Black, Color::White, Color::Yellow),
            Self::NoColor => (Color::Reset, Color::Reset, Color::Reset, Color::Reset),
        };
        Palette {
            base: Style::default().fg(foreground).bg(background),
            muted: Style::default()
                .fg(muted)
                .bg(background)
                .add_modifier(Modifier::DIM),
            accent: Style::default().fg(accent).bg(background),
        }
    }
}

/// Resolved widget styles. Status meaning also uses text, never color alone.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub base: Style,
    pub muted: Style,
    pub accent: Style,
}
