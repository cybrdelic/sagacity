use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::App;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sender {
    User,
    AI,
}

/// Represents a chat message
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub sender: Sender,
    pub content: String,
}

pub fn draw_chat(f: &mut Frame<'_>, area: Rect, app: &App) {
    // Create a block for the chat background
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Chat")
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black));

    f.render_widget(block, area);

    // Split chat area into message view and input
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Min(1),    // Messages
                Constraint::Length(3), // Input
            ]
            .as_ref(),
        )
        .split(area);

    // Render messages
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|msg| {
            let prefix = match msg.sender {
                Sender::User => "💬 You: ",
                Sender::AI => "🤖 AI: ",
            };
            ListItem::new(format!("{}{}", prefix, msg.content)).style(
                Style::default()
                    .fg(match msg.sender {
                        Sender::User => Color::LightGreen,
                        Sender::AI => Color::LightBlue,
                    })
                    .add_modifier(Modifier::ITALIC),
            )
        })
        .collect();

    let messages_list = List::new(messages)
        .block(Block::default())
        .style(Style::default())
        .highlight_style(Style::default())
        .highlight_symbol("");

    f.render_widget(messages_list, chunks[0]);

    // Render input box
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::LightYellow))
        .block(Block::default().borders(Borders::ALL).title("Input"))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(input, chunks[1]);

    // Set cursor position
    let x = chunks[1].x + app.input.len() as u16 + 1;
    let y = chunks[1].y + 1;
    f.set_cursor(x, y);
}
