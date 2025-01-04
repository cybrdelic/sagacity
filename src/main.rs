use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ignore;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use reqwest;
use serde::{Deserialize, Serialize};
use tokio;
use tokio::sync::Mutex;

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-3-sonnet-20240229";
const DEFAULT_MAX_TOKENS: i32 = 2048;

// -- states

#[derive(Debug)]
enum AppState {
    Indexing,
    MainMenu,
}

// -- logs

#[derive(Debug)]
enum LogLevel {
    Info,
    Error,
}

// -- message

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    timestamp: DateTime<Utc>,
}

// -- chatbot

#[derive(Debug)]
struct Chatbot {
    index: HashMap<String, (String, String)>,
    api_key: String,
    memory: Vec<Message>,
}

impl Chatbot {
    fn new(index: HashMap<String, (String, String)>, api_key: String) -> Self {
        Self {
            index,
            api_key,
            memory: vec![],
        }
    }

    async fn chat(&mut self, user_query: &str) -> Result<String, Box<dyn Error>> {
        let index_clone = self.index.clone();
        let api_key_clone = self.api_key.clone();
        let memory_clone = self.memory.clone();
        let relevant_files = search_index(&index_clone, user_query, &api_key_clone).await?;

        let relevant_file_info: Vec<(String, String)> = relevant_files
            .into_iter()
            .filter_map(|(file, _)| {
                self.index
                    .get(&file)
                    .map(|(_, lang)| (file.clone(), lang.clone()))
            })
            .collect();

        if relevant_file_info.is_empty() {
            return Err("no relevant files found in the index for that query.".into());
        }

        let context = prepare_context(&relevant_file_info, user_query)?;
        let response =
            generate_llm_response(&context, &api_key_clone, &memory_clone, user_query).await?;

        self.memory.push(Message {
            role: "user".to_string(),
            content: user_query.to_string(),
            timestamp: Utc::now(),
        });
        self.memory.push(Message {
            role: "assistant".to_string(),
            content: response.clone(),
            timestamp: Utc::now(),
        });

        Ok(response)
    }
}

// -- app data

struct App {
    state: AppState,
    messages: Vec<Message>,
    input: String,
    input_mode: bool,
    chatbot: Chatbot,
    indexing_total: usize,
    indexing_done: usize,
    spinner_idx: usize,
}

impl App {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let api_key = env::var("ANTHROPIC_API_KEY")
            .map_err(|_| "missing ANTHROPIC_API_KEY environment variable")?;

        let cb = Chatbot::new(HashMap::new(), api_key);
        let initial_message = Message {
            role: "assistant".to_string(),
            content: "hello! i'm claude. i can help you understand your codebase. what would you like to know?".to_string(),
            timestamp: Utc::now(),
        };

