use std::{
    error::Error,
    io::{self},
    time::Duration,
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
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::sleep;

#[derive(Clone, Debug)]
struct TreeNode {
    filename: String,
    progress: f32,
    status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppScreen {
    Splash,
    Indexing,
    Chat,
}

#[derive(Debug)]
struct LogPanel {
    entries: Vec<String>,
    visible: bool,
}

impl LogPanel {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            visible: false,
        }
    }
    fn add(&mut self, msg: impl Into<String>) {
        self.entries.push(msg.into());
        if self.entries.len() > 200 {
            self.entries.remove(0);
        }
    }
}

struct App {
    screen: AppScreen,

    // splash
    splash_selected_idx: usize,
    splash_menu_items: Vec<&'static str>,

    // indexing
    tree: Vec<TreeNode>,
    indexing_done: bool,
    indexing_count: usize,

    // chat
    chat_input: String,
    chat_messages: Vec<(String, bool)>, // (message, is_user)

    // logs
    logs: LogPanel,

    // spinner
    spinner_idx: usize,

    // thinking
    chat_thinking: bool,
}

impl App {
    fn new() -> Self {
        Self {
            screen: AppScreen::Splash,
            splash_selected_idx: 0,
            splash_menu_items: vec!["start chat", "quit"],

            tree: vec![],
            indexing_done: false,
            indexing_count: 0,

            chat_input: String::new(),
            chat_messages: vec![],

            logs: LogPanel::new(),
            spinner_idx: 0,
            chat_thinking: false,
        }
    }
}

// -- drawing

fn draw_ui(f: &mut Frame, app: &App) {
    match app.screen {
        AppScreen::Splash => draw_splash(f, app),
        AppScreen::Indexing => draw_indexing(f, app),
        AppScreen::Chat => draw_chat(f, app),
    }
}

fn draw_splash(f: &mut Frame, app: &App) {
    let size = f.area();

    // horizontal split: 40% for ascii, 60% for menu
    let hsplit = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(size);

    // the ascii block
    let ascii_art = r"
▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
██ ▄▀▄ █ ██ █ ▄▄▀█▀ ██ ▄▄▀█ ▄▀████ ▄▄▀██▄█
██ █ █ █ ▀▀ █ ▀▀▄██ ██ ▀▀ █ █ █▀▀█ ▀▀ ██ ▄
██ ███ █▀▀▀▄█▄█▄▄█▀ ▀█▄██▄█▄▄██▄▄█▄██▄█▄▄▄
▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
An Intelligent Software Development Copilot
    ";

    // create the paragraph
    let ascii_par = Paragraph::new(ascii_art)
        .alignment(Alignment::Center)
        .block(Block::default())
        .wrap(Wrap { trim: true });

    // now do a vertical layout in the left chunk to center that ascii vertically
    let ascii_vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // top filler
            Constraint::Percentage(60), // bottom filler
        ])
        .split(hsplit[0]);

    // we draw ascii in the middle
    f.render_widget(ascii_par, ascii_vert[1]);

    // the menu block
    let mut menu_lines = Vec::new();
    for (i, item) in app.splash_menu_items.iter().enumerate() {
        let selected = i == app.splash_selected_idx;
        let style = if selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        menu_lines.push(Line::from(Span::styled(
            format!("{} {}", if selected { "▶" } else { " " }, item),
            style,
        )));
    }
    let menu_par = Paragraph::new(menu_lines)
        .alignment(Alignment::Center)
        .block(Block::default());

    // let's say we have exactly app.splash_menu_items.len() lines
    let menu_line_count = app.splash_menu_items.len() as u16;

    // now do a vertical layout in the right chunk to center that menu vertically
    let menu_vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),          // top filler
            Constraint::Length(menu_line_count), // actual menu lines
            Constraint::Percentage(50),          // bottom filler
        ])
        .split(hsplit[1]);

    // draw the menu in the middle
    f.render_widget(menu_par, menu_vert[1]);
}

