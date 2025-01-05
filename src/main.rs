use std::{
    collections::HashMap,
    env,
    error::Error,
    io::{self},
    time::{Duration, SystemTime},
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

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Clone, Debug)]
struct TreeNode {
    filename: String,
    progress: f32,
    status: String,
}

impl TreeNode {
    fn new(filename: String) -> Self {
        Self {
            filename,
            progress: 0.0,
            status: "pending".into(),
        }
    }
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
struct Chatbot {
    index: HashMap<String, (String, String)>,
    api_key: String,
}

impl Chatbot {
    fn new(api_key: String) -> Self {
        Self {
            index: HashMap::new(),
            api_key,
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
    chatbot: Chatbot,
    thinking_status: String,
    indexing_start_time: Option<SystemTime>,
    chat_scroll: u16,
    logs_scroll: u16,
}

impl App {
    fn new() -> Self {
        let api_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        let chatbot = Chatbot::new(api_key);

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
            chatbot,
            thinking_status: String::new(),
            indexing_start_time: None,
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
                .fg(Color::Magenta)
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
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(main_chunks[0]);

    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spin_char = spinner_frames[app.spinner_idx % spinner_frames.len()];

    let elapsed = app
        .indexing_start_time
        .map(|start| start.elapsed().unwrap_or_default())
        .unwrap_or_default();

    let top_line = format!(
        "Status: {} {}  ({} files)\nElapsed: {}s",
        spin_char,
        if app.indexing_done {
            "Complete!"
        } else {
            "Indexing..."
        },
        app.indexing_count,
        elapsed.as_secs()
    );

    let top_para = Paragraph::new(top_line)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .title(" Status ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .alignment(Alignment::Left);
    f.render_widget(top_para, left_split[0]);

    let mut lines = Vec::new();
    for (i, node) in app.tree.iter().enumerate() {
        let bar_len: usize = 20;
        let filled = (node.progress * bar_len as f32).round() as usize;
        let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(bar_len - filled));
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
                .title(" Files ")
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
    let final_bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));
    let bot_line = format!("Overall progress: {:.1}%  {}", overall * 100.0, final_bar);
    let bot_para = Paragraph::new(bot_line)
        .block(
            Block::default()
                .title(" Progress ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .alignment(Alignment::Left);
    f.render_widget(bot_para, left_split[2]);

    let logs_block = Block::default()
        .title(" Logs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner_logs_area = logs_block.inner(main_chunks[1]);
    f.render_widget(logs_block, main_chunks[1]);

    let mut log_lines = Vec::new();
    for entry in &app.logs.entries {
        log_lines.push(Line::from(vec![Span::raw(entry)]));
    }

    let logs_para = Paragraph::new(log_lines)
        .wrap(Wrap { trim: true })
        .scroll((app.logs_scroll, 0));

    f.render_widget(logs_para, inner_logs_area);
}

fn draw_chat(f: &mut Frame, app: &App) {
    let size = f.area();

    let horizontal_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(2, 3), Constraint::Ratio(1, 3)])
        .margin(1)
        .split(size);

    let chat_vertical_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length(3),
        ])
        .split(horizontal_chunks[0]);

    let messages_area = chat_vertical_chunks[0];

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

    let input_area = chat_vertical_chunks[2];
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

    let cursor_x = input_area.x + 2 + app.chat_input.len() as u16 - scroll_offset;
    f.set_cursor_position((cursor_x, input_area.y + 1));

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

fn draw_logs_panel(f: &mut Frame, logs: &LogPanel, area: Rect) {
    let logs_block = Block::default()
        .title(" Logs ")
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
        guard.logs.add("Starting codebase indexing...");
        guard.indexing_start_time = Some(SystemTime::now());
        guard.tree = vec![TreeNode::new("src/main.rs".into())];
    }

    let api_key = {
        let guard = app.lock().await;
        guard.chatbot.api_key.clone()
    };

    let main_rs = "src/main.rs";
    {
        let mut guard = app.lock().await;
        guard.logs.add("Indexing main.rs...");
    }

    if let Ok(content) = std::fs::read_to_string(main_rs) {
        if let Ok(summary) = summarize_file(&content, "rust", &api_key).await {
            let mut guard = app.lock().await;
            guard
                .chatbot
                .index
                .insert(main_rs.to_string(), (summary, "rust".to_string()));
            guard.logs.add("Indexed main.rs successfully");

            let node = guard.tree.get_mut(0).unwrap();
            node.progress = 1.0;
            node.status = "done".into();
            guard.indexing_count += 1;
        }
    }

    {
        let mut guard = app.lock().await;
        guard.indexing_done = true;
        guard.logs.add("Indexing complete!");
        guard.screen = AppScreen::Chat;
    }
}

async fn simulate_chat_response(app: Arc<Mutex<App>>, user_input: String) {
    {
        let mut guard = app.lock().await;
        guard.chat_thinking = true;
        guard.chat_input.clear();
        guard.logs.add("Processing query...");
        guard.thinking_status = "Thinking...".to_string();
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
        guard.thinking_status = String::new();
        guard.logs.add("Response complete");
    }
}

async fn get_claude_response(
    user_input: &str,
    history: &[Value],
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let api_key = env::var("ANTHROPIC_API_KEY")?;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
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
                Event::Key(key) => {
                    let mut guard = app.lock().await;
                    match guard.screen {
                        AppScreen::Splash => match (key.modifiers, key.code) {
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
                        AppScreen::Indexing => match (key.modifiers, key.code) {
                            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                                break 'outer;
                            }
                            (KeyModifiers::NONE, KeyCode::Esc) => {
                                guard.logs.add("Indexing cancelled by user");
                                guard.screen = AppScreen::Chat;
                            }
                            _ => {}
                        },
                        AppScreen::Chat => match (key.modifiers, key.code) {
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
                            (KeyModifiers::NONE, KeyCode::Up) => {
                                if guard.chat_scroll > 0 {
                                    guard.chat_scroll -= 1;
                                }
                            }
                            (KeyModifiers::NONE, KeyCode::Down) => {
                                guard.chat_scroll += 1;
                            }
                            (KeyModifiers::NONE, KeyCode::PageUp) => {
                                guard.chat_scroll = guard.chat_scroll.saturating_sub(10);
                            }
                            (KeyModifiers::NONE, KeyCode::PageDown) => {
                                guard.chat_scroll += 10;
                            }
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

async fn summarize_file(
    content: &str,
    language: &str,
    api_key: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let client = Client::new();
    let prompt = format!(
        "Please analyze this {} code and provide a brief summary of its purpose and functionality.\n\nCode:\n{}",
        language, content
    );

    let response = client
        .post(CLAUDE_API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": "claude-3-opus-20240229",
            "max_tokens": 1024,
            "messages": [{ "role": "user", "content": prompt }]
        }))
        .send()
        .await?;

    let body: Value = response.json().await?;
    Ok(body["content"][0]["text"]
        .as_str()
        .unwrap_or("Sorry, I couldn't process that request.")
        .to_string())
}
