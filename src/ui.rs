// src/ui.rs

use crate::Chatbot;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio::time;

// Represents the application state.
struct App {
    // Chat history: Vec of (User message, Assistant response)
    chat_history: Vec<(String, String)>,
    // Input buffer for the user
    input: String,
    // Flag to indicate if the app should quit
    should_quit: bool,
    // Receiver to get responses from the core logic
    response_receiver: mpsc::Receiver<String>,
    // Sender to send user queries to the core logic
    query_sender: mpsc::Sender<String>,
}

impl App {
    fn new(query_sender: mpsc::Sender<String>, response_receiver: mpsc::Receiver<String>) -> App {
        App {
            chat_history: Vec::new(),
            input: String::new(),
            should_quit: false,
            response_receiver,
            query_sender,
        }
    }

    // Add a message pair to the chat history.
    fn add_message(&mut self, user: String, assistant: String) {
        self.chat_history.push((user, assistant));
    }
}

/// Runs the terminal UI.
pub async fn run_ui(mut chatbot: Chatbot) -> Result<(), Box<dyn Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create communication channels
    let (query_sender, mut query_receiver) = mpsc::channel::<String>(100);
    let (response_sender, response_receiver) = mpsc::channel::<String>(100);

    // Spawn a task to handle core logic communication
    tokio::spawn(async move {
        while let Some(query) = query_receiver.recv().await {
            // Process the query using the chatbot
            match chatbot.chat(&query).await {
                Ok(response) => {
                    let _ = response_sender.send(response).await;
                }
                Err(e) => {
                    let error_msg = format!("Error: {}", e);
                    let _ = response_sender.send(error_msg).await;
                }
            }
        }
    });

    // Create an instance of the app
    let app = App::new(query_sender, response_receiver);
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
        eprintln!("{:?}", err)
    }

    Ok(())
}

/// Main loop of the application.
async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
) -> Result<(), Box<dyn Error>> {
    // Create a channel to communicate events
    let (tx, mut rx) = mpsc::channel::<Event>(100);

    // Spawn a task to read user input
    tokio::spawn(async move {
        let mut last_tick = Instant::now();
        loop {
            // Poll for input with timeout
            let timeout = Duration::from_millis(100);
            if event::poll(timeout).unwrap() {
                if let Ok(event) = event::read() {
                    if tx.send(Event::Input(event)).await.is_err() {
                        return;
                    }
                }
            }

            // Send tick event every 250ms
            if last_tick.elapsed() >= Duration::from_millis(250) {
                if tx.send(Event::Tick).await.is_err() {
                    return;
                }
                last_tick = Instant::now();
            }
        }
    });

    // Setup tick rate
    let tick_rate = Duration::from_millis(250);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        tokio::select! {
            Some(event) = rx.recv() => {
                match event {
                    Event::Input(event) => {
                        if handle_input(event, &mut app).await? {
                            break;
                        }
                    }
                    Event::Tick => {
                        // Handle periodic tasks if necessary
                        // For example, check for new responses
                        if let Ok(response) = app.response_receiver.try_recv() {
                            if let Some(last) = app.chat_history.last_mut() {
                                last.1 = response;
                            } else {
                                app.chat_history.push(("(No previous conversation)".to_string(), response));
                            }
                        }
                    }
                }
            }
            else => {
                break;
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Enum for different types of events.
enum Event {
    Input(CEvent),
    Tick,
}

/// Handles user input events.
async fn handle_input(event: CEvent, app: &mut App) -> Result<bool, Box<dyn Error>> {
    match event {
        CEvent::Key(key) => match key.code {
            KeyCode::Char(c) => {
                app.input.push(c);
            }
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Enter => {
                let user_input = app.input.drain(..).collect::<String>().trim().to_string();
                if !user_input.is_empty() {
                    // Add user message with empty assistant response
                    app.chat_history.push((user_input.clone(), String::new()));

                    // Send the query to the core logic
                    app.query_sender.send(user_input.clone()).await?;

                    // Clear the input buffer
                    app.input.clear();
                }
            }
            KeyCode::Esc => {
                // Set the quit flag
                app.should_quit = true;
            }
            _ => {}
        },
        _ => {}
    }

    Ok(false)
}

/// Renders the UI components.
fn ui<B: Backend>(f: &mut Frame<B>, app: &App) {
    // Define the layout: vertical split into chat history and input box
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Min(1),    // Chat history
                Constraint::Length(3), // Input box
            ]
            .as_ref(),
        )
        .split(f.size());

    // Render chat history
    let chat_messages: Vec<ratatui::text::Line> = app
        .chat_history
        .iter()
        .rev()
        .take(10) // Show last 10 messages
        .rev()
        .map(|(user, assistant)| {
            let user_span = Span::styled(
                format!("You: {}\n", user),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            );
            let assistant_span = Span::styled(
                format!("AI: {}\n", assistant),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            );
            ratatui::text::Line::from(vec![user_span, assistant_span])
        })
        .collect();

    let chat_block = Paragraph::new(chat_messages)
        .block(Block::default().borders(Borders::ALL).title("Chat History"))
        .wrap(Wrap { trim: true });

    f.render_widget(chat_block, chunks[0]);

    // Render input box
    let input = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Input"));
    f.render_widget(input, chunks[1]);

    // Set cursor position
    f.set_cursor(chunks[1].x + app.input.len() as u16 + 1, chunks[1].y + 1)
}
