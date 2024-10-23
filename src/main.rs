use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::{error, info};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    env,
    error::Error as StdError,
    fmt::{self, Display, Formatter},
    fs, io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio::time;

#[tokio::main]
async fn main() -> Result<(), Box<dyn StdError>> {
    // Initialize logging
    env_logger::init();
    info!("Starting Sagacity - Elite Terminal Assistant");

    // Create application instance
    let mut app = App::new();

    // Load settings
    load_settings(&mut app)?;

    // Load chat history
    load_chat_history(&mut app)?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the application
    let res = run_app(&mut terminal, app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        error!("Application error: {:?}", err);
        eprintln!("Error: {}", err);
    }

    Ok(())
}

/// Runs the main loop of the application
async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    mut app: App,
) -> Result<(), AppError> {
    // Setup event channel
    let (tx, mut rx) = mpsc::channel::<EventType>(100);

    // Clone tx to send to the spawned task
    let tx_clone = tx.clone();

    // Spawn a task to read user input
    tokio::spawn(async move {
        loop {
            if event::poll(Duration::from_millis(100)).unwrap() {
                if let Ok(event) = event::read() {
                    if tx_clone.send(EventType::Crossterm(event)).await.is_err() {
                        return;
                    }
                }
            }
        }
    });

    // Main loop
    loop {
        terminal.draw(|f| ui(f, &app))?;

        tokio::select! {
            Some(event) = rx.recv() => {
                match event {
                    EventType::Crossterm(crossterm_event) => {
                        let should_quit = handle_event(crossterm_event, &mut app, &tx).await?;
                        if should_quit {
                            break;
                        }
                    }
                    EventType::MockAIResponse(response) => {
                        // Update the last message's assistant response
                        if let Some(last) = app.messages.last_mut() {
                            last.assistant = response;
                        }
                    }
                    EventType::MockAIError(error_msg) => {
                        if let Some(last) = app.messages.last_mut() {
                            last.assistant = format!("Error: {}", error_msg);
                        }
                        app.notification = Some((format!("AI Error: {}", error_msg), Color::Red));
                    }
                }
            }
            _ = time::sleep(Duration::from_millis(50)) => {
                // Handle periodic tasks if necessary
            }
        }

        if let AppState::Quit = app.state {
            break;
        }
    }

    // Save chat history before exiting
    save_chat_history(&app)?;

    Ok(())
}

