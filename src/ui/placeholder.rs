use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

pub fn draw_placeholder(f: &mut Frame<'_>, area: Rect, title: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black));

    f.render_widget(block, area);

    let placeholder_text = match title {
        "Chat" => "💬 Chat functionality will be implemented here.\n\nPress 'q' or Esc to return to the main menu.",
        "Browse Index" => "📂 Browse Index functionality is under construction.\n\nPress 'q' or Esc to return to the main menu.",
        "GitHub Recommendations" => "🔍 GitHub Recommendations functionality is under construction.\n\nPress 'q' or Esc to return to the main menu.",
        "Help" => "❓ Help information will be available here.\n\nPress 'q' or Esc to return to the main menu.",
        "Settings" => "⚙️ Settings will be configurable here.\n\nPress 'q' or Esc to return to the main menu.",
        _ => "Under construction.\n\nPress 'q' or Esc to return to the main menu.",
    };

    let paragraph = Paragraph::new(placeholder_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}
