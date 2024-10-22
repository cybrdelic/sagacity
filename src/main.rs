// src/main.rs

use chrono::{DateTime, Utc};
use clipboard::{ClipboardContext, ClipboardProvider};
use colored::{ColoredString, Colorize};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use indicatif::{ProgressBar, ProgressStyle};
use open;
use prettytable::{Cell, Row, Table};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use reqwest::header::{ACCEPT, USER_AGENT};
use rustyline::{
    completion::{Completer, FilenameCompleter, Pair},
    highlight::Highlighter,
    hint::Hinter,
    history::DefaultHistory,
    validate::Validator,
    Context as RLContext, Editor,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use shellexpand;
use skim::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    fs::File,
    io::{self, Write},
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use textwrap;
use tokio::{select, sync::mpsc, task, time::sleep};

// Constants
const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_MODEL: &str = "claude-3-sonnet-20240229";
const DEFAULT_MAX_TOKENS: usize = 4000;

// Debug macro for easier logging
macro_rules! debug_print {
    ($($arg:tt)*) => {
        eprintln!("[DEBUG] {}", format!($($arg)*));
    };
}

// Struct to log API calls
#[derive(Debug, Clone)]
struct ApiCallLog {
    timestamp: DateTime<Utc>,
    endpoint: String,
    request_summary: String,
    response_status: u16,
    response_time_ms: u128,
}

// Struct for indexing cache
#[derive(Serialize, Deserialize)]
struct IndexCache {
    timestamp: u64,
    last_modification: u64,
    index: HashMap<String, (String, String)>,
    file_mod_times: HashMap<String, u64>,
}

// Struct for messages
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    timestamp: DateTime<Utc>,
}

// Struct for conversation sessions
#[derive(Debug, Clone)]
struct ConversationSession {
    name: String,
    index: HashMap<String, (String, String)>,
    memory: Vec<Message>,
}

// Enum for token categories
enum TokenCategory {
    Input,
    CacheWrite,
    CacheHit,
    Output,
}

// Struct for cost rates
#[derive(Debug, Clone)]
struct CostRates {
    input: f64,       // $ per million tokens
    cache_write: f64, // $ per million tokens
    cache_hit: f64,   // $ per million tokens
    output: f64,      // $ per million tokens
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

// Chatbot struct
#[derive(Clone, Debug)]
struct Chatbot {
    index: HashMap<String, (String, String)>,
    api_key: String,
    memory: Vec<Message>,
    sessions: Vec<ConversationSession>,
    current_session: Option<usize>,
    api_call_logs: Vec<ApiCallLog>,
    file_mod_times: HashMap<String, u64>,

    // Token tracking fields
    input_tokens: usize,
    cache_write_tokens: usize,
    cache_hit_tokens: usize,
    output_tokens: usize,

    // Cost tracking fields
    input_cost: f64,
    cache_write_cost: f64,
    cache_hit_cost: f64,
    output_cost: f64,

    // Cost rates based on model
    cost_rates: CostRates,
}

impl Chatbot {
    fn new(
        index: HashMap<String, (String, String)>,
        file_mod_times: HashMap<String, u64>,
        api_key: String,
    ) -> Self {
        Chatbot {
            index,
            api_key,
            memory: Vec::new(),
            sessions: Vec::new(),
            current_session: None,
            api_call_logs: Vec::new(),
            file_mod_times,

            // Initialize token counts and costs
            input_tokens: 0,
            cache_write_tokens: 0,
            cache_hit_tokens: 0,
            output_tokens: 0,
            input_cost: 0.0,
            cache_write_cost: 0.0,
            cache_hit_cost: 0.0,
            output_cost: 0.0,

            // Initialize cost rates
            cost_rates: CostRates::get_rates(),
        }
    }

    /// Update tokens and calculate costs based on the category
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

    /// Calculate total tokens and total cost
    fn total_tokens(&self) -> usize {
        self.input_tokens + self.cache_write_tokens + self.cache_hit_tokens + self.output_tokens
    }

    fn total_cost(&self) -> f64 {
        self.input_cost + self.cache_write_cost + self.cache_hit_cost + self.output_cost
    }

