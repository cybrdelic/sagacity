use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};
pub fn draw_quit_confirm(f: &mut Frame<'_>, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Confirm Quit")
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black));

    f.render_widget(block, area);

    // Define confirmation text
    let quit_text = "🚪 **Are you sure you want to quit?**\n\nPress **'y'** to confirm quit or **'n'** to cancel.";

    let paragraph = Paragraph::new(quit_text)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}
