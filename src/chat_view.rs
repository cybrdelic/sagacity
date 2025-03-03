use crate::chat_message::ChatMessage;
use crate::App;
use dotenv::var;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
    Frame,
};
use serde_json::{json, Value};
use std::{error::Error, sync::Arc};
use tokio::sync::Mutex;

// These constants are moved to api.rs
pub use crate::api::{CLAUDE_API_URL, ANTHROPIC_VERSION};

pub fn draw_chat(f: &mut Frame, app: &mut App) {
    let size = f.area();
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(2, 3), Constraint::Ratio(1, 3)])
        .margin(1)
        .split(size);

    let chat_vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Min(1),
                Constraint::Length(2),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(horizontal_chunks[0]);

    let messages_area = chat_vertical_chunks[0];
    draw_messages(f, app, messages_area);

    app.status_indicator.update_spinner();
    app.status_indicator.render(f, chat_vertical_chunks[1]);

    draw_input(f, app, chat_vertical_chunks[2]);
    draw_logs(f, app, horizontal_chunks[1], size);
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();
    for (_idx, message) in app.chat_messages.iter().enumerate() {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        let message_lines = message.render(area);
        lines.extend(message_lines);
    }
    let total_lines = lines.len() as u16;
    let available_height = area.height;
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
    f.render_widget(msgs_para.scroll((chat_scroll, 0)), area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let separator = "─".repeat(area.width as usize);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            &separator,
            Style::default().fg(Color::DarkGray),
        ))),
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        },
    );

    // Show history indicator prefix when navigating command history
    let prefix = if app.command_index.is_some() {
        "⌃ "  // Changed to Ctrl symbol to reflect Ctrl+Up/Down usage
    } else {
        "→ "
    };
    
    let prefix_style = if app.command_index.is_some() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    
    let input = Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(&app.chat_input, Style::default().fg(Color::White)),
    ]);

    let visible_width = area.width.saturating_sub(2);
    let text_width = app.chat_input.len() as u16;
    let scroll_offset = if text_width > visible_width {
        text_width - visible_width
    } else {
        0
    };

    f.render_widget(
        Paragraph::new(input).scroll((0, scroll_offset)),
        Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height - 2,
        },
    );

    // Add a history mode indicator when browsing history
    if app.command_index.is_some() {
        let history_idx = app.command_index.unwrap() + 1;
        let history_len = app.command_history.len();
        let history_text = format!(" [Ctrl History {}/{}] ", history_idx, history_len);
        
        let history_indicator = Paragraph::new(Line::from(Span::styled(
            history_text.clone(),
            Style::default().fg(Color::Yellow).bg(Color::Black),
        )));
        
        let indicator_width = history_text.len() as u16;
        let indicator_x = area.x + area.width - indicator_width;
        
        f.render_widget(
            history_indicator,
            Rect {
                x: indicator_x,
                y: area.y + 1,
                width: indicator_width,
                height: 1,
            },
        );
    }

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            &separator,
            Style::default().fg(Color::DarkGray),
        ))),
        Rect {
            x: area.x,
            y: area.y + area.height - 1,
            width: area.width,
            height: 1,
        },
    );

    let cursor_x = area.x + 2 + app.chat_input.len() as u16 - scroll_offset;
    f.set_cursor_position((cursor_x, area.y + 1));
}

