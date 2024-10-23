use crate::{App, AppState};
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Paragraph, Wrap},
    Frame,
};
/// Draws the footer with dynamic instructions
pub fn draw_footer(f: &mut Frame<'_>, area: Rect, app: &App) {
    let instructions = match app.state {
        AppState::MainMenu => {
            "Use Up/Down arrows to navigate, Enter to select, 'q' or Esc to quit."
        }
        AppState::Chat => "Type your message and press Enter to send. Esc to return to main menu.",
        AppState::QuitConfirm => "Press 'y' to confirm quit or 'n' to cancel.",
        _ => "Press 'q' or Esc to quit.",
    };

    let footer = Paragraph::new(instructions)
        .style(Style::default().fg(Color::LightCyan))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(footer, area);
}
