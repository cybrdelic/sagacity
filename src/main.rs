use std::{
    env,
    error::Error,
    io,
    sync::Arc,
    time::{Duration, SystemTime},
};
mod chat_message;
mod chat_view;
mod indexing_view;
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
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use tokio::sync::Mutex;

use crate::{
    chat_view::{draw_chat, simulate_chat_response},
    indexing_view::{draw_indexing, indexing_task},
    models::{Chatbot, LogPanel, TreeNode},
    splash_screen::{SplashScreen, SplashScreenAction},
    status_indicator::StatusIndicator,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Splash,
    Indexing,
    Chat,
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
    logs: LogPanel,
    spinner_idx: usize,
    chat_thinking: bool,
    chatbot: Chatbot,
    status_indicator: StatusIndicator,
    indexing_start_time: Option<SystemTime>,
    chat_scroll: u16,
    logs_scroll: u16,
    pub focused_message_index: Option<usize>,
    input_mode: InputMode,
    command_buffer: String,
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
            logs: LogPanel::new(),
            spinner_idx: 0,
            chat_thinking: false,
            chatbot,
            status_indicator: StatusIndicator::new(),
            indexing_start_time: None,
            chat_scroll: 0,
            logs_scroll: 0,
            focused_message_index: None,
            input_mode: InputMode::Normal,
            command_buffer: String::new(),
        }
    }

    pub fn get_focused_message(&mut self) -> Option<&mut ChatMessage> {
        self.logs.add("Getting focused message");
        if let Some(index) = self.focused_message_index {
            self.logs
                .add(&format!("Attempting to get message at index {}", index));
            self.chat_messages.get_mut(index)
        } else {
            self.logs.add("No focused message");
            None
        }
    }

    pub fn log_state(&mut self) {
        self.logs.add(&format!(
            "State: msg_idx={:?}, msgs={}, scroll={}",
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
    let res = run_app(&mut terminal, app.clone()).await;
    restore_terminal(&mut terminal)?;

    if let Err(err) = res {
        eprintln!("Application error: {}", err);
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
            app.logs.add("Indexing cancelled by user");
            app.screen = AppScreen::Chat;
        }
        _ => {}
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
                app.logs.add("⬆️ UP pressed");
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
                app.logs.add("⬇️ DOWN pressed");
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
                app.logs.add("⏎ ENTER pressed");
                app.log_state();

                if let Some(msg) = app.get_focused_message() {
                    if let Some(content) = msg.get_focused_content() {
                        match copy_to_clipboard(&content) {
                            Ok(_) => {
                                app.logs.add("✓ Content copied to clipboard!");
                                app.status_indicator.set_status("Copied to clipboard!");
                            }
                            Err(e) => {
                                app.logs.add(&format!("⚠ Failed to copy: {}", e));
                                app.status_indicator.set_status("Copy failed - see logs");
                            }
                        }
                    }
                } else {
                    let input_text = app.chat_input.trim().to_string();
                    if !input_text.is_empty() {
                        app.logs.add("Sending chat message");
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
                app.logs.add("ESC pressed - clearing focus");
                if let Some(msg) = app.get_focused_message() {
                    msg.focused_chunk = None;
                }
                app.focused_message_index = None;
                app.log_state();
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Char(':')) => {
                app.logs.add("Entering command mode");
                app.input_mode = InputMode::Command;
                app.command_buffer.clear();
                app.command_buffer.push(':');
                Ok(false)
            }
            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                app.logs.add("Ctrl+C pressed - exiting");
                Ok(true)
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                app.chat_input.pop();
                app.logs.add("Backspace - removing last character");
                Ok(false)
            }
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                app.chat_input.push(c);
                app.logs.add("Adding character to input");
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::PageUp) => {
                app.chat_scroll = app.chat_scroll.saturating_sub(10);
                app.logs.add("PageUp - scrolling up by 10");
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::PageDown) => {
                app.chat_scroll += 10;
                app.logs.add("PageDown - scrolling down by 10");
                Ok(false)
            }
            _ => Ok(false),
        },
        InputMode::Command => match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Esc) => {
                app.logs
                    .add("ESC pressed in command mode - returning to normal mode");
                app.input_mode = InputMode::Normal;
                app.command_buffer.clear();
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Enter) => {
                app.logs.add("Executing command");
                handle_command(app)?;
                app.input_mode = InputMode::Normal;
                app.command_buffer.clear();
                Ok(false)
            }
            (KeyModifiers::NONE, KeyCode::Backspace) => {
                app.command_buffer.pop();
                if app.command_buffer.is_empty() {
                    app.logs
                        .add("Command buffer empty - returning to normal mode");
                    app.input_mode = InputMode::Normal;
                } else {
                    app.logs.add("Backspace in command buffer");
                }
                Ok(false)
            }
            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                app.command_buffer.push(c);
                app.logs.add("Adding character to command buffer");
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
                    app.logs.add(&format!("Command: copy block {}", block_num));
                    if let Some(msg) = app.get_focused_message() {
                        if let Some(content) = msg.handle_esc_number(block_num) {
                            match copy_to_clipboard(&content) {
                                Ok(_) => {
                                    app.logs.add(&format!("Copied code block {}", block_num));
                                    app.status_indicator.set_status("Copied to clipboard!");
                                }
                                Err(e) => {
                                    app.logs
                                        .add(&format!("Failed to copy block {}: {}", block_num, e));
                                    app.status_indicator.set_status("Copy failed - see logs");
                                }
                            }
                        } else {
                            app.logs.add(&format!("No code block {}", block_num));
                        }
                    }
                }
            }
        }
        "focus" | "f" => {
            if parts.len() >= 2 {
                if let Ok(msg_num) = parts[1].parse::<usize>() {
                    app.logs.add(&format!("Command: focus message {}", msg_num));
                    if msg_num > 0 && msg_num <= app.chat_messages.len() {
                        app.focused_message_index = Some(msg_num - 1);
                        app.logs.add(&format!("Focused message {}", msg_num));
                    }
                }
            }
        }
        "help" | "h" => {
            app.logs.add("Commands: :copy <n> | :focus <n> | :help");
        }
        _ => {
            app.logs.add("Unknown command. Try :help");
        }
    }
    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    let mut ctx =
        ClipboardContext::new().map_err(|e| format!("Failed to access clipboard: {}", e))?;
    ctx.set_contents(text.to_owned())
        .map_err(|e| format!("Failed to set clipboard contents: {}", e))?;
    Ok(())
}