    /// Chat function to process user queries
    async fn chat(&mut self, user_query: &str) -> Result<String, Box<dyn Error>> {
        debug_print!("Starting chat with system");

        // Step 1: Find relevant files
        let index_clone = self.index.clone();
        let api_key_clone = self.api_key.clone();
        let relevant_files = search_index(&index_clone, user_query, &api_key_clone, self).await?;

        // Step 2: Extract file paths and languages from relevant_files with proper handling
        let relevant_file_info: Vec<(String, String)> = relevant_files
            .into_iter()
            .filter_map(|(file, _)| {
                match self.index.get(&file) {
                    Some((_, language)) => Some((file.clone(), language.clone())),
                    None => {
                        debug_print!("Warning: File '{}' not found in index.", file);
                        None // Skip files not found in the index
                    }
                }
            })
            .collect();

        // Check if we have any relevant files after filtering
        if relevant_file_info.is_empty() {
            return Err("No relevant files found in the index for the given query.".into());
        }

        // Step 3: Prepare context for the LLM
        let context = prepare_context(&relevant_file_info, user_query)?;

        // Tokenize the context and update input tokens
        let context_tokens = count_tokens(&context)?;
        self.update_tokens(TokenCategory::Input, context_tokens);
        debug_print!("Context tokens: {}", context_tokens);

        // Step 4: Generate response using the LLM
        let api_key_clone = self.api_key.clone();
        let memory_clone = self.memory.clone();
        let (response, _) =
            generate_llm_response(&context, &api_key_clone, &memory_clone, user_query, self)
                .await?;

        // Step 5: Update conversation history
        self.memory.push(Message {
            role: "user".to_string(),
            content: user_query.to_string(),
            timestamp: Utc::now(),
        });
        self.memory.push(Message {
            role: "assistant".to_string(),
            content: response.clone(),
            timestamp: Utc::now(),
        });

        Ok(response)
    }
}

// Struct for Skim item
struct SkimItemWrapper(String);

impl SkimItem for SkimItemWrapper {
    fn text(&self) -> &str {
        &self.0
    }
    fn display(&self, _context: &SkimContext) -> AnsiString {
        AnsiString::parse(&self.0)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    clear_screen();

    // Call the codebase selection menu
    let selected_codebase = codebase_selection_menu().await?;
    println!("Selected codebase: {:?}", selected_codebase);

    // Select model
    let models = vec![
        "Claude 3.5 Sonnet",
        "Claude 3 Opus",
        "Claude 3 Sonnet",
        "Claude 3 Haiku",
    ];
    let model_selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a Claude Model Tier")
        .default(0)
        .items(&models)
        .interact()?;

    let selected_model = models[model_selection];
    println!("Selected model: {}", selected_model);

    // Initialize the selected codebase
    let root_dir = selected_codebase.to_str().unwrap_or(".");
    let api_key = get_claude_api_key()?;
    let chatbot = initialize_codebase_index(root_dir, &api_key, selected_model).await?;

    // Launch the UI
    run_ui(chatbot).await?;

    Ok(())
}

/// Function to clear the terminal screen
fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}

/// Function to get the Claude API key from .zshrc
fn get_claude_api_key() -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Getting Claude API key");
    let home_dir = env::var("HOME")?;
    let zshrc_path = format!("{}/.zshrc", home_dir);
    debug_print!("Reading .zshrc from: {}", zshrc_path);
    let zshrc_content =
        fs::read_to_string(&zshrc_path).map_err(|e| format!("Failed to read .zshrc: {}", e))?;

    for line in zshrc_content.lines() {
        if line.starts_with("export ANTHROPIC_API_KEY=") {
            let key = line
                .split('=')
                .nth(1)
                .ok_or("Invalid ANTHROPIC_API_KEY format")?
                .trim_matches('"')
                .to_string();
            debug_print!("API key found");
            return Ok(key);
        }
    }

    Err("ANTHROPIC_API_KEY not found in .zshrc".into())
}

/// Function to scan the codebase for relevant files
fn scan_codebase(root_dir: &str) -> Vec<String> {
    let walker = WalkBuilder::new(root_dir)
        .hidden(false)
        .ignore(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    walker
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map_or(false, |ft| ft.is_file()))
        .filter(|entry| {
            let extension = entry.path().extension().and_then(|e| e.to_str());
            matches!(
                extension,
                Some("rs") | Some("toml") | Some("md") | Some("py") | Some("go")
            )
        })
        .map(|entry| entry.path().to_string_lossy().to_string())
        .collect()
}

/// Function to read file contents
fn read_file_contents(file_path: &str) -> Result<String, std::io::Error> {
    fs::read_to_string(file_path)
}

/// Function to summarize content with Claude API
async fn summarize_with_claude(
    content: &str,
    api_key: &str,
    language: &str,
    chatbot: &mut Chatbot,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Summarizing content with Claude");
    let client = reqwest::Client::new();
    let prompt = format!(
        "Provide a very concise summary (2-3 sentences max) of the following {} code, focusing on its main purpose and key functionalities:\n\n{}",
        language, content
    );

    // Tokenize the prompt and update input tokens
    let prompt_tokens = count_tokens(&prompt)?;
    chatbot.update_tokens(TokenCategory::Input, prompt_tokens);
    debug_print!("Prompt tokens: {}", prompt_tokens);

    let start_time = std::time::Instant::now();

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": DEFAULT_MODEL,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let elapsed_time = start_time.elapsed().as_millis();

    // Log the API call
    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "summarize_with_claude".to_string(),
        response_status: response.status().as_u16(),
        response_time_ms: elapsed_time,
    });

    debug_print!("Response status: {}", response.status());

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error response body: {}", e))?;
        debug_print!("Error response body: {}", error_body);
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    debug_print!(
        "Response body: {}",
        serde_json::to_string_pretty(&body).unwrap()
    );

    let summary = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_string();

    if summary.is_empty() {
        return Err("Empty summary received from Claude API".into());
    }

    debug_print!("Received summary: {}", summary);

    // Tokenize the response and update output tokens
    let response_tokens = count_tokens(&summary)?;
    chatbot.update_tokens(TokenCategory::Output, response_tokens);
    debug_print!("Response tokens: {}", response_tokens);

    Ok(summary)
}

