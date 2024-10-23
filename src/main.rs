mod app;
pub mod chat;

use app::*;

use chat::{Message, Sender};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    error::Error as StdError,
    io,
    time::{Duration, Instant},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn StdError>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create application instance
    let mut app = App::new();

    // Run the UI
    let res = run_ui(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {}", err);
    }

    Ok(())
}

/// Runs the UI loop of the application
async fn run_ui(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn StdError>> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        // Poll for events with a timeout
        if event::poll(Duration::from_millis(100))? {
            if let CEvent::Key(key) = event::read()? {
                match app.state {
                    AppState::MainMenu => match key.code {
                        KeyCode::Up => {
                            if app.selected_menu_item > 0 {
                                app.selected_menu_item -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if app.selected_menu_item < app.menu_items.len() - 1 {
                                app.selected_menu_item += 1;
                            }
                        }
                        KeyCode::Enter => {
                            // Change state based on selected menu item
                            app.state = match app.selected_menu_item {
                                0 => AppState::Chat,
                                1 => AppState::BrowseIndex,
                                2 => AppState::GitHubRecommendations,
                                3 => AppState::Help,
                                4 => AppState::Settings,
                                5 => AppState::QuitConfirm,
                                _ => AppState::MainMenu,
                            };
                        }
                        KeyCode::Char('q') | KeyCode::Esc => app.state = AppState::QuitConfirm,
                        _ => {}
                    },
                    AppState::Chat => match key.code {
                        KeyCode::Esc => {
                            app.state = AppState::MainMenu;
                        }
                        KeyCode::Enter => {
                            let user_message = app.input.drain(..).collect::<String>();
                            if !user_message.trim().is_empty() {
                                app.messages.push(Message {
                                    sender: Sender::User,
                                    content: user_message.clone(),
                                });
                                // Here you can implement sending the message to your backend or AI
                                // For demonstration, we'll add a mock AI responsestruct Sen
                                app.messages.push(Message {
                                    sender: Sender::AI,
                                    content: format!("Echo: {}", user_message),
                                });
                            }
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Char(c) => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                // Handle Ctrl+C for quitting
                                if c == 'c' {
                                    app.state = AppState::QuitConfirm;
                                }
                            } else {
                                app.input.push(c);
                            }
                        }
                        _ => {}
                    },
                    AppState::QuitConfirm => match key.code {
                        KeyCode::Char('y') | KeyCode::Enter => {
                            app.state = AppState::Quit;
                        }
                        KeyCode::Char('n') | KeyCode::Esc => {
                            app.state = AppState::MainMenu;
                        }
                        _ => {}
                    },
                    // Handle other states if necessary
                    _ => {
                        // From any other state, pressing 'q' or Esc brings up the quit confirmation prompt
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => app.state = AppState::QuitConfirm,
                            _ => {}
                        }
                    }
                }
            }
        }

        // Exit the loop if the state is Quit
        if app.state == AppState::Quit {
            break;
        }
    }

    Ok(())
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
        .split(f.size());

    // Draw header
    draw_header(f, chunks[0]);

    // Draw body based on state
    match app.state {
        AppState::MainMenu => draw_main_menu(f, chunks[1], app),
        AppState::Chat => draw_chat(f, chunks[1], app),
        AppState::BrowseIndex => draw_placeholder(f, chunks[1], "Browse Index"),
        AppState::GitHubRecommendations => draw_placeholder(f, chunks[1], "GitHub Recommendations"),
        AppState::Help => draw_placeholder(f, chunks[1], "Help"),
        AppState::Settings => draw_placeholder(f, chunks[1], "Settings"),
        AppState::QuitConfirm => draw_quit_confirm(f, chunks[1]),
        AppState::Quit => {} // No need to draw anything; main loop will exit
    }

    // Draw footer
    draw_footer(f, chunks[2], app);
}

/// Draws the header with ASCII art and application title
fn draw_header(f: &mut Frame<'_>, area: Rect) {
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
        .style(Style::default().fg(Color::LightCyan).bg(Color::Black))
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
        AppState::QuitConfirm => "Press 'y' to confirm quit or 'n' to cancel.",
        _ => "Press 'q' or Esc to quit.",
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
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black));

    f.render_widget(block, area);

    // Define menu items with icons
    let items: Vec<ListItem> = app
        .menu_items
        .iter()
        .enumerate()
        .map(|(i, &item)| {
            if i == app.selected_menu_item {
                ListItem::new(item).style(
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::LightMagenta)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(item).style(Style::default().fg(Color::White))
            }
        })
        .collect();

    let list = List::new(items)
        .block(Block::default())
        .highlight_style(
            Style::default()
                .bg(Color::LightMagenta)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");

    // Calculate the layout for the list
    let list_area = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Min(1)].as_ref())
        .split(area)[0];

    f.render_widget(list, list_area);
}

/// Draws the chat interface with message display and input area
fn draw_chat(f: &mut Frame<'_>, area: Rect, app: &App) {
    // Create a block for the chat background
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Chat")
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black));

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

    // Render messages
    let messages: Vec<ListItem> = app
        .messages
        .iter()
        .map(|msg| {
            let prefix = match msg.sender {
                Sender::User => "💬 You: ",
                Sender::AI => "🤖 AI: ",
            };
            ListItem::new(format!("{}{}", prefix, msg.content)).style(
                Style::default()
                    .fg(match msg.sender {
                        Sender::User => Color::LightGreen,
                        Sender::AI => Color::LightBlue,
                    })
                    .add_modifier(Modifier::ITALIC),
            )
        })
        .collect();

    let messages_list = List::new(messages)
        .block(Block::default())
        .style(Style::default())
        .highlight_style(Style::default())
        .highlight_symbol("");

    f.render_widget(messages_list, chunks[0]);

    // Render input box
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::LightYellow))
        .block(Block::default().borders(Borders::ALL).title("Input"))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });

    f.render_widget(input, chunks[1]);

    // Set cursor position
    let x = chunks[1].x + app.input.len() as u16 + 1;
    let y = chunks[1].y + 1;
    f.set_cursor(x, y);
}

/// Draws placeholder screens for different states with enhanced styling
fn draw_placeholder(f: &mut Frame<'_>, area: Rect, title: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black));

    f.render_widget(block, area);

    let placeholder_text = match title {
        "Chat" => "💬 Chat functionality will be implemented here.\n\nPress 'q' or Esc to return to the main menu.",
        "Browse Index" => "📂 Browse Index functionality is under construction.\n\nPress 'q' or Esc to return to the main menu.",
        "GitHub Recommendations" => "🔍 GitHub Recommendations functionality is under construction.\n\nPress 'q' or Esc to return to the main menu.",
        "Help" => "❓ Help information will be available here.\n\nPress 'q' or Esc to return to the main menu.",
        "Settings" => "⚙️ Settings will be configurable here.\n\nPress 'q' or Esc to return to the main menu.",
        _ => "Under construction.\n\nPress 'q' or Esc to return to the main menu.",
    };

    let paragraph = Paragraph::new(placeholder_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

/// Draws the quit confirmation screen with interactive options and enhanced styling
fn draw_quit_confirm(f: &mut Frame<'_>, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Confirm Quit")
        .style(Style::default().fg(Color::LightYellow).bg(Color::Black));

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
