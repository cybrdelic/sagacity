// src/main.rs

use std::{
    env,
    error::Error,
    io,
    sync::Arc,
    time::{Duration, SystemTime},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};
use tokio::sync::Mutex;

mod chat_message;
mod chat_view;
mod indexing_view;
mod models;
mod splash_screen;
mod status_indicator;

use chat_view::{draw_chat, simulate_chat_response};
use indexing_view::{draw_indexing, indexing_task};
use models::{Chatbot, LogPanel, TreeNode};
use splash_screen::{SplashScreen, SplashScreenAction};
use status_indicator::StatusIndicator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppScreen {
    Splash,
    Indexing,
    Chat,
}

#[derive(Debug)]
struct App {
    screen: AppScreen,
    splash_screen: SplashScreen,
    tree: Vec<TreeNode>,
    indexing_done: bool,
    indexing_count: usize,
    chat_input: String,
    chat_messages: Vec<(String, bool)>,
    logs: LogPanel,
    spinner_idx: usize,
    chat_thinking: bool,
    chatbot: Chatbot,
    status_indicator: StatusIndicator,
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
        }
    }
}

/// Renders the entire UI based on the current screen.
fn draw_ui(f: &mut Frame, app: &mut App) {
    match app.screen {
        AppScreen::Splash => app.splash_screen.draw(f, f.area()), // Replaced f.size() with f.area()
        AppScreen::Indexing => draw_indexing(f, app),
        AppScreen::Chat => draw_chat(f, app),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize environment variables from .env file
    dotenv::dotenv().ok();

    // Setup terminal
    setup_terminal()?;

    // Create terminal backend
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    // Initialize application state
    let app = Arc::new(Mutex::new(App::new()));

    // Run the application
    let res = run_app(&mut terminal, app.clone()).await;

    // Restore terminal
    restore_terminal(&mut terminal)?;

    if let Err(err) = res {
        eprintln!("Application error: {}", err);
    }

    Ok(())
}

/// Sets up the terminal in raw mode and enters the alternate screen.
fn setup_terminal() -> Result<(), Box<dyn Error + Send + Sync>> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    Ok(())
}

/// Restores the terminal to its original state.
fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Runs the main application loop.
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: Arc<Mutex<App>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    loop {
        {
            let mut guard = app.lock().await;
            guard.spinner_idx = guard.spinner_idx.wrapping_add(1);
            // Removed `status_indicator` update
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

/// Handles key events and determines if the application should exit.
///
/// Returns `true` if the application should terminate, otherwise `false`.
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

/// Handles key events when in the Splash screen.
///
/// Returns `true` if the application should terminate, otherwise `false`.
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

/// Handles key events when in the Indexing screen.
///
/// Returns `true` if the application should terminate, otherwise `false`.
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

/// Handles key events when in the Chat screen.
///
/// Returns `true` if the application should terminate, otherwise `false`.
async fn handle_chat_input(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    app_arc: Arc<Mutex<App>>,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Ok(true),
        (KeyModifiers::NONE, KeyCode::Enter) => {
            let input_text = app.chat_input.trim().to_string();
            if !input_text.is_empty() {
                app.chat_messages.push((input_text.clone(), true));
                let clone = app_arc.clone();
                tokio::spawn(async move {
                    simulate_chat_response(clone, input_text).await;
                });
                app.chat_input.clear();
            }
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.chat_input.pop();
        }
        (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            app.chat_input.push(c);
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            if app.chat_scroll > 0 {
                app.chat_scroll -= 1;
            }
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            app.chat_scroll += 1;
        }
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            app.chat_scroll = app.chat_scroll.saturating_sub(10);
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            app.chat_scroll += 10;
        }
        _ => {}
    }
    Ok(false)
}