/// Function to load index cache
fn load_index_cache() -> Result<Option<IndexCache>, Box<dyn std::error::Error>> {
    if let Ok(contents) = fs::read_to_string("index_cache.json") {
        let cache: IndexCache = serde_json::from_str(&contents)?;
        debug_print!("Index cache loaded successfully.");
        Ok(Some(cache))
    } else {
        debug_print!("No existing index cache found.");
        Ok(None)
    }
}

/// Function to save index cache
fn save_index_cache(
    index: &HashMap<String, (String, String)>,
    last_modification: u64,
    file_mod_times: &HashMap<String, u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache = IndexCache {
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        last_modification,
        index: index.clone(),
        file_mod_times: file_mod_times.clone(),
    };
    let serialized = serde_json::to_string_pretty(&cache)?;
    fs::write("index_cache.json", serialized)?;
    debug_print!("Index cache saved successfully.");
    Ok(())
}

/// Function to index the codebase
async fn index_codebase(
    root_dir: &str,
    api_key: &str,
    selected_model: &str,
) -> Result<Chatbot, Box<dyn std::error::Error>> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Indexing codebase...");

    let cache = load_index_cache()?;
    let index = cache.as_ref().map(|c| c.index.clone()).unwrap_or_default();
    let file_mod_times = cache
        .as_ref()
        .map(|c| c.file_mod_times.clone())
        .unwrap_or_default();

    let mut chatbot = Chatbot::new(index, file_mod_times, api_key.to_string());

    let (new_index, last_modification, updated_file_mod_times) =
        index_codebase_specific(root_dir, api_key, &pb, &mut chatbot).await?;

    pb.finish_with_message("Indexing completed");

    // Update chatbot's index and file_mod_times with new data
    chatbot.index = new_index;
    chatbot.file_mod_times = updated_file_mod_times;

    Ok(chatbot)
}

/// Function to index a specific codebase
async fn index_codebase_specific(
    root_dir: &str,
    api_key: &str,
    pb: &ProgressBar,
    chatbot: &mut Chatbot,
) -> Result<
    (HashMap<String, (String, String)>, u64, HashMap<String, u64>),
    Box<dyn std::error::Error>,
> {
    let mut index = chatbot.index.clone();
    let mut file_mod_times = chatbot.file_mod_times.clone();

    let walker = ignore::WalkBuilder::new(root_dir)
        .hidden(false)
        .ignore(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    let files: Vec<String> = walker
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map_or(false, |ft| ft.is_file()))
        .filter(|entry| {
            let extension = entry.path().extension().and_then(|e| e.to_str());
            matches!(
                extension,
                Some("rs") | Some("toml") | Some("md") | Some("py") | Some("go")
            )
        })
        .map(|entry| entry.path().to_string_lossy().to_string())
        .collect();

    pb.set_length(files.len() as u64);

    let mut last_modification = 0;
    let mut files_set = HashSet::new();

    for (i, file_path) in files.iter().enumerate() {
        pb.set_message(format!(
            "Processing file {}/{}: {}",
            i + 1,
            files.len(),
            file_path
        ));

        // Get the last modification time of the file
        let metadata = fs::metadata(&file_path)?;
        let modified = metadata.modified()?;
        let modified_secs = modified.duration_since(UNIX_EPOCH)?.as_secs();
        last_modification = std::cmp::max(last_modification, modified_secs);

        files_set.insert(file_path.clone());

        // Check if the file has been modified since last indexing
        let needs_reindex = match file_mod_times.get(file_path) {
            Some(&cached_mod_time) => modified_secs > cached_mod_time,
            None => true, // New file
        };

        if needs_reindex {
            debug_print!("Re-indexing file: {}", file_path);
            let content = read_file_contents(&file_path)
                .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

            let language = detect_language(&file_path);
            let summary = match summarize_with_claude(&content, api_key, &language, chatbot).await {
                Ok(summary) => summary,
                Err(e) => {
                    debug_print!("Error summarizing {}: {}", file_path, e);
                    format!(
                        "Failed to summarize. File content preview: {}",
                        &content[..std::cmp::min(content.len(), 100)]
                    )
                }
            };

            index.insert(file_path.clone(), (summary, language));
            file_mod_times.insert(file_path.clone(), modified_secs); // Update modification time
        } else {
            debug_print!("Skipping file (no changes): {}", file_path);
            // Optionally, you can track cache hits here
            // For example:
            // let cached_summary = &index[file_path].0;
            // let tokens = count_tokens(cached_summary)?;
            // chatbot.update_tokens(TokenCategory::CacheHit, tokens);
        }

        pb.inc(1);
    }

    // Remove entries for files that no longer exist
    index.retain(|file_path, _| files_set.contains(file_path));
    file_mod_times.retain(|file_path, _| files_set.contains(file_path));

    pb.finish_with_message(format!(
        "Indexing complete. Total files indexed: {}",
        index.len()
    ));

    // Save the index cache
    save_index_cache(&index, last_modification, &file_mod_times)?;

    Ok((index, last_modification, file_mod_times))
}

/// Function to detect programming language based on file extension
fn detect_language(file_path: &str) -> String {
    let extension = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match extension {
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "ts" => "typescript",
        "js" => "javascript",
        "java" => "java",
        "c" => "c",
        "cpp" => "cpp",
        _ => "unknown",
    }
    .to_string()
}

