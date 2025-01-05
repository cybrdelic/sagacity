use std::{
    error::Error,
    io::{self},
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use textwrap::wrap;
use tokio::sync::Mutex;
use tokio::time::sleep;

/// Add the following dependencies to your `Cargo.toml`:
/// ```toml
/// [dependencies]
/// crossterm = "0.26"
/// ratatui = "0.29"
/// reqwest = { version = "0.11", features = ["json", "tokio-runtime"] }
/// serde_json = "1.0"
/// tokio = { version = "1.28", features = ["full"] }
/// dotenv = "0.15"
/// textwrap = "0.15"
/// ```

#[derive(Clone, Debug)]
struct TreeNode {
    filename: String,
    progress: f32,
    status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppScreen {
    Splash,
    Indexing,
    Chat,
}

#[derive(Debug)]
struct LogPanel {
    entries: Vec<String>,
    visible: bool,
}

impl LogPanel {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            visible: true,
        }
    }
    fn add(&mut self, msg: impl Into<String>) {
        self.entries.push(msg.into());
        if self.entries.len() > 200 {
            self.entries.remove(0);
        }
    }
}

#[derive(Debug)]
struct App {
    screen: AppScreen,
    splash_selected_idx: usize,
    splash_menu_items: Vec<&'static str>,
    tree: Vec<TreeNode>,
    indexing_done: bool,
    indexing_count: usize,
    chat_input: String,
    chat_messages: Vec<(String, bool)>,
    logs: LogPanel,
    spinner_idx: usize,
    chat_thinking: bool,
    claude_client: Client,
    conversation_history: Vec<Value>,
    thinking_status: String,

    // New fields for scroll positions
    chat_scroll: u16,
    logs_scroll: u16,
}

impl App {
    fn new() -> Self {
        Self {
            screen: AppScreen::Splash,
            splash_selected_idx: 0,
            splash_menu_items: vec!["start chat", "quit"],
            tree: vec![],
            indexing_done: false,
            indexing_count: 0,
            chat_input: String::new(),
            chat_messages: vec![],
            logs: LogPanel::new(),
            spinner_idx: 0,
            chat_thinking: false,
            claude_client: Client::new(),
            conversation_history: Vec::new(),
            thinking_status: String::new(),

            // Initialize scroll positions
            chat_scroll: 0,
            logs_scroll: 0,
        }
    }
}

fn draw_ui(f: &mut Frame, app: &App) {
    match app.screen {
        AppScreen::Splash => draw_splash(f, app),
        AppScreen::Indexing => draw_indexing(f, app),
        AppScreen::Chat => draw_chat(f, app),
    }
}

fn draw_splash(f: &mut Frame, app: &App) {
    let size = f.area();

    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(size);

    let ascii_art = r#"
▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
██ ▄▀▄ █ ██ █ ▄▄▀█▀ ██ ▄▄▀█ ▄▀████ ▄▄▀██▄█
██ █ █ █ ▀▀ █ ▀▀▄██ ██ ▀▀ █ █ █▀▀█ ▀▀ ██ ▄
██ ███ █▀▀▀▄█▄█▄▄█▀ ▀█▄██▄█▄▄██▄▄█▄██▄█▄▄▄
▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
An Intelligent Software Development Copilot
    "#;

    let ascii_par = Paragraph::new(ascii_art)
        .alignment(Alignment::Center)
        .block(Block::default())
        .wrap(Wrap { trim: true });

    let ascii_vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(hsplit[0]);

    f.render_widget(ascii_par, ascii_vert[1]);

    let mut menu_lines = Vec::new();
    for (i, item) in app.splash_menu_items.iter().enumerate() {
        let selected = i == app.splash_selected_idx;
        let style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        menu_lines.push(Line::from(Span::styled(
            format!("{} {}", if selected { "▶" } else { " " }, item),
            style,
        )));
    }
    let menu_par = Paragraph::new(menu_lines)
        .alignment(Alignment::Center)
        .block(Block::default());

    let menu_line_count = app.splash_menu_items.len() as u16;

    let menu_vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(menu_line_count),
            Constraint::Percentage(50),
        ])
        .split(hsplit[1]);

    f.render_widget(menu_par, menu_vert[1]);
}