        // start in the indexing screen
        Ok(Self {
            state: AppState::Indexing,
            messages: vec![initial_message],
            input: String::new(),
            input_mode: false,
            chatbot: cb,
            indexing_total: 0,
            indexing_done: 0,
            spinner_idx: 0,
        })
    }

    async fn index_codebase(&mut self) -> Result<(), Box<dyn Error>> {
        self.add_log(LogLevel::Info, "starting codebase indexing...");

        let current_dir = std::env::current_dir()?;
        let walker = ignore::WalkBuilder::new(&current_dir)
            .hidden(true)
            .ignore(true)
            .git_ignore(true)
            .build()
            .filter(|entry_res| {
                let entry = match entry_res {
                    Ok(e) => e,
                    Err(_) => return false,
                };
                let p = entry.path();
                // skip target dir
                if p.components().any(|c| c.as_os_str() == "target") {
                    return false;
                }
                true
            });

        // gather files first
        let mut files = Vec::new();
        for e in walker {
            if let Ok(entry) = e {
                let metadata = match entry.metadata() {
                    Ok(md) => md,
                    Err(_) => continue,
                };
                if !metadata.is_file() {
                    continue;
                }
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if matches!(
                        ext,
                        "rs" | "toml" | "py" | "js" | "ts" | "tsx" | "html" | "css" | "go"
                    ) {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }

        self.indexing_total = files.len();

        self.indexing_done = 0;

        // now process each
        for path_buf in files {
            let path_str = path_buf.to_string_lossy().to_string();
            self.add_log(LogLevel::Info, format!("processing file: {}", &path_str));

            if let Ok(content) = std::fs::read_to_string(&path_buf) {
                let ext = path_buf
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("unknown");
                let language = match ext {
                    "rs" => "rust",
                    "toml" => "toml",
                    "py" => "python",
                    "js" => "javascript",
                    "ts" | "tsx" => "typescript",
                    "html" => "html",
                    "css" => "css",
                    "go" => "go",
                    _ => "unknown",
                };

                match summarize_file(&content, language, &self.chatbot.api_key).await {
                    Ok(summary) => {
                        self.chatbot
                            .index
                            .insert(path_str.clone(), (summary, language.to_string()));
                    }
                    Err(e) => {
                        self.add_log(
                            LogLevel::Error,
                            format!("error summarizing {}: {}", path_str, e),
                        );
                    }
                }
            }

            // increment so the draw_ui can show progress
            self.indexing_done += 1;
            // short sleep to simulate slow indexing
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        }

        self.add_log(LogLevel::Info, "indexing complete.");
        self.state = AppState::MainMenu;
        Ok(())
    }

    async fn handle_user_input(&mut self) -> Result<(), Box<dyn Error>> {
        let user_message = Message {
            role: "user".to_string(),
            content: self.input.clone(),
            timestamp: Utc::now(),
        };
        self.messages.push(user_message);

        match self.chatbot.chat(&self.input).await {
            Ok(response) => {
                let assistant_message = Message {
                    role: "assistant".to_string(),
                    content: response,
                    timestamp: Utc::now(),
                };
                self.messages.push(assistant_message);
            }
            Err(e) => {
                let error_message = Message {
                    role: "system".to_string(),
                    content: format!("error: {}", e),
                    timestamp: Utc::now(),
                };
                self.messages.push(error_message);
            }
        }
        self.input.clear();
        Ok(())
    }

    fn add_log(&mut self, lvl: LogLevel, msg: impl Into<String>) {
        let level_str = match lvl {
            LogLevel::Info => "[info]",
            LogLevel::Error => "[error]",
        };
        self.messages.push(Message {
            role: "log".to_string(),
            content: format!("{} {}", level_str, msg.into()),
            timestamp: Utc::now(),
        });
    }
}

// -- summarizing

async fn summarize_file(
    content: &str,
    language: &str,
    api_key: &str,
) -> Result<String, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let prompt = format!(
        "please analyze this {} code and provide:\n1. a brief summary of its purpose and functionality\n2. list of main classes, functions, or components\n\ncode:\n{}\n\nformat your response in a concise way, focusing on the key elements.",
        language, content
    );

    let response = client
        .post(CLAUDE_API_URL)
        .header("content-type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&serde_json::json!({
            "model": DEFAULT_MODEL,
            "max_tokens": DEFAULT_MAX_TOKENS,
            "messages": [ { "role": "user", "content": prompt } ]
        }))
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response.text().await?;
        return Err(format!("claude api request failed: {} - {}", status, error_body).into());
    }

    let body: serde_json::Value = response.json().await?;
    let summary = body["content"][0]["text"]
        .as_str()
        .ok_or("missing 'text' field in claude response")?
        .trim()
        .to_string();

    Ok(summary)
}

// -- search

async fn search_index(
    index: &HashMap<String, (String, String)>,
    query: &str,
    api_key: &str,
) -> Result<Vec<(String, f32)>, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let mut prompt = format!(
        "score relevance of each file (0 to 1) to query:\nquery: {}\n\n",
        query
    );

    for (file, (summary, lang)) in index {
        prompt.push_str(&format!("file: {} ({}) - {}\n", file, lang, summary));
    }

    let response = client
        .post(CLAUDE_API_URL)
        .header("content-type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&serde_json::json!({
            "model": DEFAULT_MODEL,
            "messages": [ {
                "role": "user",
                "content": format!(
                    "{}\nplease respond with only file paths and scores in format: path,score",
                    prompt
                )
            } ],
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response.text().await?;
        return Err(format!("claude api request failed: {} - {}", status, error_body).into());
    }

    let body: serde_json::Value = response.json().await?;
    let response_text = body["content"][0]["text"]
        .as_str()
        .ok_or("missing 'text' field")?
        .trim();

    let mut relevant_files = Vec::new();
    for line in response_text.lines() {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() == 2 {
            if let Ok(score) = parts[1].trim().parse::<f32>() {
                if score > 0.0 {
                    relevant_files.push((parts[0].trim().to_string(), score));
                }
            }
        }
    }
    relevant_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    relevant_files.truncate(5);
    Ok(relevant_files)
}

// -- context prep

