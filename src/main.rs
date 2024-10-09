use clipboard::{ClipboardContext, ClipboardProvider};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use diffy::{apply, Patch};
use indicatif::{ProgressBar, ProgressStyle};
use regex::Regex;
use reqwest;
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context, Editor};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;
use textwrap;
use walkdir::WalkDir;

const HEAVY_DOWN_AND_RIGHT: char = '┏';
const HEAVY_DOWN_AND_LEFT: char = '┓';
const HEAVY_UP_AND_RIGHT: char = '┗';
const HEAVY_UP_AND_LEFT: char = '┛';
const HEAVY_HORIZONTAL: char = '━';
const HEAVY_VERTICAL: char = '┃';

const LIGHT_DOWN_AND_RIGHT: char = '┌';
const LIGHT_DOWN_AND_LEFT: char = '┐';
const LIGHT_UP_AND_RIGHT: char = '└';
const LIGHT_UP_AND_LEFT: char = '┘';
const LIGHT_HORIZONTAL: char = '─';
const LIGHT_VERTICAL: char = '│';
const LIGHT_VERTICAL_AND_RIGHT: char = '├';
const LIGHT_VERTICAL_AND_LEFT: char = '┤';

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01"; // Add this line

// Add a debug macro for easier logging
macro_rules! debug_print {
    ($($arg:tt)*) => {
        eprintln!("[DEBUG] {}", format!($($arg)*));
    };
}

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