fn draw_indexing(f: &mut Frame, app: &App) {
    let size = f.area();
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(size);

    let left_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(main_chunks[0]);

    let spinner_frames = ["-", "\\", "|", "/"];
    let spin_char = spinner_frames[app.spinner_idx % spinner_frames.len()];
    let indexing_status = if app.indexing_done {
        "complete!"
    } else {
        "indexing..."
    };
    let top_line = format!(
        "status: {} {}  (files processed: {})",
        spin_char, indexing_status, app.indexing_count
    );
    let top_para = Paragraph::new(top_line)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .title(" indexing status ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .alignment(Alignment::Left);
    f.render_widget(top_para, left_split[0]);

    let mut lines = Vec::new();
    for (i, node) in app.tree.iter().enumerate() {
        let bar_len: usize = 20;
        let filled = (node.progress * bar_len as f32).round() as usize;
        let filled_str = "#".repeat(filled);
        let empty_str = " ".repeat(bar_len.saturating_sub(filled));
        let bar = format!("[{}{}]", filled_str, empty_str);
        let line_str = format!(
            "{}. {} ({}%)  {} [{}]",
            i + 1,
            node.filename,
            (node.progress * 100.0) as u8,
            bar,
            node.status,
        );
        lines.push(Line::from(Span::raw(line_str)));
    }
    let tree_para = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" indexing files ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(tree_para, left_split[1]);

    let total_files = app.tree.len() as f32;
    let mut total_progress = 0.0;
    for node in &app.tree {
        total_progress += node.progress;
    }
    let overall = if total_files > 0.0 {
        total_progress / total_files
    } else {
        0.0
    };
    let bar_len: usize = 30;
    let filled = (overall * bar_len as f32).round() as usize;
    let empty = bar_len.saturating_sub(filled);
    let final_bar = format!("[{}{}]", "#".repeat(filled), " ".repeat(empty));
    let bot_line = format!("overall progress: {:.1}%  {}", overall * 100.0, final_bar);
    let bot_para = Paragraph::new(bot_line)
        .block(
            Block::default()
                .title(" overall progress ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .alignment(Alignment::Left);
    f.render_widget(bot_para, left_split[2]);
}

fn draw_chat(f: &mut Frame, app: &App) {
    let size = f.area();

    // Split screen horizontally into chat and logs sections
    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(2, 3), // Main chat area (2/3 of screen)
            Constraint::Ratio(1, 3), // Logs area (1/3 of screen)
        ])
        .margin(1) // Add margin around entire UI
        .split(size);

    // Split the chat section vertically for messages, status, and input
    let chat_vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Messages area (takes remaining space)
            Constraint::Length(2), // Status bar (2 lines: 1 for separator, 1 for status)
            Constraint::Length(3), // Input area (1 line for text, 2 for separators)
        ])
        .split(horizontal_chunks[0]);

    let messages_area = chat_vertical_chunks[0];

    // Render chat messages
    let mut lines = Vec::new();
    for (msg, from_user) in &app.chat_messages {
        // Add spacing between messages
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }

        let style = Style::default().fg(if *from_user {
            Color::Yellow
        } else {
            Color::Green
        });

        // Process the message content
        let indent = if *from_user { "  " } else { "" };

        // Add message start
        lines.push(Line::from(vec![
            Span::styled(indent, style),
            Span::styled("│ ", style),
        ]));

        // Split and process message by lines
        let mut in_code_block = false;
        let mut code_buffer = String::new();
        let mut text_buffer = String::new();

        for line in msg.lines() {
            if line.trim().starts_with("```") {
                // Process any accumulated text before switching modes
                if !text_buffer.is_empty() {
                    // Wrap and add accumulated text
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

                // Process any accumulated code before switching modes
                if !code_buffer.is_empty() {
                    // Add accumulated code
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

        // Process any remaining text
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

        // Process any remaining code
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

        // Add message end
        lines.push(Line::from(vec![
            Span::styled(indent, style),
            Span::styled("╰─", style),
        ]));
    }

    // Calculate total lines and available height
    let total_lines = lines.len() as u16;
    let available_height = messages_area.height;

    // Ensure scroll offset is within bounds
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

    // Render the messages with scrolling
    let msgs_para = Paragraph::new(lines)
        .style(Style::default())
        .block(Block::default())
        .wrap(Wrap { trim: true });

    f.render_widget(msgs_para.scroll((chat_scroll, 0)), messages_area);

    // Draw status bar with spinner
    let spinner_frames = ["◐", "◓", "◑", "◒"];
    let thinking_indicator = if app.chat_thinking {
        spinner_frames[app.spinner_idx % spinner_frames.len()]
    } else {
        " "
    };

    let status = Line::from(vec![
        Span::styled(thinking_indicator, Style::default().fg(Color::Gray)),
        Span::raw(" "),
        Span::styled(
            if app.chat_thinking {
                &app.thinking_status
            } else {
                ""
            },
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let status_area = chat_vertical_chunks[1];
    f.render_widget(
        Paragraph::new(status).alignment(Alignment::Left),
        Rect {
            x: status_area.x,
            y: status_area.y + 1,
            width: status_area.width,
            height: 1,
        },
    );

    // Draw input area
    let input_area = chat_vertical_chunks[2];

    // Top separator
    let separator = "─".repeat(input_area.width as usize);
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

    // Input text
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

    // Draw logs area
    let log_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Log messages
            Constraint::Length(8), // Align with input area
        ])
        .split(horizontal_chunks[1]);

    // Vertical separator
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

    // Logs
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

    // Calculate total lines and available height for logs
    let total_log_lines = log_lines.len() as u16;
    let log_available_height = log_chunks[0].height;

    // Ensure logs_scroll is within bounds
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

    // Render logs with scrolling
    let logs_para = Paragraph::new(log_lines)
        .style(Style::default().fg(Color::DarkGray))
        .wrap(Wrap { trim: true });

    f.render_widget(logs_para.scroll((logs_scroll, 0)), log_chunks[0]);
}

fn draw_logs_panel(f: &mut Frame, logs: &LogPanel, area: Rect) {
    let logs_block = Block::default()
        .title(" logs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    let inner = logs_block.inner(area);
    f.render_widget(logs_block, area);

    let mut lines = Vec::new();
    for l in &logs.entries {
        lines.push(Line::from(vec![Span::raw(l)]));
    }
    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);
    f.render_widget(para, inner);
}

async fn indexing_task(app: Arc<Mutex<App>>) {
    {
        let mut guard = app.lock().await;
        guard.logs.add("starting codebase indexing...");
        guard.tree = vec![
            TreeNode {
                filename: "src/main.rs".into(),
                progress: 0.0,
                status: "pending".into(),
            },
            TreeNode {
                filename: "src/lib.rs".into(),
                progress: 0.0,
                status: "pending".into(),
            },
            TreeNode {
                filename: "Cargo.toml".into(),
                progress: 0.0,
                status: "pending".into(),
            },
        ];
    }

    for idx in 0..3 {
        {
            let mut guard = app.lock().await;
            let node = guard.tree.get_mut(idx).unwrap();
            node.status = "indexing".into();
            let fname = node.filename.clone();
            guard.logs.add(format!("indexing {}", fname));
        }
        for _ in 0..10 {
            {
                let mut guard = app.lock().await;
                let node = guard.tree.get_mut(idx).unwrap();
                node.progress += 0.1;
            }
            sleep(Duration::from_millis(150)).await;
        }
        {
            let mut guard = app.lock().await;
            let node = guard.tree.get_mut(idx).unwrap();
            node.status = "done".into();
            guard.indexing_count += 1;
        }
    }

    {
        let mut guard = app.lock().await;
        guard.indexing_done = true;
        guard.logs.add("indexing complete!");
        guard.screen = AppScreen::Chat;
    }
}

async fn get_claude_response(
    user_input: &str,
    history: &[Value],
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")?;
    let url = "https://api.anthropic.com/v1/messages";

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

    let client = Client::new();
    let response = client
        .post(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&payload)
        .send()
        .await?;

    let response_data: Value = response.json().await?;

    let content = response_data["content"][0]["text"]
        .as_str()
        .unwrap_or("Sorry, I couldn't process that request.")
        .to_string();

    Ok(content)
}

async fn simulate_chat_response(app: Arc<Mutex<App>>, user_input: String) {
    // Set initial state
    {
        let mut guard = app.lock().await;
        guard.chat_thinking = true;
        guard.chat_input.clear();
        guard.logs.add("Sending to Claude 3.5...");
        guard.thinking_status = "Claude 3.5 thinking...".to_string();
    }

    // Get conversation history without holding the lock
    let history = {
        let guard = app.lock().await;
        guard.conversation_history.clone()
    };

    // Make API call without holding the mutex lock
    let response = match get_claude_response(&user_input, &history).await {
        Ok(response) => {
            let mut guard = app.lock().await;
            guard.logs.add("Response received from Claude");
            response
        }
        Err(e) => {
            let mut guard = app.lock().await;
            guard.logs.add(format!("Error from Claude: {}", e));
            "I apologize, but I encountered an error processing your request.".to_string()
        }
    };

    // Update final state and conversation history
    {
        let mut guard = app.lock().await;
        // Update conversation history
        guard.conversation_history.push(json!({
            "role": "user",
            "content": user_input
        }));
        guard.conversation_history.push(json!({
            "role": "assistant",
            "content": response.clone()
        }));
        guard.chat_messages.push((response, false));
        guard.chat_thinking = false;
        guard.thinking_status = String::new(); // Clear the thinking status
        guard.logs.add("Claude response complete");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize environment
    dotenv::dotenv().ok();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = Arc::new(Mutex::new(App::new()));

    'outer: loop {
        {
            let mut guard = app.lock().await;
            guard.spinner_idx = guard.spinner_idx.wrapping_add(1);
            terminal.draw(|f| {
                draw_ui(f, &guard);
            })?;
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(ke) => {
                    let mut guard = app.lock().await;
                    match guard.screen {
                        AppScreen::Splash => match (ke.modifiers, ke.code) {
                            (KeyModifiers::NONE, KeyCode::Down) => {
                                guard.splash_selected_idx =
                                    (guard.splash_selected_idx + 1) % guard.splash_menu_items.len();
                            }
                            (KeyModifiers::NONE, KeyCode::Up) => {
                                guard.splash_selected_idx = if guard.splash_selected_idx == 0 {
                                    guard.splash_menu_items.len() - 1
                                } else {
                                    guard.splash_selected_idx - 1
                                };
                            }
                            (KeyModifiers::NONE, KeyCode::Enter) => {
                                let selected = guard.splash_menu_items[guard.splash_selected_idx];
                                if selected == "quit" {
                                    break 'outer;
                                } else {
                                    guard.screen = AppScreen::Indexing;
                                    let clone = app.clone();
                                    drop(guard);
                                    tokio::spawn(async move {
                                        indexing_task(clone).await;
                                    });
                                }
                            }
                            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                                break 'outer;
                            }
                            _ => {}
                        },
                        AppScreen::Indexing => match (ke.modifiers, ke.code) {
                            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                                break 'outer;
                            }
                            _ => {}
                        },
                        AppScreen::Chat => match (ke.modifiers, ke.code) {
                            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                                break 'outer;
                            }
                            (KeyModifiers::NONE, KeyCode::Enter) => {
                                let input_text = guard.chat_input.clone();
                                if !input_text.trim().is_empty() {
                                    guard.chat_messages.push((input_text.clone(), true));
                                    let clone = app.clone();
                                    drop(guard);
                                    tokio::spawn(async move {
                                        simulate_chat_response(clone, input_text).await;
                                    });
                                }
                            }
                            (KeyModifiers::NONE, KeyCode::Backspace) => {
                                guard.chat_input.pop();
                            }
                            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                                guard.chat_input.push(c);
                            }
                            // Scroll Up in Chat Messages
                            (KeyModifiers::NONE, KeyCode::Up) => {
                                if guard.chat_scroll > 0 {
                                    guard.chat_scroll -= 1;
                                }
                            }
                            // Scroll Down in Chat Messages
                            (KeyModifiers::NONE, KeyCode::Down) => {
                                guard.chat_scroll += 1;
                            }
                            // Scroll Page Up in Chat Messages
                            (KeyModifiers::NONE, KeyCode::PageUp) => {
                                guard.chat_scroll = guard.chat_scroll.saturating_sub(10);
                            }
                            // Scroll Page Down in Chat Messages
                            (KeyModifiers::NONE, KeyCode::PageDown) => {
                                guard.chat_scroll += 10;
                            }
                            // Optionally, you can add separate keys for scrolling logs
                            _ => {}
                        },
                    }
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
