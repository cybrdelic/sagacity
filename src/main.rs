// src/main.rs

mod batch_processor {
    #[derive(Debug)]
    pub struct BatchProcessor;
    impl BatchProcessor {
        pub fn new() -> Self {
            Self
        }
    }
}

mod cache {
    #[derive(Debug)]
    pub struct CodebaseCache {
        pub codebases: Vec<String>,
    }
    pub const CACHE_EXPIRY_SECS: u64 = 0;
    pub const CACHE_FILE: &str = "dummy.cache";
    pub fn load_codebase_cache() -> Option<CodebaseCache> {
        None
    }
    pub fn save_codebase_cache(_p: &[String]) -> Result<(), String> {
        Ok(())
    }
}

mod constants {
    pub const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/complete";
    pub const ANTHROPIC_VERSION: &str = "2023-06-01";
    pub const DEFAULT_MODEL: &str = "claude-v1";
    pub const DEFAULT_MAX_TOKENS: i32 = 2048;
}

mod github_recommendations {
    pub async fn generate_github_recommendations() -> Result<(), Box<dyn std::error::Error>> {
        println!("(stub) github recommendations code here");
        Ok(())
    }
}

mod selection {
    use std::path::PathBuf;
    pub async fn codebase_selection_menu() -> Result<PathBuf, Box<dyn std::error::Error>> {
        println!("(stub) selection code here");
        Ok(PathBuf::from("."))
    }
}

use crossterm::event::{Event, KeyCode, KeyModifiers};
use crossterm::execute;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::batch_processor::*;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::io::{self};
use std::time::Duration as StdDuration;
use tachyonfx::{
    fx, CellFilter, Duration as TachyonDuration, Effect, EffectTimer, Interpolation, Motion, Shader,
};
use unicode_width::UnicodeWidthStr;

#[derive(Debug)]
struct ApiCallLog {
    timestamp: DateTime<Utc>,
    endpoint: String,
    request_summary: String,
    response_status: u16,
    response_time_ms: u128,
}

#[derive(Serialize, Deserialize)]
struct IndexCache {
    timestamp: u64,
    last_modification: u64,
    index: HashMap<String, (String, String)>,
    file_mod_times: HashMap<String, u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    timestamp: DateTime<Utc>,
}

#[derive(Debug)]
struct ConversationSession {
    name: String,
    index: HashMap<String, (String, String)>,
    memory: Vec<Message>,
}

enum TokenCategory {
    Input,
    CacheWrite,
    CacheHit,
    Output,
}

#[derive(Debug)]
struct CostRates {
    input: f64,
    cache_write: f64,
    cache_hit: f64,
    output: f64,
}

impl CostRates {
    fn get_rates() -> Self {
        CostRates {
            input: 3.00,
            cache_write: 3.75,
            cache_hit: 0.30,
            output: 15.00,
        }
    }
}

#[derive(Debug)]
struct Chatbot {
    index: HashMap<String, (String, String)>,
    api_key: String,
    memory: Vec<Message>,
    sessions: Vec<ConversationSession>,
    current_session: Option<usize>,
    api_call_logs: Vec<ApiCallLog>,
    file_mod_times: HashMap<String, u64>,

    input_tokens: usize,
    cache_write_tokens: usize,
    cache_hit_tokens: usize,
    output_tokens: usize,

    input_cost: f64,
    cache_write_cost: f64,
    cache_hit_cost: f64,
    output_cost: f64,

    cost_rates: CostRates,
    batch_processor: BatchProcessor,
}

impl Chatbot {
    fn new(
        index: HashMap<String, (String, String)>,
        file_mod_times: HashMap<String, u64>,
        api_key: String,
    ) -> Self {
        Self {
            index,
            api_key,
            memory: Vec::new(),
            sessions: Vec::new(),
            current_session: None,
            api_call_logs: Vec::new(),
            file_mod_times,

            input_tokens: 0,
            cache_write_tokens: 0,
            cache_hit_tokens: 0,
            output_tokens: 0,

            input_cost: 0.0,
            cache_write_cost: 0.0,
            cache_hit_cost: 0.0,
            output_cost: 0.0,
            cost_rates: CostRates::get_rates(),
            batch_processor: BatchProcessor::new(),
        }
    }

    fn update_tokens(&mut self, category: TokenCategory, tokens: usize) {
        match category {
            TokenCategory::Input => {
                self.input_tokens += tokens;
                self.input_cost += (tokens as f64 / 1_000_000.0) * self.cost_rates.input;
            }
            TokenCategory::CacheWrite => {
                self.cache_write_tokens += tokens;
                self.cache_write_cost +=
                    (tokens as f64 / 1_000_000.0) * self.cost_rates.cache_write;
            }
            TokenCategory::CacheHit => {
                self.cache_hit_tokens += tokens;
                self.cache_hit_cost += (tokens as f64 / 1_000_000.0) * self.cost_rates.cache_hit;
            }
            TokenCategory::Output => {
                self.output_tokens += tokens;
                self.output_cost += (tokens as f64 / 1_000_000.0) * self.cost_rates.output;
            }
        }
    }

    fn total_tokens(&self) -> usize {
        self.input_tokens + self.cache_write_tokens + self.cache_hit_tokens + self.output_tokens
    }

    fn total_cost(&self) -> f64 {
        self.input_cost + self.cache_write_cost + self.cache_hit_cost + self.output_cost
    }
}

struct MessageEffect {
    message: Message,
    fade_in: Effect,
}