fn scan_codebase(root_dir: &str) -> Vec<String> {
    WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
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

fn read_file_contents(file_path: &str) -> Result<String, std::io::Error> {
    fs::read_to_string(file_path)
}

async fn summarize_with_claude(
    content: &str,
    api_key: &str,
    language: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Summarizing content with Claude");
    let client = reqwest::Client::new();
    let prompt = format!(
        "Provide a very concise summary (2-3 sentences max) of the following {} code, focusing on its main purpose and key functionalities:\n\n{}",
        language, content
    );
    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": 4000
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

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
    Ok(summary)
}

#[derive(Serialize, Deserialize)]
struct IndexCache {
    timestamp: u64,
    last_modification: u64,
    index: HashMap<String, (String, String)>,
}

fn save_index_cache(
    index: &HashMap<String, (String, String)>,
    last_modification: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache = IndexCache {
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        last_modification,
        index: index.clone(),
    };
    let serialized = serde_json::to_string(&cache)?;
    fs::write("index_cache.json", serialized)?;
    Ok(())
}

fn load_index_cache(
) -> Result<Option<(u64, u64, HashMap<String, (String, String)>)>, Box<dyn std::error::Error>> {
    if let Ok(contents) = fs::read_to_string("index_cache.json") {
        let cache: IndexCache = serde_json::from_str(&contents)?;
        Ok(Some((
            cache.timestamp,
            cache.last_modification,
            cache.index,
        )))
    } else {
        Ok(None)
    }
}

fn check_for_codebase_changes(
    root_dir: &str,
    last_modification: u64,
) -> Result<bool, Box<dyn std::error::Error>> {
    for entry in WalkDir::new(root_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let modified_secs = modified.duration_since(UNIX_EPOCH)?.as_secs();
                    if modified_secs > last_modification {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

async fn index_codebase(
    root_dir: &str,
    api_key: &str,
    pb: &ProgressBar,
) -> Result<(HashMap<String, (String, String)>, u64), Box<dyn std::error::Error>> {
    let mut index = HashMap::new();
    let files = scan_codebase(root_dir);
    pb.set_length(files.len() as u64);

    let mut last_modification = 0;

    for (i, file_path) in files.iter().enumerate() {
        pb.set_message(format!(
            "Processing file {}/{}: {}",
            i + 1,
            files.len(),
            file_path
        ));
        let content = read_file_contents(&file_path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

        // Update last_modification time
        if let Ok(metadata) = std::fs::metadata(file_path) {
            if let Ok(modified) = metadata.modified() {
                let modified_secs = modified.duration_since(UNIX_EPOCH)?.as_secs();
                last_modification = std::cmp::max(last_modification, modified_secs);
            }
        }

        let language = detect_language(&file_path);
        let summary = match summarize_with_claude(&content, api_key, &language).await {
            Ok(summary) => summary,
            Err(_e) => {
                format!(
                    "Failed to summarize. File content preview: {}",
                    &content[..std::cmp::min(content.len(), 100)]
                )
            }
        };

        index.insert(file_path.clone(), (summary, language));
        pb.inc(1);
    }

    pb.finish_with_message(format!(
        "Indexing complete. Total files indexed: {}",
        index.len()
    ));

    // Save the index cache
    save_index_cache(&index, last_modification)?;

    Ok((index, last_modification))
}

fn detect_language(file_path: &str) -> String {
    let extension = std::path::Path::new(file_path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");

    match extension {
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        _ => "unknown",
    }
    .to_string()
}

async fn search_index(
    index: &HashMap<String, (String, String)>,
    query: &str,
    api_key: &str,
) -> Vec<(String, f32)> {
    let mut relevant_files = Vec::new();
    for (file, (summary, language)) in index {
        let relevance = calculate_relevance(summary, query, language, api_key).await;
        if relevance > 0.0 {
            relevant_files.push((file.clone(), relevance));
        }
    }
    relevant_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    relevant_files.truncate(5); // Limit to top 5 most relevant files
    relevant_files
}

async fn calculate_relevance(summary: &str, query: &str, language: &str, api_key: &str) -> f32 {
    let summary_words: Vec<&str> = summary.split_whitespace().collect();
    let query_words: Vec<&str> = query.split_whitespace().collect();

    let language_keywords = match language {
        "rust" => vec!["struct", "impl", "fn", "let", "mut", "trait", "enum"],
        "python" => vec![
            "def", "class", "import", "from", "if", "elif", "else", "for", "while",
        ],
        "go" => vec![
            "func",
            "type",
            "struct",
            "interface",
            "package",
            "import",
            "var",
            "const",
        ],
        _ => vec![],
    };

    let mut keyword_relevance = 0.0;
    for query_word in &query_words {
        if summary_words.contains(query_word) {
            keyword_relevance += 1.0;
        }
        if language_keywords.contains(query_word) {
            keyword_relevance += 0.5;
        }
    }
    keyword_relevance /= query_words.len() as f32;

    let llm_relevance = get_llm_relevance_score(summary, query, api_key)
        .await
        .unwrap_or(0.0);

    // Combine keyword-based relevance and LLM-based relevance
    // You can adjust these weights based on your requirements
    const KEYWORD_WEIGHT: f32 = 0.3;
    const LLM_WEIGHT: f32 = 0.7;

    (keyword_relevance * KEYWORD_WEIGHT) + (llm_relevance * LLM_WEIGHT)
}

async fn check_intent_to_change(
    query: &str,
    api_key: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let prompt = format!(
        "Does the following query indicate an intent to change or modify code? Answer with 'yes' or 'no'.\n\nQuery: {}",
        query
    );
    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": 100
        }))
        .send()
        .await?;

    let body: Value = response.json().await?;
    let answer = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_lowercase();

    Ok(answer == "yes")
}

fn apply_diff(file_path: &str, diff: &str) -> Result<(), Box<dyn std::error::Error>> {
    let original_content = fs::read_to_string(file_path)?;
    let patch = Patch::from_str(diff)?;
    let new_content = apply(&original_content, &patch)?;
    fs::write(file_path, new_content)?;
    Ok(())
}

async fn get_llm_relevance_score(
    summary: &str,
    query: &str,
    api_key: &str,
) -> Result<f32, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let prompt = format!(
        "On a scale of 0 to 1, how relevant is the following summary to the given query? Provide only a number as the answer.\n\nSummary: {}\n\nQuery: {}",
        summary, query
    );
    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": 100
        }))
        .send()
        .await?;

    let body: Value = response.json().await?;
    let score_str = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim();

    score_str.parse::<f32>().map_err(|e| e.into())
}