/// Handles incoming events and updates the application state
async fn handle_event(
    event: CEvent,
    app: &mut App,
    tx: &mpsc::Sender<EventType>,
) -> Result<bool, AppError> {
    match app.state {
        AppState::MainMenu => match event {
            CEvent::Key(key) => match key.code {
                KeyCode::Up => {
                    if app.selected_menu_item > 0 {
                        app.selected_menu_item -= 1;
                    }
                }
                KeyCode::Down => {
                    if app.selected_menu_item < menu_items().len() - 1 {
                        app.selected_menu_item += 1;
                    }
                }
                KeyCode::Enter => match app.selected_menu_item {
                    0 => app.state = AppState::Chat,
                    1 => app.state = AppState::BrowseIndex,
                    2 => app.state = AppState::GitHubRecommendations,
                    3 => app.state = AppState::Help,
                    4 => app.state = AppState::Settings,
                    5 => app.state = AppState::QuitConfirm,
                    _ => {}
                },
                KeyCode::Char('q') | KeyCode::Esc => app.state = AppState::QuitConfirm,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.state = AppState::QuitConfirm
                }
                KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.state = AppState::Help
                }
                _ => {}
            },
            _ => {}
        },
        AppState::Chat => match event {
            CEvent::Key(key) => match key.code {
                KeyCode::Char(c) => app.input.push(c),
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Enter => {
                    let user_message = app.input.drain(..).collect::<String>();
                    if !user_message.is_empty() {
                        // Push user message with a placeholder for assistant response
                        app.messages.push(ChatMessage {
                            user: user_message.clone(),
                            assistant: "Thinking...".to_string(),
                        });

                        // Mock AI response instead of actual API call
                        let mock_response = format!("Echo: {}", user_message);
                        if tx
                            .send(EventType::MockAIResponse(mock_response))
                            .await
                            .is_err()
                        {
                            error!("Failed to send AI response");
                        }
                    }
                }
                KeyCode::Esc => app.state = AppState::MainMenu,
                _ => {}
            },
            _ => {}
        },
        AppState::BrowseIndex => {
            // Placeholder for Browse Index functionality
            if let CEvent::Key(key) = event {
                if key.code == KeyCode::Esc {
                    app.state = AppState::MainMenu;
                }
            }
        }
        AppState::GitHubRecommendations => {
            // Placeholder for GitHub Recommendations functionality
            if let CEvent::Key(key) = event {
                if key.code == KeyCode::Esc {
                    app.state = AppState::MainMenu;
                }
            }
        }
        AppState::Help => {
            if let CEvent::Key(key) = event {
                if key.code == KeyCode::Esc {
                    app.state = AppState::MainMenu;
                }
            }
        }
        AppState::Settings => match event {
            CEvent::Key(key) => match key.code {
                KeyCode::Esc => app.state = AppState::MainMenu,
                KeyCode::Char('t') => {
                    // Toggle theme
                    app.settings.theme = match app.settings.theme {
                        Theme::Light => Theme::Dark,
                        Theme::Dark => Theme::Light,
                    };
                    app.notification = Some((
                        format!(
                            "Theme changed to {}",
                            match app.settings.theme {
                                Theme::Light => "Light",
                                Theme::Dark => "Dark",
                            }
                        ),
                        Color::Green,
                    ));
                    // Save settings
                    if let Err(e) = save_settings(app) {
                        app.notification =
                            Some((format!("Failed to save settings: {}", e), Color::Red));
                        error!("Failed to save settings: {:?}", e);
                    }
                }
                KeyCode::Char('k') => {
                    // Simulate updating the API key
                    app.settings.ai_api_key = "mock_api_key".to_string();
                    app.notification =
                        Some(("API Key updated successfully.".to_string(), Color::Green));
                    // Save settings
                    if let Err(e) = save_settings(app) {
                        app.notification =
                            Some((format!("Failed to save settings: {}", e), Color::Red));
                        error!("Failed to save settings: {:?}", e);
                    }
                }
                _ => {}
            },
            _ => {}
        },
        AppState::QuitConfirm => match event {
            CEvent::Key(key) => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => return Ok(true),
                KeyCode::Char('n') | KeyCode::Esc => app.state = AppState::MainMenu,
                _ => {}
            },
            _ => {}
        },
        AppState::Quit => {}
    }

    Ok(false)
}

/// Returns the main menu items with associated icons
fn menu_items() -> Vec<&'static str> {
    vec![
        "💬 Chat",
        "📂 Browse Index",
        "🔍 GitHub Recommendations",
        "❓ Help",
        "⚙️ Settings",
        "🚪 Quit",
    ]
}

/// Draws the user interface based on the current application state
fn ui(f: &mut Frame<'_>, app: &App) {
    // Define the overall layout with header, body, and footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(7), // Header
                Constraint::Min(1),    // Body
                Constraint::Length(3), // Footer
            ]
            .as_ref(),
        )
        .split(f.area());

    // Draw header
    draw_header(f, chunks[0], app);

    // Draw body based on state
    match app.state {
        AppState::MainMenu => draw_main_menu(f, chunks[1], app),
        AppState::Chat => draw_chat(f, chunks[1], app),
        AppState::BrowseIndex => draw_placeholder(f, chunks[1], "Browse Index"),
        AppState::GitHubRecommendations => draw_placeholder(f, chunks[1], "GitHub Recommendations"),
        AppState::Help => draw_help(f, chunks[1], app),
        AppState::Settings => draw_settings(f, chunks[1], app),
        AppState::QuitConfirm => draw_quit_confirm(f, chunks[1], app),
        AppState::Quit => {} // No need to draw anything; main loop will exit
    }

    // Draw footer
    draw_footer(f, chunks[2], app);

    // Draw notification if present
    draw_notification(f, app);
}