fn draw_indexing(f: &mut Frame, app: &App) {
    let size = f.area();
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 2), // left: tree
            Constraint::Ratio(1, 2), // right: logs
        ])
        .split(size);

    let left_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(main_chunks[0]);

    // top status
    let spinner_frames = ["-", "\\", "|", "/"];
    let spin_char = spinner_frames[app.spinner_idx % spinner_frames.len()];
    let indexing_status = if app.indexing_done {
        "complete!"
    } else {
        "indexing..."
    };
    let top_line = format!(
        "status: {} {}  (files processed: {})",
        spin_char, indexing_status, app.indexing_count
    );
    let top_para = Paragraph::new(top_line)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .title(" indexing status ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .alignment(Alignment::Left);
    f.render_widget(top_para, left_split[0]);

    // middle: show tree nodes
    let mut lines = Vec::new();
    for (i, node) in app.tree.iter().enumerate() {
        let bar_len: usize = 20;
        let filled = (node.progress * bar_len as f32).round() as usize;
        let filled_str = "#".repeat(filled);
        let empty_str = " ".repeat(bar_len.saturating_sub(filled));
        let bar = format!("[{}{}]", filled_str, empty_str);
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
                .title(" indexing files ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(tree_para, left_split[1]);

    // bottom overall progress
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
    let final_bar = format!("[{}{}]", "#".repeat(filled), " ".repeat(empty));
    let bot_line = format!("overall progress: {:.1}%  {}", overall * 100.0, final_bar);
    let bot_para = Paragraph::new(bot_line)
        .block(
            Block::default()
                .title(" overall progress ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .alignment(Alignment::Left);
    f.render_widget(bot_para, left_split[2]);
}

fn draw_chat(f: &mut Frame, app: &App) {
    let size = f.area();
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Ratio(4, 5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(size);

    // chat msgs
    let chat_block = Block::default()
        .title(" chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    f.render_widget(chat_block.clone(), main_chunks[0]);
    let chat_area = chat_block.inner(main_chunks[0]);

    let mut lines = Vec::new();
    for (msg, from_user) in &app.chat_messages {
        let prefix = if *from_user { "user: " } else { "assistant: " };
        let color = if *from_user {
            Color::Yellow
        } else {
            Color::Green
        };
        lines.push(Line::from(vec![
            Span::styled(
                prefix,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(msg),
        ]));
    }
    let msgs_para = Paragraph::new(lines).wrap(Wrap { trim: true });
    f.render_widget(msgs_para, chat_area);

    // chat status
    let thinking_line = if app.chat_thinking {
        "assistant is thinking..."
    } else {
        ""
    };
    let status_para = Paragraph::new(thinking_line)
        .style(Style::default().fg(Color::Blue))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" assistant status ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        );
    f.render_widget(status_para, main_chunks[1]);

    // chat input
    let input_block = Block::default()
        .title(" input ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::White));
    f.render_widget(input_block.clone(), main_chunks[2]);
    let input_area = input_block.inner(main_chunks[2]);

    let inp_para = Paragraph::new(app.chat_input.as_str())
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: true });
    f.render_widget(inp_para, input_area);

    let cursor_x = input_area.x + app.chat_input.len() as u16 + 1;
    let cursor_y = input_area.y;
    f.set_cursor_position((cursor_x, cursor_y));

    // logs if visible overlay
    if app.logs.visible {
        let logs_rect = Rect {
            x: size.width.saturating_sub(size.width / 3),
            y: 0,
            width: size.width / 3,
            height: size.height,
        };
        draw_logs_panel(f, &app.logs, logs_rect);
    }
}

// logs

fn draw_logs_panel(f: &mut Frame, logs: &LogPanel, area: Rect) {
    let logs_block = Block::default()
        .title(" logs ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green));
    // do .inner before we move logs_block
    let inner = logs_block.inner(area);
    f.render_widget(logs_block, area);

    let mut lines = Vec::new();
    for l in &logs.entries {
        lines.push(Line::from(vec![Span::raw(l)]));
    }
    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);
    f.render_widget(para, inner);
}

// helper
async fn indexing_task(app: Arc<Mutex<App>>) {
    {
        let mut guard = app.lock().await;
        guard.logs.add("starting codebase indexing...");
        guard.tree = vec![
            TreeNode {
                filename: "src/main.rs".into(),
                progress: 0.0,
                status: "pending".into(),
            },
            TreeNode {
                filename: "src/lib.rs".into(),
                progress: 0.0,
                status: "pending".into(),
            },
            TreeNode {
                filename: "Cargo.toml".into(),
                progress: 0.0,
                status: "pending".into(),
            },
        ];
    }

    for idx in 0..3 {
        {
            let mut guard = app.lock().await;
            let node = guard.tree.get_mut(idx).unwrap();
            node.status = "indexing".into();
            let fname = node.filename.clone();
            guard.logs.add(format!("indexing {fname}"));
        }
        for _ in 0..10 {
            {
                let mut guard = app.lock().await;
                let node = guard.tree.get_mut(idx).unwrap();
                node.progress += 0.1;
            }
            sleep(Duration::from_millis(150)).await;
        }
        {
            let mut guard = app.lock().await;
            let node = guard.tree.get_mut(idx).unwrap();
            node.status = "done".into();
            guard.indexing_count += 1;
        }
    }

    {
        let mut guard = app.lock().await;
        guard.indexing_done = true;
        guard.logs.add("indexing complete!");
        guard.screen = AppScreen::Chat;
    }
}

async fn simulate_chat_response(app: Arc<Mutex<App>>, user_input: String) {
    {
        let mut guard = app.lock().await;
        guard.chat_thinking = true;
        guard
            .logs
            .add(format!("assistant thinking about '{user_input}'..."));
    }
    sleep(Duration::from_secs(2)).await;
    {
        let mut guard = app.lock().await;
        guard.chat_thinking = false;
        guard
            .chat_messages
            .push((format!("response to '{user_input}'"), false));
        guard
            .logs
            .add(format!("assistant responded to '{user_input}'"));
    }
}

// center ascii in chunk
fn center_in_rect(chunk: Rect, _ascii: &str) -> Rect {
    // just return chunk for now or do something fancier
    chunk
}

// -- main

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = Arc::new(Mutex::new(App::new()));

    'outer: loop {
        {
            // draw
            let mut guard = app.lock().await;
            guard.spinner_idx = guard.spinner_idx.wrapping_add(1);
            terminal.draw(|f| {
                draw_ui(f, &guard);
            })?;
        }
        // input
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(ke) => {
                    let mut guard = app.lock().await;
                    match guard.screen {
                        AppScreen::Splash => match (ke.modifiers, ke.code) {
                            (KeyModifiers::NONE, KeyCode::Down) => {
                                guard.splash_selected_idx =
                                    (guard.splash_selected_idx + 1) % guard.splash_menu_items.len();
                            }
                            (KeyModifiers::NONE, KeyCode::Up) => {
                                guard.splash_selected_idx = if guard.splash_selected_idx == 0 {
                                    guard.splash_menu_items.len() - 1
                                } else {
                                    guard.splash_selected_idx - 1
                                };
                            }
                            (KeyModifiers::NONE, KeyCode::Enter) => {
                                let selected = guard.splash_menu_items[guard.splash_selected_idx];
                                if selected == "quit" {
                                    break 'outer;
                                } else {
                                    guard.screen = AppScreen::Indexing;
                                    let clone = app.clone();
                                    drop(guard);
                                    tokio::spawn(async move {
                                        indexing_task(clone).await;
                                    });
                                }
                            }
                            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                                break 'outer;
                            }
                            _ => {}
                        },
                        AppScreen::Indexing => match (ke.modifiers, ke.code) {
                            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                                break 'outer;
                            }
                            (KeyModifiers::NONE, KeyCode::Char('l')) => {
                                guard.logs.visible = !guard.logs.visible;
                            }
                            _ => {}
                        },
                        AppScreen::Chat => match (ke.modifiers, ke.code) {
                            (KeyModifiers::CONTROL, KeyCode::Char('c')) => {
                                break 'outer;
                            }
                            (KeyModifiers::NONE, KeyCode::Char('l')) => {
                                guard.logs.visible = !guard.logs.visible;
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
                                } else {
                                    guard.chat_input.clear();
                                }
                            }
                            (KeyModifiers::NONE, KeyCode::Backspace) => {
                                guard.chat_input.pop();
                            }
                            (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(c)) => {
                                guard.chat_input.push(c);
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