/// Function to search the index based on a query
async fn search_index(
    index: &HashMap<String, (String, String)>,
    query: &str,
    api_key: &str,
    chatbot: &mut Chatbot,
) -> Result<Vec<(String, f32)>, Box<dyn std::error::Error>> {
    let mut prompt = format!(
        "Based on the following query, score the relevance of each summary on a scale of 0 to 1:\n\nQuery: {}\n\n",
        query
    );

    for (file, (summary, _)) in index {
        prompt.push_str(&format!("Summary for {}: {}\n\n", file, summary));
    }

    prompt.push_str(
        "Provide your response in the following format:\n\n<file_path_1>,<relevance_score_1>\n<file_path_2>,<relevance_score_2>\n...\n",
    );

    let client = reqwest::Client::new();
    let start_time = Instant::now();

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": DEFAULT_MODEL,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let elapsed_time = start_time.elapsed().as_millis();

    // Log the API call
    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "search_index".to_string(),
        response_status: response.status().as_u16(),
        response_time_ms: elapsed_time,
    });

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error response body: {}", e))?;
        debug_print!("Error response body: {}", error_body);
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    let body: Value = response.json().await?;
    let response_text = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_string();

    let mut relevant_files = Vec::new();
    for line in response_text.lines() {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() == 2 {
            let file = parts[0].to_string();
            let relevance: f32 = parts[1].parse().unwrap_or(0.0);
            relevant_files.push((file, relevance));
        }
    }

    relevant_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    relevant_files.truncate(5); // Limit to top 5 most relevant files
    Ok(relevant_files)
}

/// Function to prepare context for the LLM
fn prepare_context(
    relevant_files: &[(String, String)],
    user_query: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut context = format!("User query: {}\n\nRelevant file contents:\n", user_query);
    for (file_path, _) in relevant_files {
        let file_content = read_file_contents(file_path)?;
        context.push_str(&format!(
            "File: {}\nContent:\n{}\n\n",
            file_path, file_content
        ));
    }
    Ok(context)
}

/// Function to generate LLM response using Claude API
async fn generate_llm_response(
    context: &str,
    api_key: &str,
    conversation_history: &Vec<Message>,
    user_query: &str,
    chatbot: &mut Chatbot,
) -> Result<(String, bool), Box<dyn std::error::Error>> {
    debug_print!("Generating LLM response");
    let client = reqwest::Client::new();

    let mut messages: Vec<Value> = conversation_history
        .iter()
        .map(|m| {
            json!({
                "role": m.role,
                "content": m.content
            })
        })
        .collect();

    // Add the current context and user query
    messages.push(json!({
        "role": "user",
        "content": format!(
            "Based on the following context about a codebase and our previous conversation, please answer the user's query:\n\nContext: {}\n\nUser query: {}",
            context, user_query
        )
    }));

    // Tokenize the user content and update input tokens
    let user_tokens = count_tokens(&messages.last().unwrap()["content"].as_str().unwrap())?;
    chatbot.update_tokens(TokenCategory::Input, user_tokens);
    debug_print!("User query tokens: {}", user_tokens);

    let start_time = Instant::now();

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": DEFAULT_MODEL,
            "messages": messages,
            "system": "You are an AI assistant helping with a codebase. Use the provided context and conversation history to answer questions.",
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let elapsed_time = start_time.elapsed().as_millis();

    // Log the API call
    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "generate_llm_response".to_string(),
        response_status: response.status().as_u16(),
        response_time_ms: elapsed_time,
    });

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error response body: {}", e))?;
        debug_print!("Error response body: {}", error_body);
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    let body: Value = response.json().await?;
    let answer = body["content"][0]["text"]
        .as_str()
        .ok_or_else(|| {
            debug_print!("Missing 'text' field in API response: {:?}", body);
            "Missing 'text' field in API response"
        })?
        .trim()
        .to_string();

    let is_complete = !body["stop_reason"].is_null() && body["stop_reason"] == "stop_sequence";

    debug_print!("API Response: {}", answer);

    // Tokenize the AI response and update output tokens
    let response_tokens = count_tokens(&answer)?;
    chatbot.update_tokens(TokenCategory::Output, response_tokens);
    debug_print!("AI response tokens: {}", response_tokens);

    Ok((answer, is_complete))
}

/// Function to initialize the codebase index
async fn initialize_codebase_index(
    root_dir: &str,
    api_key: &str,
    selected_model: &str,
) -> Result<Chatbot, Box<dyn std::error::Error>> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message("Indexing codebase...");

    let cache = load_index_cache()?;
    let index = cache.as_ref().map(|c| c.index.clone()).unwrap_or_default();
    let file_mod_times = cache
        .as_ref()
        .map(|c| c.file_mod_times.clone())
        .unwrap_or_default();

    let mut chatbot = Chatbot::new(index, file_mod_times, api_key.to_string());

    let (new_index, last_modification, updated_file_mod_times) =
        index_codebase_specific(root_dir, api_key, &pb, &mut chatbot).await?;

    pb.finish_with_message("Indexing completed");

    // Update chatbot's index and file_mod_times with new data
    chatbot.index = new_index;
    chatbot.file_mod_times = updated_file_mod_times;

    Ok(chatbot)
}

