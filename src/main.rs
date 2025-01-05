use std::{
    collections::HashMap,
    env,
    error::Error,
    io::{self},
    sync::Arc,
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
use tokio::sync::Mutex;

mod chat_view;
mod models;
mod splash_screen;
mod status_indicator;

use chat_view::{draw_chat, simulate_chat_response};
use models::{Chatbot, LogPanel, TreeNode};
use splash_screen::{SplashScreen, SplashScreenAction};
use status_indicator::StatusIndicator;

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

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
        AppScreen::Splash => app.splash_screen.draw(f, f.area()),
        AppScreen::Indexing => draw_indexing(f, app),
        AppScreen::Chat => {
            // Since `draw_chat` is async, we need to handle it appropriately.
            // However, `draw_ui` itself is not async, so we should adjust `draw_chat` to be synchronous.
            // Alternatively, handle rendering differently. For simplicity, we'll make `draw_chat` synchronous.
            // Ensure that `draw_chat` in `chat_view.rs` is not async.
            // For now, assuming it's synchronous:
            draw_chat(f, app);
        }
    }
}

/// Renders the indexing screen.
fn draw_indexing(f: &mut Frame, app: &mut App) {
    let size = f.size();
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
        log_lines.push(Line::from(Span::raw(entry)));
    }

    let logs_para = Paragraph::new(log_lines)
        .wrap(Wrap { trim: true })
        .scroll((app.logs_scroll, 0));
    f.render_widget(logs_para, inner_logs_area);
}

/// Handles the indexing task asynchronously.
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
        if let Ok(summary) = chat_view::summarize_file(&content, "rust", &api_key).await {
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
                draw_ui(f, &mut guard);
            })?;
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    let mut guard = app.lock().await;
                    match guard.screen {
                        AppScreen::Splash => {
                            if let Some(action) = guard.splash_screen.handle_input(key) {
                                match action {
                                    SplashScreenAction::Quit => break 'outer,
                                    SplashScreenAction::StartChat => {
                                        guard.screen = AppScreen::Indexing;
                                        let clone = app.clone();
                                        drop(guard);
                                        tokio::spawn(async move {
                                            indexing_task(clone).await;
                                        });
                                    }
                                }
                            }
                        }
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