/// Draws the header with ASCII art and application title
fn draw_header(f: &mut Frame<'_>, area: Rect, app: &App) {
    // ASCII Art Logo
    let logo = r#"
     _____                 _
    / ____|               | |
   | (___  _   _ _ __ ___ | |__   ___
    \___ \| | | | '_ ` _ \| '_ \ / _ \
    ____) | |_| | | | | | | |_) | (_) |
   |_____/ \__,_|_| |_| |_|_.__/ \___/
    "#;

    // Create a block for the header background
    let block = Block::default()
        .style(
            get_theme_style(app.settings.theme)
                .fg(Color::LightCyan)
                .bg(Color::Black),
        )
        .borders(Borders::NONE);

    f.render_widget(block, area);

    // Split the header area into two parts: logo and title
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
        .split(area);

    // Render the logo
    let logo_paragraph = Paragraph::new(logo)
        .style(
            Style::default()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Left);

    f.render_widget(logo_paragraph, chunks[0]);

    // Render the title
    let title = Paragraph::new("Sagacity - Elite Terminal Assistant")
        .style(
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD | Modifier::ITALIC),
        )
        .alignment(Alignment::Center);

    f.render_widget(title, chunks[1]);
}

/// Draws the footer with dynamic instructions
fn draw_footer(f: &mut Frame<'_>, area: Rect, app: &App) {
    let instructions = match app.state {
        AppState::MainMenu => {
            "Use Up/Down arrows to navigate, Enter to select, 'q' or Esc to quit."
        }
        AppState::Chat => "Type your message and press Enter to send. Esc to return to main menu.",
        AppState::BrowseIndex => "Press Esc to return to main menu.",
        AppState::GitHubRecommendations => "Press Esc to return to main menu.",
        AppState::Help => "Press Esc to return to main menu.",
        AppState::Settings => "Press 't' to toggle theme, 'k' to update API key, Esc to return.",
        AppState::QuitConfirm => "Press 'y' to confirm quit or 'n' to cancel.",
        AppState::Quit => "",
    };

    let footer = Paragraph::new(instructions)
        .style(Style::default().fg(Color::LightCyan))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(footer, area);
}

/// Draws the main menu with selectable items and icons
fn draw_main_menu(f: &mut Frame<'_>, area: Rect, app: &App) {
    // Create a block for the menu background
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Main Menu")
        .style(
            get_theme_style(app.settings.theme)
                .fg(Color::LightYellow)
                .bg(Color::Black),
        );

    f.render_widget(block, area);

    // Create menu items with icons
    let items: Vec<ListItem> = menu_items()
        .iter()
        .enumerate()
        .map(|(i, &item)| {
            let content = if i == app.selected_menu_item {
                // Highlight selected item
                ListItem::new(item).style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::LightMagenta)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(item).style(Style::default().fg(Color::White))
            };
            content
        })
        .collect();

    let list = List::new(items)
        .block(Block::default())
        .highlight_style(Style::default().bg(Color::LightMagenta).fg(Color::Black))
        .highlight_symbol("➤ ");

    // Calculate the layout for the list
    let list_area = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Min(1)].as_ref())
        .split(area)[0];

    f.render_widget(list, list_area);
}

