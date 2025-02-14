use std::{
    env,
    error::Error,
    io,
    sync::Arc,
    time::{Duration, SystemTime},
};

mod build;
mod chat_message;
mod chat_view;
mod code_snippet;
mod db;
mod db_details_view;
mod indexing_view;
mod log_view;
mod models;
mod splash_screen;
mod status_indicator;

use chat_message::ChatMessage;
use copypasta::{ClipboardContext, ClipboardProvider};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dotenv::var;
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use tokio::sync::Mutex;

// import public constants from chat_view
use crate::chat_view::{ANTHROPIC_VERSION, CLAUDE_API_URL};

use crate::{
    chat_view::{draw_chat, simulate_chat_response},
    db::Db,
    indexing_view::{draw_indexing, indexing_task},
    models::{Chatbot, TreeNode},
    splash_screen::{SplashScreen, SplashScreenAction},
    status_indicator::StatusIndicator,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Splash,
    Indexing,
    Chat,
    DBDetails,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Normal,
    Command,
}

#[derive(Debug)]
pub struct App {
    screen: AppScreen,
    splash_screen: SplashScreen,
    tree: Vec<TreeNode>,
    indexing_done: bool,
    indexing_count: usize,
    chat_input: String,
    pub chat_messages: Vec<ChatMessage>,
    logs: log_view::LogView,
    spinner_idx: usize,
    chat_thinking: bool,
    chatbot: Chatbot,
    status_indicator: StatusIndicator,
    indexing_start_time: Option<SystemTime>,
    chat_scroll: u16,
    logs_scroll: u16,
    db_markdown_scroll: u16,
    pub focused_message_index: Option<usize>,
    input_mode: InputMode,
    command_buffer: String,
    pub db: Option<Db>,
    pub db_path: String,
}

impl App {
    fn new() -> Self {
        let api_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        let chatbot = Chatbot::new(api_key);
        Self {
            screen: AppScreen::Splash,
            splash_screen: SplashScreen::new(),
            tree: vec![],
            indexing_done: false,
            indexing_count: 0,
            chat_input: String::new(),
            chat_messages: vec![],
            logs: log_view::LogView::new(),
            spinner_idx: 0,
            chat_thinking: false,
            chatbot,
            status_indicator: StatusIndicator::new(),
            indexing_start_time: None,
            chat_scroll: 0,
            logs_scroll: 0,
            db_markdown_scroll: 0,
            focused_message_index: None,
            input_mode: InputMode::Normal,
            command_buffer: String::new(),
            db: None,
            db_path: "myriad_db.sqlite".to_string(),
        }
    }

    pub fn get_focused_message(&mut self) -> Option<&mut ChatMessage> {
        self.logs.add("getting focused message".to_string());
        if let Some(index) = self.focused_message_index {
            self.logs
                .add(format!("attempting to get message at index {}", index));
            self.chat_messages.get_mut(index)
        } else {
            self.logs.add("no focused message".to_string());
            None
        }
    }

    pub fn log_state(&mut self) {
        self.logs.add(format!(
            "state: msg_idx={:?}, msgs={}, scroll={}",
            self.focused_message_index,
            self.chat_messages.len(),
            self.chat_scroll
        ));
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    dotenv::dotenv().ok();
    setup_terminal()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let app = Arc::new(Mutex::new(App::new()));

    {
        let mut guard = app.lock().await;
        let db_instance = Db::init(&guard.db_path).await?;
        guard.db = Some(db_instance);
        guard.logs.add("db initialized successfully".to_string());
    }

    let res = run_app(&mut terminal, app.clone()).await;
    restore_terminal(&mut terminal)?;

    if let Err(err) = res {
        eprintln!("application error: {}", err);
    }

    Ok(())
}

fn setup_terminal() -> Result<(), Box<dyn Error + Send + Sync>> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    Ok(())
}

fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn draw_ui(f: &mut Frame, app: &mut App) {
    match app.screen {
        AppScreen::Splash => app.splash_screen.draw(f, f.area()),
        AppScreen::Indexing => draw_indexing(f, app),
        AppScreen::Chat => draw_chat(f, app),
        AppScreen::DBDetails => {
            // use block_in_place to avoid starting a nested runtime
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(db_details_view::draw_db_details(f, app))
            });
        }
    }
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: Arc<Mutex<App>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    loop {
        {
            let mut guard = app.lock().await;
            guard.spinner_idx = guard.spinner_idx.wrapping_add(1);
            terminal.draw(|f| draw_ui(f, &mut guard))?;
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    let should_exit = {
                        let mut guard = app.lock().await;
                        handle_key_event(&mut *guard, key, app.clone()).await?
                    };
                    if should_exit {
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

async fn handle_key_event(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    app_arc: Arc<Mutex<App>>,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    match app.screen {
        AppScreen::Splash => handle_splash_input(app, key, app_arc).await,
        AppScreen::Indexing => handle_indexing_input(app, key),
        AppScreen::Chat => handle_chat_input(app, key, app_arc).await,
        AppScreen::DBDetails => handle_db_details_input(app, key).await,
    }
}

async fn handle_splash_input(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    app_arc: Arc<Mutex<App>>,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    if let Some(action) = app.splash_screen.handle_input(key) {
        match action {
            SplashScreenAction::Quit => return Ok(true),
            SplashScreenAction::StartChat => {
                app.screen = AppScreen::Indexing;
                let clone = app_arc.clone();
                tokio::spawn(async move {
                    indexing_task(clone).await;
                });
            }
            SplashScreenAction::DbDetails => {
                app.screen = AppScreen::DBDetails;
            }
        }
    }
    Ok(false)
}

fn handle_indexing_input(
    app: &mut App,
    key: crossterm::event::KeyEvent,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Ok(true),
        (KeyModifiers::NONE, KeyCode::Esc) => {
            app.logs.add("indexing cancelled by user".to_string());
            app.screen = AppScreen::Chat;
        }
        _ => {}
    }
    Ok(false)
}

async fn handle_db_details_input(
    app: &mut App,
    key: crossterm::event::KeyEvent,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    if key.code == KeyCode::Esc {
        app.logs
            .add("exiting db details screen, returning to chat".to_string());
        app.screen = AppScreen::Chat;
    }
    Ok(false)
}

async fn handle_chat_input(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    app_arc: Arc<Mutex<App>>,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    match app.input_mode {
        InputMode::Normal => match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Up) => {
                app.logs.add("⬆️ up pressed".to_string());
                app.log_state();

                if let Some(current_message) = app.focused_message_index {
                    if let Some(msg) = app.get_focused_message() {
                        msg.focus_previous();
                        if msg.focused_chunk.is_none() && current_message > 0 {
                            app.focused_message_index = Some(current_message - 1);
                            if let Some(prev_msg) = app.get_focused_message() {
                                prev_msg.focused_chunk = Some(prev_msg.chunks.len() - 1);
                            }
                        }
                    }
                } else if app.chat_scroll > 0 {
                    app.chat_scroll -= 1;
                } else if !app.chat_messages.is_empty() {
                    app.focused_message_index = Some(app.chat_messages.len() - 1);
                    if let Some(msg) = app.get_focused_message() {
                        msg.focused_chunk = Some(msg.chunks.len() - 1);
                    }
                }
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Down) => {
                app.logs.add("⬇️ down pressed".to_string());
                app.log_state();

                if let Some(current_message) = app.focused_message_index {
                    if let Some(msg) = app.get_focused_message() {
                        msg.focus_next();
                        if msg.focused_chunk.is_none() {
                            if current_message + 1 < app.chat_messages.len() {
                                app.focused_message_index = Some(current_message + 1);
                                if let Some(next_msg) = app.get_focused_message() {
                                    next_msg.focused_chunk = Some(0);
                                }
                            } else {
                                app.focused_message_index = None;
                            }
                        }
                    }
                } else {
                    app.chat_scroll += 1;
                }
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                app.logs.add("⏎ enter pressed".to_string());
                app.log_state();

                if let Some(msg) = app.get_focused_message() {
                    if let Some(content) = msg.get_focused_content() {
                        match copy_to_clipboard(&content) {
                            Ok(_) => {
                                app.logs.add("✓ content copied to clipboard!".to_string());
                                app.status_indicator.set_status("copied to clipboard!");
                            }
                            Err(e) => {
                                app.logs.add(format!("⚠ failed to copy: {}", e));
                                app.status_indicator.set_status("copy failed - see logs");
                            }
                        }
                    }
                } else {
                    let input_text = app.chat_input.trim().to_string();
                    if !input_text.is_empty() {
                        app.logs.add("sending chat message".to_string());
                        app.chat_messages
                            .push(ChatMessage::new(input_text.clone(), true));
                        let clone = app_arc.clone();
                        tokio::spawn(async move {
                            simulate_chat_response(clone, input_text).await;
                        });
                        app.chat_input.clear();
                    }
                }
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Esc) => {
                app.logs.add("esc pressed - clearing focus".to_string());
                if let Some(msg) = app.get_focused_message() {
                    msg.focused_chunk = None;
                }
                app.focused_message_index = None;
                app.log_state();
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Char(':')) => {
                app.logs.add("entering command mode".to_string());
                app.input_mode = InputMode::Command;
                app.command_buffer.clear();
                app.command_buffer.push(':');
                Ok(false)
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                app.logs.add("ctrl+c pressed - exiting".to_string());
                Ok(true)
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                app.chat_input.pop();
                app.logs
                    .add("backspace - removing last character".to_string());
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                app.chat_input.push(c);
                app.logs.add("adding character to input".to_string());
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::PageUp) => {
                app.chat_scroll = app.chat_scroll.saturating_sub(10);
                app.logs.add("pageup - scrolling up by 10".to_string());
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::PageDown) => {
                app.chat_scroll += 10;
                app.logs.add("pagedown - scrolling down by 10".to_string());
                Ok(false)
            }
            _ => Ok(false),
        },
        InputMode::Command => match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                app.logs
                    .add("esc pressed in command mode - returning to normal mode".to_string());
                app.input_mode = InputMode::Normal;
                app.command_buffer.clear();
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                app.logs.add("executing command".to_string());
                handle_command(app)?;
                app.input_mode = InputMode::Normal;
                app.command_buffer.clear();
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                app.command_buffer.pop();
                if app.command_buffer.is_empty() {
                    app.logs
                        .add("command buffer empty - returning to normal mode".to_string());
                    app.input_mode = InputMode::Normal;
                } else {
                    app.logs.add("backspace in command buffer".to_string());
                }
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                app.command_buffer.push(c);
                app.logs
                    .add("adding character to command buffer".to_string());
                Ok(false)
            }
            _ => Ok(false),
        },
    }
}

