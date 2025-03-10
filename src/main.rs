use std::{
    env,
    error::Error,
    io,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

mod api;
mod build;
mod chat_message;
mod chat_view;
mod code_snippet;
mod config;
mod db;
mod db_details_view;
mod errors;
mod indexing_view;
mod log_view;
mod models;
mod splash_screen;
mod status_indicator;
mod test_view;

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

// Import public constants from api module
use crate::api::{ANTHROPIC_VERSION, CLAUDE_API_URL};

use crate::{
    chat_view::{draw_chat, simulate_chat_response},
    config::initialize_config,
    db::Db,
    errors::{SagacityError, SagacityResult},
    indexing_view::{draw_indexing, indexing_task},
    models::{Chatbot, TreeNode},
    splash_screen::{SplashScreen, SplashScreenAction},
    status_indicator::StatusIndicator,
    test_view::{TestView, draw_test_view, run_tests},
};

// --- Logging Initialization ---
// We use flexi_logger to write logs to a file without interfering with TUI output.
use flexi_logger::{FileSpec, Logger, WriteMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppScreen {
    Splash,
    Indexing,
    Chat,
    DBDetails,
    Tests,
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
    pub test_view: TestView,
    command_history: Vec<String>,
    command_index: Option<usize>,
    run_tests_on_startup: bool,
}

impl App {
    fn new() -> Self {
        let api_key = env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        let chatbot = Chatbot::new(api_key);
        
        // Check if tests should run on startup
        let run_tests_on_startup = env::args().any(|arg| arg == "--run-tests");
        
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
            test_view: TestView::new(),
            command_history: Vec::new(),
            command_index: None,
            run_tests_on_startup,
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

    // Initialize configuration
    if let Err(e) = initialize_config() {
        eprintln!("Failed to initialize configuration: {:?}", e);
        return Err(Box::<dyn Error + Send + Sync>::from(e));
    }

    // Initialize flexi_logger to write logs to a file.
    if let Err(e) = Logger::try_with_str("info")
        .map_err(|e| format!("Logger error: {}", e))?
        .write_mode(WriteMode::BufferAndFlush)
        .log_to_file(FileSpec::default())
        .start() {
        return Err(Box::<dyn Error + Send + Sync>::from(format!("Logger start error: {}", e)));
    }

    setup_terminal()?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let app = Arc::new(Mutex::new(App::new()));

    // Initialize database
    {
        let mut guard = app.lock().await;
        let db_instance = match Db::init(&guard.db_path).await {
            Ok(db) => db,
            Err(e) => return Err(Box::<dyn Error + Send + Sync>::from(format!("Database initialization error: {}", e))),
        };
        guard.db = Some(db_instance);
        guard.logs.add("db initialized successfully".to_string());
        
        // Run tests on startup if flag is set
        if guard.run_tests_on_startup {
            guard.screen = AppScreen::Tests;
            let app_clone = app.clone();
            tokio::spawn(async move {
                run_tests(app_clone).await;
            });
        }
    }

    let res = run_app(&mut terminal, app.clone()).await;
    
    // Handle terminal restoration
    if let Err(e) = restore_terminal(&mut terminal) {
        eprintln!("Failed to restore terminal: {}", e);
        return Err(e);
    }

    // Handle application errors
    if let Err(err) = res {
        eprintln!("application error: {}", err);
        return Err(err);
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
        AppScreen::Chat => crate::chat_view::draw_chat(f, app),
        AppScreen::DBDetails => {
            // Use block_in_place to avoid starting a nested runtime.
            tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(db_details_view::draw_db_details(f, app))
            });
        },
        AppScreen::Tests => draw_test_view(f, app),
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
        AppScreen::Tests => handle_test_input(app, key),
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
            SplashScreenAction::RunTests => {
                app.screen = AppScreen::Tests;
                let clone = app_arc.clone();
                tokio::spawn(async move {
                    run_tests(clone).await;
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
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Ok(true),
        (KeyModifiers::NONE, KeyCode::Esc) => {
            if app.input_mode == InputMode::Command {
                app.input_mode = InputMode::Normal;
                app.command_buffer.clear();
            } else if app.focused_message_index.is_some() {
                app.focused_message_index = None;
            } else {
                app.screen = AppScreen::Splash;
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if !app.chat_input.trim().is_empty() && !app.chat_thinking {
                let input = app.chat_input.clone();
                app.chat_messages.push(ChatMessage::new(input.clone(), true));
                app.chat_input.clear();
                app.focused_message_index = None;
                
                let app_clone = app_arc.clone();
                tokio::spawn(async move {
                    chat_view::simulate_chat_response(app_clone, input).await;
                });
            }
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.chat_input.pop();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            app.chat_input.clear();
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            if let Some(idx) = app.focused_message_index {
                if idx > 0 {
                    app.focused_message_index = Some(idx - 1);
                }
            } else if !app.chat_messages.is_empty() {
                app.focused_message_index = Some(app.chat_messages.len() - 1);
            }
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            if let Some(idx) = app.focused_message_index {
                if idx < app.chat_messages.len() - 1 {
                    app.focused_message_index = Some(idx + 1);
                } else {
                    app.focused_message_index = None;
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::PageUp) => {
            if app.chat_scroll > 0 {
                app.chat_scroll = app.chat_scroll.saturating_sub(10);
            }
        }
        (KeyModifiers::NONE, KeyCode::PageDown) => {
            app.chat_scroll = app.chat_scroll.saturating_add(10);
        }
        (KeyModifiers::NONE, KeyCode::Char(c)) => {
            app.chat_input.push(c);
        }
        _ => {}
    }
    Ok(false)
}

fn handle_test_input(
    app: &mut App, 
    key: crossterm::event::KeyEvent
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => return Ok(true),
        (KeyModifiers::NONE, KeyCode::Esc) => {
            app.logs.add("exiting test screen, returning to chat".to_string());
            app.screen = AppScreen::Chat;
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            app.test_view.select_prev();
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            app.test_view.select_next();
        }
        (KeyModifiers::NONE, KeyCode::Char('r')) => {
            app.logs.add("rerunning tests".to_string());
            if let Err(e) = app.test_view.run_all_tests() {
                app.logs.add(format!("failed to run tests: {}", e));
            }
        }
        _ => {}
    }
    Ok(false)
}