/// Draws the chat interface with enhanced styling
fn draw_chat(f: &mut Frame<'_>, area: Rect, app: &App) {
    // Create a block for the chat background
    let block = Block::default().borders(Borders::ALL).title("Chat").style(
        get_theme_style(app.settings.theme)
            .fg(Color::LightYellow)
            .bg(Color::Black),
    );

    f.render_widget(block, area);

    // Split chat area into message view and input
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Min(1),    // Messages
                Constraint::Length(3), // Input
            ]
            .as_ref(),
        )
        .split(area);

    // Render messages with distinct styles
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|msg| {
            let content = format!("💬 You: {}\n🤖 AI: {}", msg.user, msg.assistant);
            ListItem::new(content).style(Style::default().fg(Color::White))
        })
        .collect();

    let messages_list = List::new(messages)
        .block(Block::default())
        .style(Style::default())
        .highlight_style(Style::default())
        .highlight_symbol("");

    f.render_widget(messages_list, chunks[0]);

    // Render input box with blinking cursor simulation
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::LightYellow))
        .block(Block::default().borders(Borders::ALL).title("Input"))
        .alignment(Alignment::Left);

    f.render_widget(input, chunks[1]);

    // Set cursor position
    let x = chunks[1].x + app.input.len() as u16 + 1;
    let y = chunks[1].y + 1;
    f.set_cursor_position((x, y));
}