fn draw_logs(f: &mut Frame, app: &App, area: Rect, size: Rect) {
    let log_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(8)].as_ref())
        .split(area);

    let vsep = "│".repeat(size.height as usize - 2);
    f.render_widget(
        Paragraph::new(Span::raw(vsep)).style(Style::default().fg(Color::DarkGray)),
        Rect {
            x: area.x - 1,
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

pub async fn simulate_chat_response(app: Arc<Mutex<App>>, user_input: String) {
    {
        let mut guard = app.lock().await;
        guard.chat_thinking = true;
        guard.chat_input.clear();
        guard.logs.add("Processing query...".to_string());
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

    // <<< ADDED >>>
    // Build a final prompt containing the codebase context and user question.
    let final_prompt = format!(
        "Based on this codebase context:\n{}\n\nAnswer this question: {}",
        context, user_input
    );

    {
        let mut guard = app.lock().await;
        guard.logs.add("Sending request to Claude API...".to_string());

        // Log a small snippet of the prompt to confirm what is being sent.
        let snippet = if final_prompt.len() > 120 {
            format!("{}...", &final_prompt[..120])
        } else {
            final_prompt.clone()
        };
        guard.logs.add(format!("Prompt snippet: \"{}\"", snippet));
    }

    // <<< CHANGED >>> Use final_prompt instead of `prompt`
    match get_claude_response(&final_prompt, &[]).await {
        Ok(response_data) => {
            {
                // <<< ADDED >>>
                let mut guard = app.lock().await;
                guard.logs.add("Claude API call success!".to_string());
                if response_data.content.len() < 500 {
                    guard.logs.add(format!(
                        "Claude response content: {}",
                        response_data.content
                    ));
                } else {
                    guard.logs.add(format!(
                        "Claude response content length: {} chars",
                        response_data.content.len()
                    ));
                }
            }

            let mut guard = app.lock().await;
            guard.logs.add("Response received from API".to_string());
            if let Some(warning) = response_data.warning {
                guard.logs.add(format!("API Warning: {}", warning));
            }
            let message = ChatMessage::new(response_data.content, false);
            guard.chat_messages.push(message);
            if let Some(usage) = response_data.usage {
                guard.logs.add(format!(
                    "Tokens used - Input: {}, Output: {}, Total: {}",
                    usage.input_tokens,
                    usage.output_tokens,
                    usage.input_tokens + usage.output_tokens
                ));
            }
        }
        Err(e) => {
            let mut guard = app.lock().await;
            // <<< ADDED >>>
            guard
                .logs
                .add("Claude API call returned an error.".to_string());
            if let Some(req_err) = e.downcast_ref::<reqwest::Error>() {
                if let Some(status) = req_err.status() {
                    guard.logs.add(format!("HTTP Status: {}", status));
                }
            }
            guard.logs.add(format!("Full error: {}", e));

            if let Some(req_err) = e.downcast_ref::<reqwest::Error>() {
                if req_err.is_timeout() {
                    guard.logs.add("Error: API request timed out".to_string());
                } else if req_err.is_connect() {
                    guard
                        .logs
                        .add("Error: Could not connect to API".to_string());
                } else if let Some(status) = req_err.status() {
                    guard
                        .logs
                        .add(format!("API Error ({}): {}", status, req_err));
                } else {
                    guard.logs.add(format!("API Error: {}", req_err));
                }
            } else {
                guard.logs.add(format!("Error: {}", e));
            }
            guard.chat_messages.push(ChatMessage::new(
                "I encountered an error processing your request.".to_string(),
                false,
            ));
        }
    }

    {
        let mut guard = app.lock().await;
        guard.chat_thinking = false;
        guard.status_indicator.set_thinking(false);
        guard.status_indicator.set_status("");
        guard.logs.add("Request complete".to_string());
    }
}

#[derive(Debug)]
pub struct ClaudeResponse {
    pub content: String,
    pub warning: Option<String>,
    pub usage: Option<TokenUsage>,
}

#[derive(Debug)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

pub async fn get_claude_response(
    user_input: &str,
    history: &[Value],
) -> Result<ClaudeResponse, Box<dyn Error + Send + Sync>> {
    let api_key = var("ANTHROPIC_API_KEY")?;
    let mut messages = history.to_vec();
    messages.push(json!({ "role": "user", "content": user_input }));

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
    let content = response_data["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let warning = response_data["warning"].as_str().map(|s| s.to_string());
    let usage = if let (Some(input), Some(output)) = (
        response_data["usage"]["input_tokens"].as_u64(),
        response_data["usage"]["output_tokens"].as_u64(),
    ) {
        Some(TokenUsage {
            input_tokens: input as u32,
            output_tokens: output as u32,
        })
    } else {
        None
    };

    Ok(ClaudeResponse {
        content,
        warning,
        usage,
    })
}

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
    if let Some(error) = body["error"].as_object() {
        return Err(format!(
            "API Error: {} - {}",
            error["type"].as_str().unwrap_or("Unknown"),
            error["message"].as_str().unwrap_or("No message")
        )
        .into());
    }
    Ok(body["content"][0]["text"]
        .as_str()
        .unwrap_or("Sorry, I couldn't process that request.")
        .to_string())
}
