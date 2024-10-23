use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn draw_header(f: &mut Frame<'_>, area: Rect) {
    // ASCII Art Logo
    let logo = r#"
     _____                 _
    / ____|               | |
   | (___  _   _ _ __ ___ | |__   ___
    \___ \| | | | '_ ` _ \| '_ \ / _ \
    ____) | |_| | | | | | | |_) | (_) |
   |_____/ \__,_|_| |_| |_|_.__/ \___/
    "#;

    // Create a block for the header background
    let block = Block::default()
        .style(Style::default().fg(Color::LightCyan).bg(Color::Black))
        .borders(Borders::NONE);

    f.render_widget(block, area);

    // Split the header area into two parts: logo and title
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(area);

    // Render the logo
    let logo_paragraph = Paragraph::new(logo)
        .style(
            Style::default()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Left);

    f.render_widget(logo_paragraph, chunks[0]);

    // Render the title
    let title = Paragraph::new("Sagacity - Elite Terminal Assistant")
        .style(
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
        )
        .alignment(Alignment::Center);

    f.render_widget(title, chunks[1]);
}