use chrono::{DateTime, Utc};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    timestamp: DateTime<Utc>,
}

struct ConversationSession {
    name: String,
    index: HashMap<String, (String, String)>,
    memory: Vec<Message>,
}

struct Chatbot {
    index: HashMap<String, (String, String)>,
    api_key: String,
    memory: Vec<Message>,
    sessions: Vec<ConversationSession>,
    current_session: Option<usize>,
}

impl Chatbot {
    fn new(index: HashMap<String, (String, String)>, api_key: String) -> Self {
        Chatbot {
            index,
            api_key,
            memory: Vec::new(),
            sessions: Vec::new(),
            current_session: None,
        }
    }

    fn create_session(&mut self, name: String, index: HashMap<String, (String, String)>) {
        let session = ConversationSession {
            name,
            index,
            memory: Vec::new(),
        };
        self.sessions.push(session);
        self.current_session = Some(self.sessions.len() - 1);
    }

    fn switch_session(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.current_session = Some(index);
            self.index = self.sessions[index].index.clone();
            self.memory = self.sessions[index].memory.clone();
        }
    }

    fn get_current_session(&self) -> Option<&ConversationSession> {
        self.current_session.map(|index| &self.sessions[index])
    }

    fn get_current_session_mut(&mut self) -> Option<&mut ConversationSession> {
        self.current_session
            .map(move |index| &mut self.sessions[index])
    }

    fn get_last_user_message(&self) -> Option<&Message> {
        self.memory.iter().rev().find(|m| m.role == "user")
    }

    async fn chat(&mut self, user_query: &str) -> Result<String, Box<dyn std::error::Error>> {
        debug_print!("Starting chat with system");

        // Step 1: Find relevant files
        let relevant_files = search_index(&self.index, user_query, &self.api_key).await;

        // Step 2: Extract file paths and languages from relevant_files
        let relevant_file_info: Vec<(String, String)> = relevant_files
            .into_iter()
            .map(|(file, _)| {
                let (_, language) = self.index.get(&file).unwrap();
                (file, language.clone())
            })
            .collect();

        // Step 3: Prepare context for the LLM
        let context = prepare_context(&relevant_file_info, user_query)?;

        // Step 4: Generate response using the LLM
        let (response, _) =
            generate_llm_response(&context, &self.api_key, &self.memory, user_query).await?;

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

async fn chat_with_system(
    chatbot: &mut Chatbot,
    user_query: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    chatbot.chat(user_query).await
}

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

async fn generate_llm_response(
    context: &str,
    api_key: &str,
    conversation_history: &Vec<Message>,
    user_query: &str,
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
        "content": format!("Based on the following context about a codebase and our previous conversation, please answer the user's query:\n\nContext: {}\n\nUser query: {}", context, user_query)
    }));

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": messages,
            "system": "You are an AI assistant helping with a codebase. Use the provided context and conversation history to answer questions. In all your responses, keep a cool and chill vibe that is cracked and overpowered in terms of technical ability and aptitude. You are personable but not in a creepy fake way.",
            "max_tokens": 4000
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    debug_print!("API Response: {:?}", body);

    let answer = body["content"][0]["text"]
        .as_str()
        .ok_or_else(|| {
            debug_print!("Missing 'text' field in API response: {:?}", body);
            "Missing 'text' field in API response"
        })?
        .trim()
        .to_string();

    let is_complete = !body["stop_reason"].is_null() && body["stop_reason"] == "stop_sequence";

    Ok((answer, is_complete))
}

async fn generate_organized_filename(
    api_key: &str,
    content: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Generating organized filename");
    let client = reqwest::Client::new();

    let prompt = format!(
        "Based on the following content, generate a concise and descriptive filename (max 50 characters) that summarizes the main topic or purpose. Include the .txt extension. Only return the filename, nothing else:\n\n{}",
        content
    );

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
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

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    let filename = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_string();

    Ok(filename)
}

