use std::{error::Error, sync::Arc};

use crate::status_indicator::StatusIndicator;
use crate::{App, Chatbot, LogPanel, TreeNode};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};
use serde_json::{json, Value};
use std::env::var;
use textwrap::wrap;
use tokio::sync::Mutex;

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Renders the chat interface.
pub fn draw_chat(f: &mut Frame, app: &mut App) {
    let size = f.size();

    // Define the main horizontal layout: Chat area and Logs
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(2, 3), Constraint::Ratio(1, 3)])
        .margin(1)
        .split(size);

    // Define the vertical layout within the Chat area: Messages, Status, Input
    let chat_vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Messages
            Constraint::Length(2), // Status
            Constraint::Length(3), // Input
        ])
        .split(horizontal_chunks[0]);

    let messages_area = chat_vertical_chunks[0];

    // Prepare chat messages
    let mut lines = Vec::new();
    for (msg, from_user) in &app.chat_messages {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }

        let style = Style::default().fg(if *from_user {
            Color::Yellow
        } else {
            Color::Green
        });

        let indent = if *from_user { "  " } else { "" };
        lines.push(Line::from(vec![
            Span::styled(indent, style),
            Span::styled("│ ", style),
        ]));

        let mut in_code_block = false;
        let mut code_buffer = String::new();
        let mut text_buffer = String::new();

        for line in msg.lines() {
            if line.trim().starts_with("```") {
                if !text_buffer.is_empty() {
                    let wrapped = wrap(
                        &text_buffer,
                        (messages_area.width as usize).saturating_sub(4),
                    );
                    for wrapped_line in wrapped {
                        lines.push(Line::from(vec![
                            Span::styled(indent, style),
                            Span::styled("│ ", style),
                            Span::styled(wrapped_line.to_string(), style),
                        ]));
                    }
                    text_buffer.clear();
                }

                if !code_buffer.is_empty() {
                    for code_line in code_buffer.lines() {
                        lines.push(Line::from(vec![
                            Span::styled(indent, style),
                            Span::styled("│ ", style),
                            Span::styled("▎", Style::default().fg(Color::DarkGray)),
                            Span::styled(
                                format!(" {}", code_line),
                                Style::default().fg(Color::Rgb(209, 154, 102)),
                            ),
                        ]));
                    }
                    code_buffer.clear();
                }

                in_code_block = !in_code_block;
                continue;
            }

            if in_code_block {
                code_buffer.push_str(line);
                code_buffer.push('\n');
            } else {
                text_buffer.push_str(line);
                text_buffer.push('\n');
            }
        }

        if !text_buffer.is_empty() {
            let wrapped = wrap(
                &text_buffer,
                (messages_area.width as usize).saturating_sub(4),
            );
            for wrapped_line in wrapped {
                lines.push(Line::from(vec![
                    Span::styled(indent, style),
                    Span::styled("│ ", style),
                    Span::styled(wrapped_line.to_string(), style),
                ]));
            }
        }

        if !code_buffer.is_empty() {
            for code_line in code_buffer.lines() {
                lines.push(Line::from(vec![
                    Span::styled(indent, style),
                    Span::styled("│ ", style),
                    Span::styled("▎", Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        format!(" {}", code_line),
                        Style::default().fg(Color::Rgb(209, 154, 102)),
                    ),
                ]));
            }
        }

        lines.push(Line::from(vec![
            Span::styled(indent, style),
            Span::styled("╰─", style),
        ]));
    }

    let total_lines = lines.len() as u16;
    let available_height = messages_area.height;
    let max_scroll = if total_lines > available_height {
        total_lines - available_height
    } else {
        0
    };
    let chat_scroll = if app.chat_scroll > max_scroll {
        max_scroll
    } else {
        app.chat_scroll
    };

    let msgs_para = Paragraph::new(lines)
        .style(Style::default())
        .block(Block::default())
        .wrap(Wrap { trim: true });

    f.render_widget(msgs_para.scroll((chat_scroll, 0)), messages_area);

    // Render the status indicator
    app.status_indicator.update_spinner();
    app.status_indicator.render(f, chat_vertical_chunks[1]);

    // Render the input box
    let input_area = chat_vertical_chunks[2];
    let separator = "─".repeat(input_area.width as usize);

    // Top separator
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            &separator,
            Style::default().fg(Color::DarkGray),
        ))),
        Rect {
            x: input_area.x,
            y: input_area.y,
            width: input_area.width,
            height: 1,
        },
    );

    // Input line
    let input = Line::from(vec![
        Span::styled("→ ", Style::default().fg(Color::DarkGray)),
        Span::styled(&app.chat_input, Style::default().fg(Color::White)),
    ]);

    let visible_width = input_area.width.saturating_sub(2);
    let text_width = app.chat_input.len() as u16;
    let scroll_offset = if text_width > visible_width {
        text_width - visible_width
    } else {
        0
    };

    f.render_widget(
        Paragraph::new(input).scroll((0, scroll_offset)),
        Rect {
            x: input_area.x,
            y: input_area.y + 1,
            width: input_area.width,
            height: input_area.height - 2,
        },
    );

    // Bottom separator
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            &separator,
            Style::default().fg(Color::DarkGray),
        ))),
        Rect {
            x: input_area.x,
            y: input_area.y + input_area.height - 1,
            width: input_area.width,
            height: 1,
        },
    );

    // Set cursor position
    let cursor_x = input_area.x + 2 + app.chat_input.len() as u16 - scroll_offset;
    f.set_cursor_position((cursor_x, input_area.y + 1));

    // Render the Logs section
    let log_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(8)])
        .split(horizontal_chunks[1]);

    let vsep = "│".repeat(size.height as usize - 2);
    f.render_widget(
        Paragraph::new(vsep).style(Style::default().fg(Color::DarkGray)),
        Rect {
            x: horizontal_chunks[1].x - 1,
            y: 1,
            width: 1,
            height: size.height - 2,
        },
    );

    let log_lines: Vec<Line> = app
        .logs
        .entries
        .iter()
        .map(|entry| {
            Line::from(vec![
                Span::styled("• ", Style::default().fg(Color::DarkGray)),
                Span::raw(entry),
            ])
        })
        .collect();

    let total_log_lines = log_lines.len() as u16;
    let log_available_height = log_chunks[0].height;

    let max_log_scroll = if total_log_lines > log_available_height {
        total_log_lines - log_available_height
    } else {
        0
    };
    let logs_scroll = if app.logs_scroll > max_log_scroll {
        max_log_scroll
    } else {
        app.logs_scroll
    };

    let logs_para = Paragraph::new(log_lines)
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true });

    f.render_widget(logs_para.scroll((logs_scroll, 0)), log_chunks[0]);
}