impl MessageEffect {
    fn new(message: Message) -> Self {
        let fade_timer =
            EffectTimer::new(StdDuration::from_millis(500).into(), Interpolation::Linear);
        let mut fade_in = fx::fade_from_fg(Color::DarkGray, fade_timer);
        fade_in.set_cell_selection(CellFilter::All);

        Self { message, fade_in }
    }

    fn render(&mut self, f: &mut ratatui::Frame, area: Rect) {
        let style_color = if self.message.role == "assistant" {
            Color::LightCyan
        } else {
            Color::LightYellow
        };

        let prefix = format!("{}: ", self.message.role);
        let content = Line::from(vec![
            Span::styled(
                prefix,
                Style::default()
                    .fg(style_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                self.message.content.clone(),
                Style::default().fg(style_color),
            ),
        ]);

        let message_block = Block::default().style(Style::default().fg(style_color));

        let paragraph = Paragraph::new(content)
            .block(message_block)
            .wrap(Wrap { trim: true });

        f.render_widget(paragraph, area);

        self.fade_in
            .execute(TachyonDuration::from_millis(16), area, f.buffer_mut());
    }
}

fn get_claude_api_key() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(k) = env::var("ANTHROPIC_API_KEY") {
        Ok(k)
    } else {
        Err("missing ANTHROPIC_API_KEY env var".into())
    }
}

async fn index_codebase_stub(
    _root: &str,
    _api_key: &str,
    _chatbot: &mut Chatbot,
) -> Result<HashMap<String, (String, String)>, Box<dyn std::error::Error>> {
    let mut map = HashMap::new();
    map.insert(
        "src/main.rs".to_string(),
        ("(dummy summary)".to_string(), "rust".to_string()),
    );
    Ok(map)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize API and chatbot
    let _api_key = get_claude_api_key().unwrap_or("dummy_key".to_string());
    let idx = index_codebase_stub(
        ".",
        &_api_key,
        &mut Chatbot::new(HashMap::new(), HashMap::new(), _api_key.clone()),
    )
    .await?;
    let mut chatbot = Chatbot::new(idx, HashMap::new(), _api_key.clone());

    // Initialize terminal
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Chat state
    let mut input = String::new();
    let mut input_mode = false;
    let mut message_effects: Vec<MessageEffect> = Vec::new();
    message_effects.push(MessageEffect::new(Message {
        role: "assistant".to_string(),
        content: "Hello! I'm Claude. How can I help you today?".to_string(),
        timestamp: Utc::now(),
    }));

    // Initialize slide effect for input box
    let slide_timer = EffectTimer::new(StdDuration::from_millis(300).into(), Interpolation::Linear);
    let mut slide_effect = fx::slide_in(
        Motion::RightToLeft,
        10, // offset_x
        0,  // offset_y
        Color::Black,
        slide_timer,
    );

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Min(3),    // Messages area
                    Constraint::Length(3), // Input area
                    Constraint::Length(1), // Status bar
                ])
                .split(f.area());

            // Messages area
            let messages_block = Block::default()
                .title("Chat History")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue));

            let messages_area = messages_block.inner(chunks[0]);
            f.render_widget(messages_block, chunks[0]);

            // Render messages with spacing
            let message_height = 3;
            for (i, msg_effect) in message_effects.iter_mut().enumerate() {
                let msg_area = Rect::new(
                    messages_area.x,
                    messages_area.y + (i as u16 * message_height),
                    messages_area.width,
                    message_height,
                );
                msg_effect.render(f, msg_area);
            }

            // Input area with mode indicator
            let input_style = if input_mode {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Blue)
            };

            let input_block = Block::default()
                .title(if input_mode {
                    "Input (Active)"
                } else {
                    "Input (Press Tab)"
                })
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(input_style);

            let input_text = Paragraph::new(input.as_str())
                .block(input_block)
                .style(Style::default().fg(Color::White))
                .wrap(Wrap { trim: true });

            f.render_widget(input_text, chunks[1]);

            // Status bar
            let status = format!(
                "Messages: {} | Tab: Toggle Input | Ctrl+C: Exit",
                message_effects.len()
            );
            let status_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray));

            let status_text = Paragraph::new(status)
                .block(status_block)
                .style(Style::default().fg(Color::Gray))
                .alignment(Alignment::Center);

            f.render_widget(status_text, chunks[2]);

            // Show cursor only in input mode
            if input_mode {
                f.set_cursor_position((chunks[1].x + 1 + input.width() as u16, chunks[1].y + 1));
            }
        })?;

        // Handle input
        if crossterm::event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = crossterm::event::read()? {
                match (key.modifiers, key.code) {
                    (KeyModifiers::CONTROL, KeyCode::Char('c')) => break,

                    (_, KeyCode::Tab) => {
                        input_mode = !input_mode;
                    }

                    (_, KeyCode::Enter) if input_mode && !input.trim().is_empty() => {
                        // Add user message
                        message_effects.push(MessageEffect::new(Message {
                            role: "user".to_string(),
                            content: input.clone(),
                            timestamp: Utc::now(),
                        }));

                        // Add assistant response
                        message_effects.push(MessageEffect::new(Message {
                            role: "assistant".to_string(),
                            content: format!("You said: {}", input),
                            timestamp: Utc::now(),
                        }));

                        input.clear();
                    }

                    (_, KeyCode::Backspace) if input_mode => {
                        input.pop();
                    }

                    (_, KeyCode::Char(c)) if input_mode => {
                        input.push(c);
                    }

                    _ => {}
                }
            }
        }
    }

    // Clean up
    crossterm::terminal::disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    Ok(())
}