fn prepare_context(
    relevant_files: &[(String, String)],
    user_query: &str,
) -> Result<String, Box<dyn Error>> {
    let mut context = format!("user query: {}\n\nrelevant files:\n\n", user_query);
    for (file_path, language) in relevant_files {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            context.push_str(&format!(
                "file: {} ({})\ncontent:\n{}\n\n",
                file_path, language, content
            ));
        }
    }
    Ok(context)
}

// -- llm response

async fn generate_llm_response(
    context: &str,
    api_key: &str,
    conversation_history: &[Message],
    user_query: &str,
) -> Result<String, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let mut messages = conversation_history
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content
            })
        })
        .collect::<Vec<_>>();
    messages.push(serde_json::json!({
        "role": "user",
        "content": format!(
            "based on the following context about a codebase and our previous conversation, please answer the user's query:\n\ncontext: {}\n\nuser query: {}",
            context,
            user_query
        )
    }));

    let response = client
        .post(CLAUDE_API_URL)
        .header("content-type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&serde_json::json!({
            "model": DEFAULT_MODEL,
            "max_tokens": DEFAULT_MAX_TOKENS,
            "messages": messages,
        }))
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response.text().await?;
        return Err(format!("claude api request failed: {} - {}", status, error_body).into());
    }

    let body: serde_json::Value = response.json().await?;
    let answer = body["content"][0]["text"]
        .as_str()
        .ok_or("missing 'text' field in claude response")?
        .trim()
        .to_string();

    Ok(answer)
}

// -- drawing

