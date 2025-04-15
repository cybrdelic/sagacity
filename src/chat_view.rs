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
    
    // Updated layout to include context panel on the right
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(2, 3), Constraint::Ratio(1, 3)])
        .margin(1)
        .split(size);

    // Split the right chunk into context and logs
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(horizontal_chunks[1]);
    
    let context_area = right_chunks[0];
    let logs_area = right_chunks[1];

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
    
    // Draw the context panel
    draw_context(f, app, context_area);
    
    // Draw logs panel
    draw_logs(f, app, logs_area, size);
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

fn draw_logs(f: &mut Frame, app: &App, area: Rect, _size: Rect) {
    // Create a block for the logs area
    let logs_block = Block::default()
        .title(" Logs ")
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    
    let inner_area = logs_block.inner(area);
    f.render_widget(logs_block, area);

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
    let log_available_height = inner_area.height;
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
    f.render_widget(logs_para.scroll((logs_scroll, 0)), inner_area);
}

/// Draws the context management panel showing which files are in context
fn draw_context(f: &mut Frame, app: &mut App, area: Rect) {
    // Create a block for the context area
    let context_block = Block::default()
        .title(" Context Files ")
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    
    let inner_area = context_block.inner(area);
    f.render_widget(context_block, area);
    
    // Header for context panel showing # files in context
    let in_context_count = app.chatbot.context_entries.iter().filter(|e| e.in_context).count();
    let total_count = app.chatbot.context_entries.len();
    
    let header_text = format!("{}/{} files in context | ↑/↓ navigate, Enter toggle", in_context_count, total_count);
    let header = Paragraph::new(Line::from(vec![
        Span::styled(header_text, Style::default().fg(Color::Yellow))
    ]));
    
    let header_area = Rect {
        x: inner_area.x,
        y: inner_area.y,
        width: inner_area.width,
        height: 1,
    };
    
    f.render_widget(header, header_area);
    
    // List context entries
    let list_area = Rect {
        x: inner_area.x,
        y: inner_area.y + 1,
        width: inner_area.width,
        height: inner_area.height - 1,
    };
    
    let mut context_lines = Vec::new();
    for (i, entry) in app.chatbot.context_entries.iter().enumerate() {
        // Check if this entry is currently focused
        let is_focused = app.focused_context_index == Some(i);
        
        // Format the file path to be more readable
        let file_path = entry.file_path.clone();
        
        // Show icon based on whether the file is in context
        let icon = if entry.in_context {
            "▶ "
        } else {
            "  "
        };
        
        // Set style based on focus and context status
        let style = if is_focused {
            // Highlighted when focused
            Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD)
        } else if entry.in_context {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        
        // Format the score as a percentage
        let score_pct = (entry.relevance_score * 100.0).round() as u8;
        let score_text = format!(" {:3}%", score_pct);
        
        // Score color based on value
        let score_style = if is_focused {
            // Use same highlight style when focused
            style 
        } else if entry.relevance_score > 0.5 {
            Style::default().fg(Color::Green)
        } else if entry.relevance_score > 0.2 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        
        context_lines.push(Line::from(vec![
            Span::styled(icon, style),
            Span::styled(file_path, style),
            Span::styled(score_text, score_style),
        ]));
    }
    
    // Calculate scroll position based on focused item
    if let Some(focused_idx) = app.focused_context_index {
        let visible_items = list_area.height as usize;
        if focused_idx >= app.context_scroll as usize + visible_items {
            // Need to scroll down to show focused item
            app.context_scroll = (focused_idx - visible_items + 1) as u16;
        } else if focused_idx < app.context_scroll as usize {
            // Need to scroll up to show focused item
            app.context_scroll = focused_idx as u16;
        }
    }
    
    let context_para = Paragraph::new(context_lines)
        .scroll((app.context_scroll, 0))
        .wrap(Wrap { trim: true });
    
    f.render_widget(context_para, list_area);
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

    // Update context relevance scores based on the user query
    {
        let mut guard = app.lock().await;
        // Update relevance scores
        guard.chatbot.update_relevance_scores(&user_input);
        guard.logs.add(format!(
            "Updated context relevance scores for query: '{}'", 
            if user_input.len() > 30 { 
                format!("{}...", &user_input[0..30]) 
            } else { 
                user_input.clone() 
            }
        ));
    }

    // Get the context string from the selected files
    let context = {
        let guard = app.lock().await;
        guard.chatbot.get_context_string()
    };

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