/// Enum for main menu options
enum MainMenuOption {
    Chat,
    BrowseIndex,
    GitHubRecommendations, // New option
    Debug,
    Help,
    Quit,
}

/// Function to pause and wait for user input
fn pause() {
    println!("\nPress Enter to continue...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}

/// Function to handle response actions (copy to clipboard, save to file, continue)
async fn handle_response_actions_simple(
    response: &str,
    api_key: &str,
    chatbot: &mut Chatbot,
) -> Result<(), Box<dyn std::error::Error>> {
    let action = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do with the response?")
        .default(0)
        .items(&["Copy to clipboard", "Save to file", "Continue"])
        .interact()?;

    match action {
        0 => copy_to_clipboard(response)?,
        1 => save_to_file(response, api_key, chatbot).await?,
        2 => {}
        _ => unreachable!(),
    }
    Ok(())
}

/// Function to chat with the system
async fn chat_mode(
    chatbot: &mut Chatbot,
    rl: &mut Editor<SkimItemWrapper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Automatically load conversation history at the start of chat mode
    if let Ok(history) = load_conversation() {
        chatbot.memory = history;
        println!("{}", "Conversation history loaded successfully.".green());
    } else {
        chatbot.memory = Vec::new();
    }

    loop {
        clear_screen();
        match display_main_menu() {
            MainMenuOption::Chat => {
                print_header("Chat with AI");
                display_chat_history(chatbot);

                let chat_query = rl.readline(&format!(
                    "{} ",
                    "Enter your question (or type '/help' for commands):"
                        .bold()
                        .cyan()
                ))?;
                let chat_query = chat_query.trim();

                match chat_query {
                    "/exit" => break,
                    "/clear" => {
                        chatbot.memory.clear();
                        println!("{}", "Conversation history cleared.".bold().green());
                        save_conversation(&chatbot.memory)?;
                        continue;
                    }
                    "/help" => {
                        display_chat_help();
                        pause();
                        continue;
                    }
                    _ => {}
                }

                // Create a ProgressBar with custom style
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.cyan} {msg}")
                        .unwrap(),
                );

                // Initialize ProgressBar with the first message
                pb.set_message("Initializing chat session...");
                pb.enable_steady_tick(Duration::from_millis(120));

                // Pass the ProgressBar to the chat function
                let response = chatbot.chat(chat_query).await?;

                pb.finish_and_clear();

                chatbot.memory.push(Message {
                    role: "user".to_string(),
                    content: chat_query.to_string(),
                    timestamp: Utc::now(),
                });
                chatbot.memory.push(Message {
                    role: "assistant".to_string(),
                    content: response.clone(),
                    timestamp: Utc::now(),
                });

                // Automatically save conversation history after each response
                save_conversation(&chatbot.memory)?;

                display_ai_response(&response);

                // Display token count and cost
                println!("{}", "---- Token Usage ----".bold().underline());
                println!("{}: {} tokens", "Input Tokens".bold(), chatbot.input_tokens);
                println!("{}: ${:.2}", "Input Cost".bold(), chatbot.input_cost);
                println!(
                    "{}: {} tokens",
                    "Cache Write Tokens".bold(),
                    chatbot.cache_write_tokens
                );
                println!(
                    "{}: ${:.2}",
                    "Cache Write Cost".bold(),
                    chatbot.cache_write_cost
                );
                println!(
                    "{}: {} tokens",
                    "Cache Hit Tokens".bold(),
                    chatbot.cache_hit_tokens
                );
                println!(
                    "{}: ${:.2}",
                    "Cache Hit Cost".bold(),
                    chatbot.cache_hit_cost
                );
                println!(
                    "{}: {} tokens",
                    "Output Tokens".bold(),
                    chatbot.output_tokens
                );
                println!("{}: ${:.2}", "Output Cost".bold(), chatbot.output_cost);
                println!(
                    "{}: {} tokens",
                    "Total Tokens".bold(),
                    chatbot.total_tokens()
                );
                println!("{}: ${:.2}", "Total Cost".bold(), chatbot.total_cost());
                println!("{}", "----------------------".bold().underline());
                println!();

                // Handle response actions without diff-related options
                let api_key_clone = chatbot.api_key.clone();
                handle_response_actions_simple(&response, &api_key_clone, chatbot).await?;
            }
            MainMenuOption::BrowseIndex => browse_index(&chatbot.index),
            MainMenuOption::GitHubRecommendations => {
                generate_github_recommendations(chatbot).await?
            }
            MainMenuOption::Debug => display_api_call_logs(&chatbot),
            MainMenuOption::Help => display_help(),
            MainMenuOption::Quit => {
                display_goodbye_message(&chatbot);
                break;
            }
        }
        pause();
    }
    Ok(())
}

/// Enum for different types of events.
enum Event {
    Input(CEvent),
    Tick,
}

/// Function to display the main menu and return the selected option
fn display_main_menu() -> MainMenuOption {
    let choices = vec![
        "Chat",
        "Browse Index",
        "GitHub Recommendations",
        "Debug",
        "Help",
        "Quit",
    ];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Main Menu")
        .default(0)
        .items(&choices)
        .interact()
        .unwrap();

    match selection {
        0 => MainMenuOption::Chat,
        1 => MainMenuOption::BrowseIndex,
        2 => MainMenuOption::GitHubRecommendations,
        3 => MainMenuOption::Debug,
        4 => MainMenuOption::Help,
        5 => MainMenuOption::Quit,
        _ => MainMenuOption::Quit,
    }
}

