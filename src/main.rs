use clipboard::{ClipboardContext, ClipboardProvider};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};

use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};

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
use std::collections::HashSet;
use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use textwrap;

const HEAVY_DOWN_AND_RIGHT: char = '┏';
const HEAVY_DOWN_AND_LEFT: char = '┓';
const HEAVY_UP_AND_RIGHT: char = '┗';
const HEAVY_UP_AND_LEFT: char = '┛';
const HEAVY_HORIZONTAL: char = '━';
const HEAVY_VERTICAL: char = '┃';
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
    file_mod_times: HashMap<String, u64>, // Add this line
}

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
    let serialized = serde_json::to_string(&cache)?;
    fs::write("index_cache.json", serialized)?;
    Ok(())
}

fn load_index_cache() -> Result<Option<IndexCache>, Box<dyn std::error::Error>> {
    if let Ok(contents) = fs::read_to_string("index_cache.json") {
        let cache: IndexCache = serde_json::from_str(&contents)?;
        Ok(Some(cache))
    } else {
        Ok(None)
    }
}

async fn index_codebase(
    root_dir: &str,
    api_key: &str,
    pb: &ProgressBar,
    previous_index: Option<IndexCache>, // Accept previous index
) -> Result<
    (HashMap<String, (String, String)>, u64, HashMap<String, u64>),
    Box<dyn std::error::Error>,
> {
    let mut index = previous_index
        .as_ref()
        .map_or(HashMap::new(), |cache| cache.index.clone());
    let mut file_mod_times = previous_index
        .as_ref()
        .map_or(HashMap::new(), |cache| cache.file_mod_times.clone());

    let files = scan_codebase(root_dir);
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
            let content = read_file_contents(&file_path)
                .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

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
            file_mod_times.insert(file_path.clone(), modified_secs);
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

fn detect_language(file_path: &str) -> String {
    let extension = std::path::Path::new(file_path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");

    match extension {
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "ts" => "typescript",
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
            "system": "You are an AI assistant helping with a codebase. Use the provided context and conversation history to answer questions. In all your responses, keep a cool and chill vibe that is cracked and overpowered in terms of technical ability and aptitude. You are personable but not in a creepy fake way. Think L from Death Note or Gilfoyle from Silicon Valley. Dont try to be personable or cool, but also dont be too pessimistic. Don't be cringe",
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
        "Based on the following content, generate a concise and descriptive filename (max 50 characters) that summarizes the main topic or purpose. title it in all caps and keep it from 1 to 4 words. Include the .md extension. Only return the filename, nothing else:\n\n{}",
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

    // Automatically load conversation history for the default session
    if let Ok(history) = load_conversation() {
        chatbot.memory = history;
        println!("{}", "Conversation history loaded successfully.".green());
    } else {
        chatbot.memory = Vec::new();
    }

    loop {
        clear_screen();
        match display_main_menu() {
            MainMenuOption::Chat => chat_mode(&mut chatbot, &mut rl).await?,
            MainMenuOption::BrowseIndex => browse_index(&chatbot.index),
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

    let cache = load_index_cache()?;
    let (index, _, _) = index_codebase(root_dir, api_key, &pb, cache).await?;

    pb.finish_with_message("Indexing completed");
    Ok(index)
}

enum MainMenuOption {
    Chat,
    BrowseIndex,
    Help,
    Quit,
}

fn display_main_menu() -> MainMenuOption {
    let choices = vec!["Chat with AI", "Browse Index", "Help", "Quit"];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do?")
        .default(0)
        .items(&choices)
        .interact()
        .unwrap();

    match selection {
        0 => MainMenuOption::Chat,
        1 => MainMenuOption::BrowseIndex,
        2 => MainMenuOption::Help,
        3 => MainMenuOption::Quit,
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

async fn handle_response_actions_simple(
    response: &str,
    api_key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let action = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do with the response?")
        .default(0)
        .items(&["Copy to clipboard", "Save to file", "Continue"])
        .interact()?;

    match action {
        0 => copy_to_clipboard(response)?,
        1 => save_to_file(response, api_key).await?,
        2 => {}
        _ => unreachable!(),
    }
    Ok(())
}

async fn chat_mode(
    chatbot: &mut Chatbot,
    rl: &mut Editor<MyHelper, DefaultHistory>,
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
            // Remove /save and /load commands as saving/loading is now automatic
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

        // Automatically save conversation history after each response
        save_conversation(&chatbot.memory)?;

        display_ai_response(&response);

        // Handle response actions without diff-related options
        handle_response_actions_simple(&response, &chatbot.api_key).await?;
    }
    Ok(())
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