/// Parses markdown text into ratatui::text::Spans
fn parse_markdown(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let parser = pulldown_cmark::Parser::new_ext(text, pulldown_cmark::Options::all());

    let mut bold = false;
    let mut italic = false;

    for event in parser {
        match event {
            pulldown_cmark::Event::Start(tag) => match tag {
                pulldown_cmark::Tag::Emphasis => italic = true,
                pulldown_cmark::Tag::Strong => bold = true,
                _ => {}
            },
            pulldown_cmark::Event::End(tag) => match tag {
                pulldown_cmark::TagEnd::Emphasis => italic = false,
                pulldown_cmark::TagEnd::Strong => bold = false,
                _ => {}
            },
            pulldown_cmark::Event::Text(text) => {
                let mut style = Style::default();
                if bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if italic {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                spans.push(Span::styled(text.to_string(), style));
            }
            _ => {}
        }
    }

    spans
}

/// Handles the simulation of chat responses.
pub async fn simulate_chat_response(app: Arc<Mutex<App>>, user_input: String) {
    {
        let mut guard = app.lock().await;
        guard.chat_thinking = true;
        guard.chat_input.clear();
        guard.logs.add("Processing query...");
        guard.status_indicator.set_thinking(true);
        guard.status_indicator.set_status("Thinking...");
    }

    let context = {
        let guard = app.lock().await;
        let mut ctx = String::new();
        for (file, (summary, _)) in &guard.chatbot.index {
            ctx.push_str(&format!("File: {}\nSummary: {}\n\n", file, summary));
        }
        ctx
    };

    let prompt = format!(
        "Based on this codebase context:\n{}\n\nAnswer this question: {}",
        context, user_input
    );

    let response = match get_claude_response(&prompt, &[]).await {
        Ok(response) => {
            let mut guard = app.lock().await;
            guard.logs.add("Response received");
            response
        }
        Err(e) => {
            let mut guard = app.lock().await;
            guard.logs.add(format!("Error: {}", e));
            "I encountered an error processing your request.".to_string()
        }
    };

    {
        let mut guard = app.lock().await;
        guard.chat_messages.push((response, false));
        guard.chat_thinking = false;
        guard.status_indicator.set_thinking(false);
        guard.status_indicator.set_status("");
        guard.logs.add("Response complete");
    }
}

/// Sends a request to the Claude API and retrieves the response.
pub async fn get_claude_response(
    user_input: &str,
    history: &[Value],
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let api_key = var("ANTHROPIC_API_KEY")?;

    let mut messages = history.to_vec();
    messages.push(json!({
        "role": "user",
        "content": user_input
    }));

    let payload = json!({
        "model": "claude-3-opus-20240229",
        "max_tokens": 1024,
        "messages": messages,
        "temperature": 0.7
    });

    let client = reqwest::Client::new();
    let response = client
        .post(CLAUDE_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&payload)
        .send()
        .await?;

    let response_data: Value = response.json().await?;
    Ok(response_data["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string())
}

/// Summarizes the content of a file using the Claude API.
pub async fn summarize_file(
    content: &str,
    language: &str,
    api_key: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let client = reqwest::Client::new();
    let prompt = format!(
        "Please analyze this {} code and provide a brief summary of its purpose and functionality.\n\nCode:\n{}",
        language, content
    );

    let payload = json!({
        "model": "claude-3-opus-20240229",
        "max_tokens": 1024,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": 0.7
    });

    let response = client
        .post(CLAUDE_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&payload)
        .send()
        .await?;

    let body: Value = response.json().await?;
    Ok(body["content"][0]["text"]
        .as_str()
        .unwrap_or("Sorry, I couldn't process that request.")
        .to_string())
}
