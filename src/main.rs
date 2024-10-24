mod app;
pub mod ui;

use app::*;
use ui::chat::draw_chat;
use ui::chat::{Message, Sender};
use ui::footer::draw_footer;
use ui::header::draw_header;
use ui::main_menu::draw_main_menu;
use ui::placeholder::draw_placeholder;
use ui::quit_confirm::draw_quit_confirm;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    Frame, Terminal,
};
use std::{error::Error as StdError, io, time::Duration};

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
        .split(f.area());

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
        AppState::SelectCodebase => {
            // Render the directory tree
            app.dir_tree.render(f, chunks[1]);
        }
        AppState::Quit => {}
    }

    // Draw footer
    draw_footer(f, chunks[2], app);
}
