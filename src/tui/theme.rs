//! Dark, high-contrast palette with bold per-service accents.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders, Padding};

pub const BG: Color = Color::Rgb(0x12, 0x12, 0x14);
pub const SURFACE: Color = Color::Rgb(0x1c, 0x1c, 0x20);
pub const SURFACE_2: Color = Color::Rgb(0x2a, 0x2a, 0x32);
pub const FG: Color = Color::Rgb(0xea, 0xea, 0xf0);
pub const FG_DIM: Color = Color::Rgb(0x8a, 0x8a, 0x94);

pub const RADARR: Color = Color::Rgb(0xff, 0xc2, 0x30); // gold
pub const SONARR: Color = Color::Rgb(0x35, 0xc5, 0xf4); // sky blue
pub const PLEX: Color = Color::Rgb(0xe5, 0xa0, 0x0d); // amber
pub const QBIT: Color = Color::Rgb(0x4f, 0x8f, 0xef); // blue
pub const NZBGET: Color = Color::Rgb(0x54, 0xd6, 0x62); // green

pub const ACCENT: Color = Color::Rgb(0xb4, 0x8c, 0xff); // app accent (violet)
pub const SUCCESS: Color = Color::Rgb(0x54, 0xd6, 0x62);
pub const WARN: Color = Color::Rgb(0xff, 0xc2, 0x30);
pub const ERROR: Color = Color::Rgb(0xff, 0x5c, 0x7a);

pub const SELECT_MARKER: &str = "▍";
pub const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Radarr,
    Sonarr,
    Plex,
    Qbit,
    Nzbget,
}

impl Service {
    pub fn accent(self) -> Color {
        match self {
            Service::Radarr => RADARR,
            Service::Sonarr => SONARR,
            Service::Plex => PLEX,
            Service::Qbit => QBIT,
            Service::Nzbget => NZBGET,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Service::Radarr => "RADARR",
            Service::Sonarr => "SONARR",
            Service::Plex => "PLEX",
            Service::Qbit => "QBIT",
            Service::Nzbget => "NZBGET",
        }
    }
}

pub fn base() -> Style {
    Style::new().fg(FG).bg(BG)
}

pub fn dim() -> Style {
    Style::new().fg(FG_DIM)
}

pub fn accent_bold(color: Color) -> Style {
    Style::new().fg(color).add_modifier(Modifier::BOLD)
}

pub fn selected_row() -> Style {
    Style::new().fg(FG).bg(SURFACE_2).add_modifier(Modifier::BOLD)
}

/// Rounded, padded panel. Focused panels get their accent border.
pub fn panel(title: &str, accent: Color, focused: bool) -> Block<'_> {
    let border = if focused { accent } else { FG_DIM };
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .padding(Padding::horizontal(1))
        .title(format!(" {title} "))
        .title_style(accent_bold(if focused { accent } else { FG_DIM }))
}