use rustyline::Helper;

struct MyHelper {
    completer: FilenameCompleter,
}

impl MyHelper {
    fn new(completer: FilenameCompleter) -> Self {
        MyHelper { completer }
    }
}

impl Highlighter for MyHelper {}
impl Validator for MyHelper {}
impl Hinter for MyHelper {
    type Hint = String;

    fn hint(&self, _line: &str, _pos: usize, _ctx: &Context<'_>) -> Option<Self::Hint> {
        None
    }
}

impl Completer for MyHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        self.completer.complete(line, pos, ctx)
    }
}

impl Helper for MyHelper {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    clear_screen();
    display_welcome_screen();

    let root_dir = "."; // Current directory
    let api_key = get_claude_api_key()?;

    let index = initialize_codebase_index(root_dir, &api_key).await?;

    let mut rl = Editor::<MyHelper, DefaultHistory>::new()?;
    rl.set_helper(Some(MyHelper::new(FilenameCompleter::new())));

    let mut chatbot = Chatbot::new(index.clone(), api_key.to_string());
    chatbot.create_session("Default".to_string(), index);

    loop {
        clear_screen();
        match display_main_menu() {
            MainMenuOption::Chat => chat_mode(&mut chatbot, &mut rl).await?,
            MainMenuOption::CodeEdit => code_edit_mode(&mut chatbot, &mut rl).await?,
            MainMenuOption::BrowseIndex => browse_index(&chatbot.index),
            MainMenuOption::ManageSessions => manage_sessions(&mut chatbot, &mut rl).await?,
            MainMenuOption::Help => display_help(),
            MainMenuOption::Quit => {
                display_goodbye_message();
                break;
            }
        }
        pause();
    }

    Ok(())
}

fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}

fn display_welcome_screen() {
    println!("{}", "Welcome to Codebase Explorer".bold().cyan());
    println!("{}", "Your intelligent coding companion".italic());
    println!("\nInitializing...");
}

async fn initialize_codebase_index(
    root_dir: &str,
    api_key: &str,
) -> Result<HashMap<String, (String, String)>, Box<dyn std::error::Error>> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Indexing codebase...");

    let (index, _) = if let Some((cache_timestamp, last_mod, cached_index)) = load_index_cache()? {
        let current_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        if current_time - cache_timestamp < 3600 && !check_for_codebase_changes(root_dir, last_mod)?
        {
            (cached_index, last_mod)
        } else {
            index_codebase(root_dir, api_key, &pb).await?
        }
    } else {
        index_codebase(root_dir, api_key, &pb).await?
    };

    pb.finish_with_message("Indexing completed");
    Ok(index)
}

enum MainMenuOption {
    Chat,
    CodeEdit,
    BrowseIndex,
    ManageSessions,
    Help,
    Quit,
}

fn display_main_menu() -> MainMenuOption {
    let choices = vec![
        "Chat with AI",
        "Code Edit Mode",
        "Browse Index",
        "Manage Sessions",
        "Help",
        "Quit",
    ];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do?")
        .default(0)
        .items(&choices)
        .interact()
        .unwrap();

    match selection {
        0 => MainMenuOption::Chat,
        1 => MainMenuOption::CodeEdit,
        2 => MainMenuOption::BrowseIndex,
        3 => MainMenuOption::ManageSessions,
        4 => MainMenuOption::Help,
        5 => MainMenuOption::Quit,
        _ => unreachable!(),
    }
}