pub fn draw_ui(f: &mut Frame, app: &App) {
    match app.state {
        AppState::Indexing => {
            let area = f.size();

            // layout for ascii up top + logs below
            let top_bottom = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(10), // ascii + spinner
                    Constraint::Min(0),     // logs
                ])
                .split(area);

            let ascii_art = r#"
   __     ___   _ _______
   \ \   / / | | |__   __|
    \ \_/ /| | | |  | |
     \   / | | | |  | |
      | |  | |_| |  | |
      |_|   \___/   |_|
   "#;

            let fancy_block = Block::default()
                .title(" codebase indexing in progress ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta));
            f.render_widget(fancy_block, top_bottom[0]);

            // center ascii inside top_bottom[0]
            let (bx, by, bw, bh) = (
                top_bottom[0].x,
                top_bottom[0].y,
                top_bottom[0].width,
                top_bottom[0].height,
            );

            let lines = ascii_art.lines().count() as u16;
            let max_len = ascii_art.lines().map(|l| l.len()).max().unwrap_or(10) as u16;

            let x_center = bx + bw / 2;
            let y_center = by + bh / 2;
            let render_w = max_len.min(bw);
            let render_h = lines.min(bh);

            let ascii_par = Paragraph::new(Text::from(ascii_art))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });

            let offset_x = x_center.saturating_sub(render_w / 2);
            let offset_y = y_center.saturating_sub(render_h / 2);

            f.render_widget(ascii_par, Rect::new(offset_x, offset_y, render_w, render_h));

            // draw spinner & progress at the top row
            let spinner_frames = ["-", "\\", "|", "/"];
            let spin_char = spinner_frames[app.spinner_idx % spinner_frames.len()];

            // let's do a rudimentary progress readout
            let total = app.indexing_total;
            let done = app.indexing_done;
            let progress = if total > 0 {
                (done as f32 / total as f32) * 100.0
            } else {
                0.0
            };

            // we'll render that line just below the ascii block
            let spin_line = format!(
                "spinner: {} | indexed {}/{} (~{:.1}%)",
                spin_char, done, total, progress
            );
            let spin_par = Paragraph::new(spin_line).wrap(Wrap { trim: true });
            f.render_widget(spin_par, Rect::new(bx + 2, by, bw.saturating_sub(4), 1));

            // logs below
            let logs_block = Block::default()
                .title(" logs ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green));
            let logs_area = logs_block.inner(top_bottom[1]);
            f.render_widget(logs_block, top_bottom[1]);

            let mut y_log = logs_area.y;
            for m in app.messages.iter().filter(|m| m.role == "log") {
                let paragraph = Paragraph::new(m.content.clone()).wrap(Wrap { trim: true });
                let needed = (m.content.len() as u16 / logs_area.width).max(1) + 1;
                if y_log + needed <= logs_area.y + logs_area.height {
                    f.render_widget(
                        paragraph,
                        Rect::new(logs_area.x, y_log, logs_area.width, needed),
                    );
                    y_log += needed;
                } else {
                    break;
                }
            }
        }

        AppState::MainMenu => {
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints([
                    Constraint::Min(6),
                    Constraint::Length(6),
                    Constraint::Length(3),
                    Constraint::Length(1),
                ])
                .split(f.size());

            let chat_block = Block::default()
                .title(" chat ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan));
            let chat_area = chat_block.inner(main_chunks[0]);
            f.render_widget(chat_block, main_chunks[0]);

            let mut y_chat = chat_area.y;
            for m in app.messages.iter().filter(|m| m.role != "log") {
                let style_color = match m.role.as_str() {
                    "assistant" => Color::Cyan,
                    "user" => Color::Yellow,
                    "system" => Color::Magenta,
                    _ => Color::Red,
                };
                let prefix = format!("{}:", m.role);
                let line = Line::from(vec![
                    Span::styled(
                        prefix,
                        Style::default()
                            .fg(style_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(m.content.clone(), Style::default().fg(Color::White)),
                ]);
                let paragraph = Paragraph::new(line).wrap(Wrap { trim: true });
                let needed = (m.content.len() as u16 / chat_area.width).max(1) + 1;
                if y_chat + needed <= chat_area.y + chat_area.height {
                    f.render_widget(
                        paragraph,
                        Rect::new(chat_area.x, y_chat, chat_area.width, needed),
                    );
                    y_chat += needed;
                } else {
                    break;
                }
            }

            let logs_block = Block::default()
                .title(" logs ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green));
            let logs_area = logs_block.inner(main_chunks[1]);
            f.render_widget(logs_block, main_chunks[1]);

            let mut y_log = logs_area.y;
            for m in app.messages.iter().filter(|m| m.role == "log") {
                let paragraph = Paragraph::new(m.content.clone()).wrap(Wrap { trim: true });
                let needed = (m.content.len() as u16 / logs_area.width).max(1) + 1;
                if y_log + needed <= logs_area.y + logs_area.height {
                    f.render_widget(
                        paragraph,
                        Rect::new(logs_area.x, y_log, logs_area.width, needed),
                    );
                    y_log += needed;
                } else {
                    break;
                }
            }

            let input_block = Block::default()
                .title(if app.input_mode {
                    " input (active) "
                } else {
                    " input (press tab) "
                })
                .borders(Borders::ALL)
                .border_style(if app.input_mode {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                });
            let input_area = input_block.inner(main_chunks[2]);
            f.render_widget(input_block, main_chunks[2]);

            let input_text = Paragraph::new(app.input.as_str())
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: true });
            f.render_widget(input_text, input_area);

            let status_msg = format!(
                " {} | tab=toggle | enter=send | ctrl+c=exit ",
                if app.input_mode {
                    "input mode active"
                } else {
                    "press tab to start typing"
                }
            );
            let status_par = Paragraph::new(status_msg)
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: true });
            f.render_widget(status_par, main_chunks[3]);

            if app.input_mode {
                let x_cursor = input_area.x + app.input.len() as u16 + 1;
                let y_cursor = input_area.y;
                f.set_cursor(x_cursor, y_cursor);
            }
        }
    }
}

// -- main

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = Arc::new(Mutex::new(App::new().await?));

    // spawn the indexing in the background
    {
        let app_clone = app.clone();
        tokio::spawn(async move {
            let mut guard = app_clone.lock().await;
            if let Err(e) = guard.index_codebase().await {
                guard.add_log(LogLevel::Error, format!("failed to index codebase: {e}"));
            }
        });
    }

    loop {
        {
            let mut app_guard = app.lock().await;
            // increment spinner index if still indexing
            if matches!(app_guard.state, AppState::Indexing) {
                app_guard.spinner_idx = app_guard.spinner_idx.wrapping_add(1);
            }
            terminal.draw(|f| {
                draw_ui(f, &app_guard);
            })?;
        }

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match (key.modifiers, key.code) {
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => break,
                    (_, KeyCode::Tab) => {
                        let mut a = app.lock().await;
                        a.input_mode = !a.input_mode;
                    }
                    (_, KeyCode::Enter) => {
                        let mut a = app.lock().await;
                        if a.input_mode && !a.input.trim().is_empty() {
                            a.handle_user_input().await?;
                        }
                    }
                    (_, KeyCode::Char(c)) => {
                        let mut a = app.lock().await;
                        if a.input_mode {
                            a.input.push(c);
                        }
                    }
                    (_, KeyCode::Backspace) => {
                        let mut a = app.lock().await;
                        if a.input_mode {
                            a.input.pop();
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