/// Function to display chat history
fn display_chat_history(chatbot: &Chatbot) {
    for message in chatbot.memory.iter().rev().take(5).rev() {
        let role = if message.role == "user" { "You" } else { "AI" };
        let color = if message.role == "user" {
            "blue"
        } else {
            "green"
        };
        println!("{}: {}", role.bold().color(color), message.content);
        println!();
    }
}

/// Function to display chat help
fn display_chat_help() {
    clear_screen();
    print_header("Chat Commands");
    println!("{:<15} {}", "/exit".bold(), "Return to main menu");
    println!("{:<15} {}", "/clear".bold(), "Clear conversation history");
    println!("{:<15} {}", "/help".bold(), "Display this help message");
    println!("{:<15} {}", "/save".bold(), "Save the current conversation");
    println!(
        "{:<15} {}",
        "/load".bold(),
        "Load a previously saved conversation"
    );
}

/// Function to display AI response
fn display_ai_response(response: &str) {
    println!("{}", "AI Response:".bold().green());
    for line in textwrap::wrap(response, 80) {
        println!("  {}", line);
    }
    println!();
}

/// Function to display help menu
fn display_help() {
    print_header("Help Menu");
    println!("{}", "Available Commands:".bold().yellow());
    println!("  {} {}", "/exit:".bold(), "End the chat session");
    println!(
        "  {} {}",
        "/clear:".bold(),
        "Clear the conversation history"
    );
    println!("  {} {}", "/help:".bold(), "Display this help message");
    println!("  {} {}", "/save:".bold(), "Save the current conversation");
    println!(
        "  {} {}",
        "/load:".bold(),
        "Load a previously saved conversation"
    );
    println!("  {} {}", "/last:".bold(), "Display your last message");
    println!("\n{}", "Chat Instructions:".bold().yellow());
    println!("  Type your questions normally to chat with the AI about the codebase.");
    println!("  The AI will provide information based on the indexed files and your queries.");
    println!("\n{}", "Navigation:".bold().yellow());
    println!("  Use the arrow keys to navigate through previous commands.");
    println!("  Press Enter to submit your query or command.");
    println!("\n{}", "Tips:".bold().yellow());
    println!("  - Be specific in your questions to get more accurate responses.");
    println!("  - Use '/save' regularly to keep a backup of your conversation.");
    println!("  - If you're lost, use '/clear' to start a fresh conversation.");
    println!("  - Use '/last' to review your most recent message.");
    println!();
}

/// Function to save conversation history
fn save_conversation(conversation_history: &[Message]) -> io::Result<()> {
    let json = serde_json::to_string_pretty(conversation_history).unwrap();
    fs::write("conversation_history.json", json)?;
    println!("Conversation saved successfully.");
    Ok(())
}

/// Function to load conversation history
fn load_conversation() -> io::Result<Vec<Message>> {
    let json = fs::read_to_string("conversation_history.json")?;
    let conversation_history: Vec<Message> = serde_json::from_str(&json)?;
    println!("Conversation loaded successfully.");
    Ok(conversation_history)
}

/// Function to copy text to clipboard
fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx: ClipboardContext = ClipboardProvider::new()?;
    ctx.set_contents(text.to_owned())?;
    println!("Output copied to clipboard.");
    Ok(())
}

/// Function to save text to a file
async fn save_to_file(
    text: &str,
    api_key: &str,
    chatbot: &mut Chatbot,
) -> Result<(), Box<dyn std::error::Error>> {
    let filename = generate_organized_filename(api_key, text, chatbot).await?;
    let output_dir = "ai_responses";
    fs::create_dir_all(output_dir)?;
    let file_path = format!("{}/{}", output_dir, filename);
    let mut file = File::create(&file_path)?;
    file.write_all(text.as_bytes())?;
    println!("Output saved to file: {}", file_path);
    Ok(())
}

/// Function to generate an organized filename using Claude API
async fn generate_organized_filename(
    api_key: &str,
    content: &str,
    chatbot: &mut Chatbot,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Generating organized filename");
    let client = reqwest::Client::new();

    let prompt = format!(
        "Based on the following content, generate a concise and descriptive filename (max 50 characters) that summarizes the main topic or purpose. Title it in all caps and keep it from 1 to 4 words. Include the .md extension. Only return the filename, nothing else:\n\n{}",
        content
    );

    // Tokenize the prompt
    let prompt_tokens = count_tokens(&prompt)?;
    chatbot.update_tokens(TokenCategory::Input, prompt_tokens);
    debug_print!("Prompt tokens: {}", prompt_tokens);

    let start_time = Instant::now();

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": DEFAULT_MODEL,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": 100
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let elapsed_time = start_time.elapsed().as_millis();

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    let filename = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_string();

    // Tokenize the filename and update output tokens
    let filename_tokens = count_tokens(&filename)?;
    chatbot.update_tokens(TokenCategory::Output, filename_tokens);
    debug_print!("Filename tokens: {}", filename_tokens);

    Ok(filename)
}