fn pause() {
    println!("\nPress Enter to continue...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}

fn display_goodbye_message() {
    clear_screen();
    println!("{}", "Thank you for using Codebase Explorer".bold().green());
    println!("Have a great day!");
}

async fn manage_sessions(
    chatbot: &mut Chatbot,
    rl: &mut Editor<MyHelper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        clear_screen();
        print_header("Manage Sessions");

        let mut choices = vec!["Create new session", "Return to main menu"];
        for session in &chatbot.sessions {
            choices.push(&session.name);
        }

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a session or action")
            .default(0)
            .items(&choices)
            .interact()?;

        if selection == 0 {
            create_new_session(chatbot, rl).await?;
        } else if selection == 1 {
            break;
        } else {
            chatbot.switch_session(selection - 2);
            println!(
                "{}",
                format!(
                    "Switched to session: {}",
                    chatbot.get_current_session().unwrap().name
                )
                .green()
            );
            pause();
        }
    }
    Ok(())
}

async fn create_new_session(
    chatbot: &mut Chatbot,
    rl: &mut Editor<MyHelper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    let name = rl.readline("Enter a name for the new session: ")?;
    let root_dir = rl.readline("Enter the root directory for the new session: ")?;

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message("Indexing codebase for new session...");

    let (new_index, _) = index_codebase(&root_dir, &chatbot.api_key, &pb).await?;
    chatbot.create_session(name.trim().to_string(), new_index);

    println!("{}", "New session created and switched to.".green());
    pause();
    Ok(())
}

fn detect_file_change_request(query: &str) -> bool {
    let patterns = [
        r"(?i)change|modify|update|edit|alter",
        r"(?i)file|code|implementation|function",
    ];
    patterns
        .iter()
        .all(|pattern| Regex::new(pattern).unwrap().is_match(query))
}

fn generate_diff(original: &str, modified: &str) -> String {
    use diffy::create_patch;
    use diffy::PatchFormatter;

    let patch = create_patch(original, modified);
    let patch_str = format!("{}", PatchFormatter::new().fmt_patch(&patch));
    patch_str
}

fn display_colorized_diff(diff: &str) {
    for line in diff.lines() {
        if line.starts_with('+') && !line.starts_with("+++ ") {
            println!("{}", line.green().bold());
        } else if line.starts_with('-') && !line.starts_with("--- ") {
            println!("{}", line.red().bold());
        } else if line.starts_with("@@") {
            println!("{}", line.yellow().bold());
        } else {
            println!("{}", line.normal());
        }
    }
}

fn apply_changes(file_path: &str, diff: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "{}",
        format!("Applying changes to {}...", file_path)
            .bold()
            .green()
    );
    let original_content = fs::read_to_string(file_path)?;
    let patch = Patch::from_str(diff)?;
    let new_content = diffy::apply(&original_content, &patch)?;
    fs::write(file_path, new_content)?;
    println!("{}", "Changes applied successfully.".bold().green());
    Ok(())
}
fn open_in_diff_application(file_path: &str, diff: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "{}",
        "Opening diff in external application...".bold().yellow()
    );
    open_diff_in_external_app(diff)?;

    let apply = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Do you want to apply the changes?")
        .default(0)
        .items(&["Yes", "No"])
        .interact()?;

    if apply == 0 {
        apply_changes(file_path, diff)?;
    } else {
        println!(
            "{}",
            format!("Changes discarded for {}", file_path)
                .bold()
                .yellow()
        );
    }

    Ok(())
}

fn view_full_diff(diff: &str) {
    clear_screen();
    print_header("Full Diff View");
    display_colorized_diff(diff);
    pause();
}