fn handle_command(app: &mut App) -> Result<(), Box<dyn Error + Send + Sync>> {
    let parts: Vec<&str> = app
        .command_buffer
        .trim_start_matches(':')
        .split_whitespace()
        .collect();
    if parts.is_empty() {
        return Ok(());
    }

    match parts[0] {
        "copy" | "c" => {
            if parts.len() >= 2 {
                if let Ok(block_num) = parts[1].parse::<usize>() {
                    app.logs.add(format!("command: copy block {}", block_num));
                    if let Some(msg) = app.get_focused_message() {
                        if let Some(content) = msg.handle_esc_number(block_num) {
                            match copy_to_clipboard(&content) {
                                Ok(_) => {
                                    app.logs.add(format!("copied code block {}", block_num));
                                    app.status_indicator.set_status("copied to clipboard!");
                                }
                                Err(e) => {
                                    app.logs
                                        .add(format!("failed to copy block {}: {}", block_num, e));
                                    app.status_indicator.set_status("copy failed - see logs");
                                }
                            }
                        } else {
                            app.logs.add(format!("no code block {}", block_num));
                        }
                    }
                }
            }
        }
        "focus" | "f" => {
            if parts.len() >= 2 {
                if let Ok(msg_num) = parts[1].parse::<usize>() {
                    app.logs.add(format!("command: focus message {}", msg_num));
                    if msg_num > 0 && msg_num <= app.chat_messages.len() {
                        app.focused_message_index = Some(msg_num - 1);
                        app.logs.add(format!("focused message {}", msg_num));
                    }
                }
            }
        }
        "help" | "h" => {
            app.logs
                .add("commands: :copy <n> | :focus <n> | :help".to_string());
        }
        _ => {
            app.logs.add("unknown command. try :help".to_string());
        }
    }
    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    let mut ctx =
        ClipboardContext::new().map_err(|e| format!("failed to access clipboard: {}", e))?;
    ctx.set_contents(text.to_owned())
        .map_err(|e| format!("failed to set clipboard contents: {}", e))?;
    Ok(())
}

use serde_json::{json, Value};

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
        "please analyze this {} code and provide a brief summary of its purpose and functionality.\n\ncode:\n{}",
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
            "api error: {} - {}",
            error["type"].as_str().unwrap_or("unknown"),
            error["message"].as_str().unwrap_or("no message")
        )
        .into());
    }
    Ok(body["content"][0]["text"]
        .as_str()
        .unwrap_or("sorry, i couldn't process that request.")
        .to_string())
}