/// Function to present GitHub recommendations to the user
async fn generate_github_recommendations(
    chatbot: &mut Chatbot,
) -> Result<(), Box<dyn std::error::Error>> {
    // Define the path to the `.current/` directory
    let current_dir = shellexpand::tilde("~/alexf/software-projects/.current/").to_string();
    let current_path = PathBuf::from(current_dir);

    if !current_path.exists() || !current_path.is_dir() {
        println!(
            "{}",
            "The .current/ directory does not exist or is not a directory.".red()
        );
        return Ok(());
    }

    // Scan all codebases in `.current/`
    let codebases = scan_current_directory(&current_path)?;

    if codebases.is_empty() {
        println!(
            "{}",
            "No codebases found in the .current/ directory.".yellow()
        );
        return Ok(());
    }

    // Load or create index caches for each codebase
    let mut aggregated_context = String::new();
    let pb = ProgressBar::new(codebases.len() as u64);
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} [{elapsed_precise}] {msg}")
            .unwrap(),
    );
    pb.set_message("Aggregating codebase indexes...");

    for codebase in codebases {
        pb.set_message(format!("Processing: {}", codebase.display()));

        // Load or create index cache
        let index = load_or_create_index_cache(&codebase, chatbot).await?;

        // Append to aggregated context
        for (_file, (summary, _language)) in index {
            aggregated_context.push_str(&format!("{}\n", summary));
        }

        pb.inc(1);
    }

    pb.finish_with_message("Aggregation complete.");

    // Use the aggregated context to search GitHub
    println!(
        "{}",
        "Generating GitHub recommendations based on your codebases..."
            .bold()
            .cyan()
    );

    let github_repos = search_github_repos(&aggregated_context).await?;

    if github_repos.is_empty() {
        println!("{}", "No relevant GitHub repositories found.".yellow());
        return Ok(());
    }

    // Present the recommendations to the user
    present_github_recommendations(&github_repos);

    Ok(())
}

/// Function to scan `.current/` directory for codebases
fn scan_current_directory(
    current_path: &PathBuf,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut codebases = Vec::new();
    for entry in fs::read_dir(current_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            codebases.push(path);
        }
    }
    Ok(codebases)
}

/// Function to load or create index cache for a codebase
async fn load_or_create_index_cache(
    codebase_path: &PathBuf,
    chatbot: &mut Chatbot,
) -> Result<HashMap<String, (String, String)>, Box<dyn std::error::Error>> {
    let cache_path = codebase_path.join("index_cache.json");

    if cache_path.exists() {
        // Load existing cache
        let cache_content = fs::read_to_string(&cache_path)?;
        let cache: IndexCache = serde_json::from_str(&cache_content)?;
        debug_print!("Loaded index cache for {}", codebase_path.display());
        Ok(cache.index)
    } else {
        // Create new index
        let index = index_codebase_specific(codebase_path, chatbot).await?;
        // Save to cache
        let cache = IndexCache {
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            last_modification: 0, // Update as needed
            index: index.clone(),
            file_mod_times: HashMap::new(), // Update as needed
        };
        let serialized = serde_json::to_string_pretty(&cache)?;
        fs::write(&cache_path, serialized)?;
        debug_print!(
            "Created and saved new index cache for {}",
            codebase_path.display()
        );
        Ok(index)
    }
}

/// Function to index a specific codebase (for GitHub recommendations)
async fn index_codebase_specific(
    codebase_path: &PathBuf,
    api_key: &str,
    pb: &ProgressBar,
    chatbot: &mut Chatbot,
) -> Result<HashMap<String, (String, String)>, Box<dyn std::error::Error>> {
    let mut index = HashMap::new();
    let walker = ignore::WalkBuilder::new(codebase_path)
        .hidden(false)
        .ignore(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    let files: Vec<String> = walker
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map_or(false, |ft| ft.is_file()))
        .filter(|entry| {
            let extension = entry.path().extension().and_then(|e| e.to_str());
            matches!(
                extension,
                Some("rs") | Some("toml") | Some("md") | Some("py") | Some("go")
            )
        })
        .map(|entry| entry.path().to_string_lossy().to_string())
        .collect();

    pb.set_length(files.len() as u64);

    for (i, file_path) in files.iter().enumerate() {
        pb.set_message(format!(
            "Processing file {}/{}: {}",
            i + 1,
            files.len(),
            file_path
        ));

        // Read file content
        let content = fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

        // Detect language
        let language = detect_language(&file_path);

        // Summarize with Claude
        let summary = match summarize_with_claude(&content, api_key, &language, chatbot).await {
            Ok(summary) => summary,
            Err(e) => {
                debug_print!("Error summarizing {}: {}", file_path, e);
                format!(
                    "Failed to summarize. File content preview: {}",
                    &content[..std::cmp::min(content.len(), 100)]
                )
            }
        };

        index.insert(file_path.clone(), (summary, language));
        pb.inc(1);
    }

    pb.finish_with_message(format!("Indexing complete for {}", codebase_path.display()));

    Ok(index)
}