/// Draws placeholder screens for Browse Index and GitHub Recommendations with enhanced styling
fn draw_placeholder(f: &mut Frame<'_>, area: Rect, title: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black));

    f.render_widget(block, area);

    let placeholder_text = format!(
        "{} functionality is under construction.\n\nPress 'Esc' to return to the main menu.",
        title
    );

    let paragraph = Paragraph::new(placeholder_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Draws the help screen with improved formatting and styling
fn draw_help(f: &mut Frame<'_>, area: Rect, app: &App) {
    // Create a block for the help background
    let block = Block::default().borders(Borders::ALL).title("Help").style(
        get_theme_style(app.settings.theme)
            .fg(Color::LightYellow)
            .bg(Color::Black),
    );

    f.render_widget(block, area);

    // Define help text with clear formatting
    let help_text = "\
📚 **Navigation:**
 - Use **Up/Down arrows** to navigate the main menu.
 - Press **Enter** to select an option.
 - In **Chat**, type your message and press **Enter** to send.
 - Press **Esc** to return to the main menu from any screen.

💡 **Tips:**
 - Be clear and concise in your messages for better responses.
 - Explore different features as they become available.

Press **Esc** to return to the main menu.";

    let paragraph = Paragraph::new(help_text)
        .style(Style::default().fg(Color::White))
        .block(Block::default())
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Draws the settings screen with options to change theme and update API key
fn draw_settings(f: &mut Frame<'_>, area: Rect, app: &App) {
    // Create a block for the settings background
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Settings")
        .style(
            get_theme_style(app.settings.theme)
                .fg(Color::LightYellow)
                .bg(Color::Black),
        );

    f.render_widget(block, area);

    // Define settings text
    let settings_text = format!(
        "🔑 **API Key:** {}\n🎨 **Theme:** {:?}\n\nOptions:\n - Press 't' to toggle theme.\n - Press 'k' to update API key.\n\nPress **Esc** to return to main menu.",
        if app.settings.ai_api_key.is_empty() {
            "Not Set".to_string()
        } else {
            "******".to_string()
        },
        app.settings.theme
    );

    let paragraph = Paragraph::new(settings_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Draws the quit confirmation screen with interactive options and enhanced styling
fn draw_quit_confirm(f: &mut Frame<'_>, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Confirm Quit")
        .style(
            get_theme_style(app.settings.theme)
                .fg(Color::LightYellow)
                .bg(Color::Black),
        );

    f.render_widget(block, area);

    // Define confirmation text
    let quit_text = "🚪 **Are you sure you want to quit?**\n\nPress **'y'** to confirm quit or **'n'** to cancel.";

    let paragraph = Paragraph::new(quit_text)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Draws a notification message if present
fn draw_notification(f: &mut Frame<'_>, app: &App) {
    if let Some((msg, color)) = &app.notification {
        let size = f.area(); // Updated from f.size() to f.area()
        let block = Block::default()
            .style(Style::default().fg(*color).bg(Color::Black))
            .borders(Borders::ALL)
            .title("Notification");

        let paragraph = Paragraph::new(msg.clone())
            .style(Style::default().fg(*color))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        let area = Rect {
            x: size.x + 2,
            y: size.y + 2,
            width: size.width - 4,
            height: 5,
        };

        f.render_widget(block, area);
        f.render_widget(paragraph, area);
    }
}

/// Returns the style based on the current theme
fn get_theme_style(theme: Theme) -> Style {
    match theme {
        Theme::Light => Style::default().bg(Color::White).fg(Color::Black),
        Theme::Dark => Style::default().bg(Color::Black).fg(Color::White),
    }
}

/// Loads settings from a JSON file or environment variables
fn load_settings(app: &mut App) -> Result<(), AppError> {
    // Attempt to load settings from a file
    if let Ok(data) = fs::read_to_string("settings.json") {
        let settings: Settings = serde_json::from_str(&data)?;
        app.settings = settings;
        info!("Settings loaded from file.");
    } else {
        // If file doesn't exist, load from environment variables
        let api_key = env::var("OPENAI_API_KEY").unwrap_or_default();
        app.settings.ai_api_key = api_key;
        app.settings.theme = Theme::Light;
        info!("Settings loaded from environment variables.");
    }

    Ok(())
}

/// Saves settings to a JSON file
fn save_settings(app: &App) -> Result<(), AppError> {
    let data = serde_json::to_string_pretty(&app.settings)?;
    fs::write("settings.json", data)?;
    info!("Settings saved to file.");
    Ok(())
}

/// Loads chat history from a JSON file
fn load_chat_history(app: &mut App) -> Result<(), AppError> {
    if let Ok(data) = fs::read_to_string("chat_history.json") {
        let chat_history: Vec<ChatMessage> = serde_json::from_str(&data)?;
        app.messages = chat_history;
        info!("Chat history loaded from file.");
    }
    Ok(())
}

/// Saves chat history to a JSON file
fn save_chat_history(app: &App) -> Result<(), AppError> {
    let chat_history = &app.messages;
    let data = serde_json::to_string_pretty(chat_history)?;
    fs::write("chat_history.json", data)?;
    info!("Chat history saved to file.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_response() {
        let input = "Hello, AI!";
        let expected = "Echo: Hello, AI!";
        let mock_response = format!("Echo: {}", input);
        assert_eq!(mock_response, expected);
    }

    // Add more tests for different components
}

/// Placeholder function for simulating assistant response
/// This function replaces the actual AI integration
fn mock_ai_response(user_message: &str) -> String {
    format!("Echo: {}", user_message)
}

/// Define custom events that include both CEvent and Mock AI responses
enum EventType {
    Crossterm(CEvent),
    MockAIResponse(String),
    MockAIError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppState {
    MainMenu,
    Chat,
    BrowseIndex,
    GitHubRecommendations,
    Help,
    Settings,
    QuitConfirm,
    Quit,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    user: String,
    assistant: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Settings {
    ai_api_key: String,
    theme: Theme,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
enum Theme {
    Light,
    Dark,
}

struct App {
    state: AppState,
    messages: Vec<ChatMessage>,
    input: String,
    tick_rate: Duration,
    selected_menu_item: usize,
    notification: Option<(String, Color)>,
    settings: Settings,
    last_tick: Instant,
}

impl App {
    fn new() -> App {
        App {
            state: AppState::MainMenu,
            messages: Vec::new(),
            input: String::new(),
            tick_rate: Duration::from_millis(250),
            selected_menu_item: 0,
            notification: None,
            settings: Settings {
                ai_api_key: String::new(),
                theme: Theme::Light,
            },
            last_tick: Instant::now(),
        }
    }
}

#[derive(Debug)]
enum AppError {
    IoError(io::Error),
    ApiError(String),
    SerdeError(serde_json::Error),
    MissingApiKey,
}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError::IoError(err)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::SerdeError(err)
    }
}

// Implement Display and Error for AppError
impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AppError::IoError(e) => write!(f, "IO Error: {}", e),
            AppError::ApiError(msg) => write!(f, "API Error: {}", msg),
            AppError::SerdeError(e) => write!(f, "Serialization Error: {}", e),
            AppError::MissingApiKey => write!(f, "Missing API Key"),
        }
    }
}

impl StdError for AppError {}