fn open_diff_in_external_app(diff: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut temp_file = NamedTempFile::new()?;
    write!(temp_file, "{}", diff)?;

    #[cfg(target_os = "macos")]
    let diff_command = "opendiff";
    #[cfg(target_os = "linux")]
    let diff_command = "meld";
    #[cfg(target_os = "windows")]
    let diff_command = "winmerge";

    Command::new(diff_command).arg(temp_file.path()).status()?;

    Ok(())
}
async fn code_edit_mode(
    chatbot: &mut Chatbot,
    rl: &mut Editor<MyHelper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        clear_screen();
        print_header("Code Edit Mode");
        let file_path =
            rl.readline("Enter the file path you want to edit (or type '/exit' to return): ")?;
        if file_path.trim() == "/exit" {
            break;
        }

        if !Path::new(&file_path).exists() {
            println!("{}", "File does not exist.".red());
            pause();
            continue;
        }

        let original_content = fs::read_to_string(&file_path)?;
        let editor = rl.readline("Enter your code editor command (e.g., 'nano', 'vim'): ")?;
        Command::new(editor.trim())
            .arg(&file_path)
            .status()
            .expect("Failed to open editor");

        let modified_content = fs::read_to_string(&file_path)?;
        if original_content != modified_content {
            let diff = generate_diff(&original_content, &modified_content);
            println!("Diff for {}:", file_path.bold());
            display_colorized_diff(&diff);
            println!(); // Add a newline for better readability

            let confirm = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Do you want to keep the changes?")
                .default(0)
                .items(&["Yes", "No"])
                .interact()?;

            if confirm == 0 {
                apply_changes(&file_path, &diff)?;
            } else {
                fs::write(&file_path, original_content)?;
                println!("{}", "Changes discarded.".yellow());
            }
            pause();
        } else {
            println!("{}", "No changes detected.".yellow());
            pause();
        }
    }
    Ok(())
}

async fn chat_mode(
    chatbot: &mut Chatbot,
    rl: &mut Editor<MyHelper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        clear_screen();
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
                continue;
            }
            "/help" => {
                display_chat_help();
                pause();
                continue;
            }
            "/save" => {
                save_conversation(&chatbot.memory)?;
                continue;
            }
            "/load" => {
                chatbot.memory = load_conversation()?;
                continue;
            }
            _ => {}
        }

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message("AI is analyzing your request...");
        pb.enable_steady_tick(std::time::Duration::from_millis(120));

        let response = chat_with_system(chatbot, chat_query).await?;
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

        display_ai_response(&response);

        // Extract file paths and changes from the AI response
        let changes = extract_changes_from_response(&response);

        if !changes.is_empty() {
            println!(
                "{}",
                "The AI has proposed changes to the project files."
                    .bold()
                    .yellow()
            );

            for (file_path, original, modified) in changes {
                println!(
                    "{}",
                    format!("Proposed changes for file: {}", file_path)
                        .bold()
                        .cyan()
                );
                let diff = generate_diff(&original, &modified);
                println!("Diff for {}:", file_path.bold());
                display_colorized_diff(&diff);
                println!(); // Add a newline for better readability

                let confirm = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("What would you like to do with these changes?")
                    .default(0)
                    .items(&[
                        "Apply changes",
                        "Discard changes",
                        "Open in diff application",
                        "View full diff",
                    ])
                    .interact()?;

                match confirm {
                    0 => {
                        apply_changes(&file_path, &diff)?;
                    }
                    1 => {
                        println!(
                            "{}",
                            format!("Changes discarded for {}", file_path)
                                .bold()
                                .yellow()
                        );
                    }
                    2 => {
                        open_in_diff_application(&file_path, &diff)?;
                        // After viewing in diff application, ask if user wants to apply changes
                        let apply = Select::with_theme(&ColorfulTheme::default())
                            .with_prompt("Do you want to apply the changes?")
                            .default(0)
                            .items(&["Yes", "No"])
                            .interact()?;
                        if apply == 0 {
                            apply_changes(&file_path, &diff)?;
                        } else {
                            println!(
                                "{}",
                                format!("Changes discarded for {}", file_path)
                                    .bold()
                                    .yellow()
                            );
                        }
                    }
                    3 => {
                        view_full_diff(&diff);
                        // After viewing the full diff, ask again what to do
                        continue;
                    }
                    _ => unreachable!(),
                }
            }
        } else {
            if let Err(e) = handle_response_actions(&response, &chatbot.api_key).await {
                eprintln!("Error: {}", e);
            }
        }
    }
    Ok(())
}

