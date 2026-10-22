use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders},
};

pub const BACKGROUND: Color = Color::Rgb(8, 12, 22);
pub const SURFACE: Color = Color::Rgb(14, 20, 34);
pub const ELEVATED: Color = Color::Rgb(21, 30, 48);
pub const BORDER: Color = Color::Rgb(43, 57, 82);
pub const TEXT: Color = Color::Rgb(230, 237, 247);
pub const MUTED: Color = Color::Rgb(133, 149, 174);
pub const DIM: Color = Color::Rgb(77, 93, 119);
pub const CYAN: Color = Color::Rgb(62, 220, 210);
pub const PURPLE: Color = Color::Rgb(170, 130, 255);
pub const PINK: Color = Color::Rgb(255, 111, 181);
pub const GREEN: Color = Color::Rgb(91, 215, 147);
pub const YELLOW: Color = Color::Rgb(247, 194, 91);
pub const RED: Color = Color::Rgb(255, 111, 120);

pub fn panel(title: impl Into<String>, accent: Color) -> Block<'static> {
    Block::default()
        .title(title.into())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER).bg(SURFACE))
        .title_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(SURFACE))
}