/// Function to search GitHub repositories based on aggregated context
async fn search_github_repos(
    aggregated_context: &str,
) -> Result<Vec<GitHubRepo>, Box<dyn std::error::Error>> {
    // Use the aggregated context as the search query
    // Here, we'll extract keywords from the context for a more effective search
    let keywords = extract_keywords(aggregated_context);

    if keywords.is_empty() {
        return Ok(Vec::new());
    }

    let query = keywords.join("+");

    // GitHub Search API
    let url = format!(
        "https://api.github.com/search/repositories?q={}&sort=stars&order=desc&per_page=10",
        query
    );
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header(USER_AGENT, "CodebaseExplorer")
        .header(ACCEPT, "application/vnd.github.v3+json")
        .send()
        .await?;

    if response.status() == 403 {
        return Err("GitHub API rate limit exceeded.".into());
    }

    let body: Value = response.json().await?;
    let repos: Vec<GitHubRepo> =
        serde_json::from_value(body["items"].clone()).unwrap_or(Vec::new());

    Ok(repos)
}

/// Function to extract keywords from aggregated context
fn extract_keywords(context: &str) -> Vec<String> {
    // Simple keyword extraction: split by whitespace and collect unique words longer than 4 characters
    let mut keywords = HashSet::new();
    for word in context.split_whitespace() {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric());
        if word.len() > 4 {
            keywords.insert(word.to_lowercase());
        }
    }
    keywords.into_iter().collect()
}

/// Function to present GitHub recommendations to the user
fn present_github_recommendations(repos: &[GitHubRepo]) {
    println!("{}", "\n--- GitHub Recommendations ---".bold().green());

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        Cell::new("Name").style_spec("b"),
        Cell::new("Description"),
        Cell::new("Stars"),
        Cell::new("Language"),
        Cell::new("URL"),
    ]));

    for repo in repos {
        table.add_row(Row::new(vec![
            Cell::new(&repo.full_name),
            Cell::new(repo.description.as_deref().unwrap_or("No description")),
            Cell::new(&repo.stargazers_count.to_string()),
            Cell::new(repo.language.as_deref().unwrap_or("N/A")),
            Cell::new(&repo.html_url),
        ]));
    }

    table.printstd();

    // Allow user to select a repository to clone or open in browser
    let choices: Vec<String> = repos.iter().map(|r| r.full_name.clone()).collect();
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a repository to clone or open")
        .default(0)
        .items(&choices)
        .item("Back to Main Menu")
        .interact()
        .unwrap();

    if selection < repos.len() {
        let selected_repo = &repos[selection];
        let action_choices = vec!["Clone Repository", "Open in Browser", "Back"];
        let action = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do with this repository?")
            .default(0)
            .items(&action_choices)
            .interact()
            .unwrap();

        match action {
            0 => {
                // Clone the repository
                match clone_github_repo(&selected_repo.clone_url, &selected_repo.full_name) {
                    Ok(path) => println!("Repository cloned to {:?}", path),
                    Err(e) => println!("Failed to clone repository: {}", e),
                }
            }
            1 => {
                // Open in browser
                if let Err(e) = open::that(&selected_repo.html_url) {
                    println!("Failed to open browser: {}", e);
                }
            }
            2 => {}
            _ => unreachable!(),
        }
    }
}

/// Function to list projects in the user's home directory and subdirectories
fn list_projects_in_home() -> Vec<PathBuf> {
    // Attempt to load cache
    if let Some(cache) = load_codebase_cache() {
        return cache.codebases.iter().map(|p| PathBuf::from(p)).collect();
    }

    let mut projects = Vec::new();
    if let Some(home_path) = home::home_dir() {
        // Use WalkBuilder to recursively search for directories containing source files
        let walker = WalkBuilder::new(home_path)
            .follow_links(false)
            .max_depth(Some(4)) // Set maximum depth to prevent excessive recursion
            .build();

        let source_extensions = [
            "rs", "py", "go", "js", "ts", "java", "c", "cpp", "md", "toml",
        ];

        let mut project_paths = HashSet::new();

        for entry in walker {
            if let Ok(entry) = entry {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if source_extensions.contains(&ext.to_string_lossy().as_ref()) {
                            if let Some(parent) = path.parent() {
                                // Add the parent directory as a project
                                project_paths.insert(parent.to_path_buf());
                            }
                        }
                    }
                }
            }
        }

        projects.extend(project_paths.into_iter());
    }

    // Manually add specific directories if needed
    let additional_paths = vec!["~/alexf/software-projects/.current"];
    for path_str in additional_paths {
        let expanded_path = shellexpand::tilde(path_str).into_owned();
        let path = PathBuf::from(expanded_path);
        if path.exists() && path.is_dir() {
            // For the specified .current directory, add all subdirectories
            if path_str == "~/alexf/software-projects/.current" {
                if let Ok(entries) = fs::read_dir(&path) {
                    for entry in entries.flatten() {
                        if let Ok(file_type) = entry.file_type() {
                            if file_type.is_dir() {
                                projects.push(entry.path());
                            }
                        }
                    }
                }
            } else {
                projects.push(path);
            }
        }
    }

    // Convert PathBuf to String for caching
    let codebase_strings: Vec<String> = projects
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    // Save to cache
    if let Err(e) = save_codebase_cache(&codebase_strings) {
        debug_print!("Failed to save codebase cache: {}", e);
    }

    projects
}