fn extract_changes_from_response(response: &str) -> Vec<(String, String, String)> {
    let mut changes = Vec::new();
    let lines: Vec<&str> = response.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].ends_with(".rs") || lines[i].ends_with(".toml") {
            let file_path = lines[i].to_string();
            i += 1;
            let mut original = String::new();
            let mut modified = String::new();

            while i < lines.len() && !lines[i].starts_with("<SEARCH>") {
                i += 1;
            }
            i += 1;

            while i < lines.len() && !lines[i].starts_with("</SEARCH>") {
                original.push_str(lines[i]);
                original.push('\n');
                i += 1;
            }
            i += 1;

            while i < lines.len() && !lines[i].starts_with("<REPLACE>") {
                i += 1;
            }
            i += 1;

            while i < lines.len() && !lines[i].starts_with("</REPLACE>") {
                modified.push_str(lines[i]);
                modified.push('\n');
                i += 1;
            }

            changes.push((file_path, original, modified));
        }
        i += 1;
    }

    changes
}

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

fn display_ai_response(response: &str) {
    println!("{}", "AI Response:".bold().green());
    for line in textwrap::wrap(response, 80) {
        println!("  {}", line);
    }
    println!();
}

async fn handle_response_actions(
    response: &str,
    api_key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let action = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do with the response?")
            .default(0)
            .items(&["Copy to clipboard", "Save to file", "Continue"])
            .interact()?;

        match action {
            0 => copy_to_clipboard(response)?,
            1 => save_to_file(response, api_key).await?,
            2 => break,
            _ => unreachable!(),
        }
    }
    Ok(())
}

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

fn save_conversation(conversation_history: &[Message]) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(conversation_history)?;
    std::fs::write("conversation_history.json", json)?;
    println!("Conversation saved successfully.");
    Ok(())
}

fn load_conversation() -> std::io::Result<Vec<Message>> {
    let json = std::fs::read_to_string("conversation_history.json")?;
    let conversation_history: Vec<Message> = serde_json::from_str(&json)?;
    println!("Conversation loaded successfully.");
    Ok(conversation_history)
}

fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx: ClipboardContext = ClipboardProvider::new()?;
    ctx.set_contents(text.to_owned())?;
    println!("Output copied to clipboard.");
    Ok(())
}

async fn save_to_file(text: &str, api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    let filename = generate_organized_filename(api_key, text).await?;
    let output_dir = "ai_responses";
    std::fs::create_dir_all(output_dir)?;
    let file_path = format!("{}/{}", output_dir, filename);
    let mut file = File::create(&file_path)?;
    file.write_all(text.as_bytes())?;
    println!("Output saved to file: {}", file_path);
    Ok(())
}

fn print_header(title: &str) {
    let width = 60;
    println!(
        "{}",
        HEAVY_DOWN_AND_RIGHT.to_string()
            + &HEAVY_HORIZONTAL.to_string().repeat(width - 2)
            + &HEAVY_DOWN_AND_LEFT.to_string()
    );
    println!(
        "{} {: ^width$} {}",
        HEAVY_VERTICAL,
        title.bold().green(),
        HEAVY_VERTICAL
    );
    println!(
        "{}",
        HEAVY_UP_AND_RIGHT.to_string()
            + &HEAVY_HORIZONTAL.to_string().repeat(width - 2)
            + &HEAVY_UP_AND_LEFT.to_string()
    );
}

fn browse_index(index: &HashMap<String, (String, String)>) {
    let mut files: Vec<&String> = index.keys().collect();
    files.sort();

    loop {
        clear_screen();
        print_header("Browse Index");

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a file to view its summary (or 'Back' to return)")
            .default(0)
            .items(&files)
            .item("Back")
            .interact()
            .unwrap();

        if selection == files.len() {
            break;
        } else {
            let file = &files[selection];
            if let Some((summary, language)) = index.get(*file) {
                clear_screen();
                print_header(&format!("File Summary: {}", file));
                println!("{}: {}", "Language".bold(), language);
                println!("{}: {}", "Summary".bold(), summary);
                pause();
            } else {
                println!("Error: File not found in index");
                pause();
            }
        }
    }
}
