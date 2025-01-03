use crate::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sender {
    User,
    AI,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub sender: Sender,
    pub content: String,
}

const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn draw_chat(f: &mut Frame<'_>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Min(1),    // Messages area
                Constraint::Length(1), // Processing status
                Constraint::Length(3), // Input area
            ]
            .as_ref(),
        )
        .split(area);

    // Draw message history
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|msg| {
            let prefix = match msg.sender {
                Sender::User => "💬 You: ",
                Sender::AI => "🤖 AI: ",
            };

            let content = format!("{}{}", prefix, msg.content);
            let style = Style::default().fg(match msg.sender {
                Sender::User => Color::LightGreen,
                Sender::AI => Color::LightBlue,
            });

            let wrap_width = chunks[0].width.saturating_sub(4) as usize;
            let wrapped_lines: Vec<Line> = textwrap::wrap(&content, wrap_width)
                .into_iter()
                .map(|s| Line::from(vec![Span::styled(s.to_string(), style)]))
                .collect();

            ListItem::new(wrapped_lines)
        })
        .collect();

    let messages_list = List::new(messages)
        .block(Block::default().borders(Borders::ALL).title("Chat History"))
        .style(Style::default());

    f.render_widget(messages_list, chunks[0]);

    // Draw processing status
    if app.is_processing {
        let spinner = SPINNER_FRAMES[app.processing_frame];
        let status = Paragraph::new(format!("{} Processing query...", spinner))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(status, chunks[1]);
    }

    // Draw input area
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Input"))
        .wrap(Wrap { trim: true });

    f.render_widget(input, chunks[2]);

    // Set cursor position in input area
    let input_cursor_x = chunks[2].x + app.input.len() as u16 + 1;
    let input_cursor_y = chunks[2].y + 1;
    f.set_cursor_position((input_cursor_x, input_cursor_y));
}
