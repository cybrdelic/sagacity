use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

use clipboard::{ClipboardContext, ClipboardProvider};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use indicatif::{ProgressBar, ProgressStyle};
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context, Editor};
use std::env;
use std::fs::File;
use std::io::{self, Write};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};
use termimad::crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use termimad::MadSkin;
// Unicode box-drawing characters
const LIGHT_DOWN_AND_RIGHT: char = '┌';
const LIGHT_DOWN_AND_LEFT: char = '┐';
const LIGHT_UP_AND_RIGHT: char = '└';
const LIGHT_UP_AND_LEFT: char = '┘';
const LIGHT_VERTICAL_AND_RIGHT: char = '├';
const LIGHT_VERTICAL_AND_LEFT: char = '┤';
const LIGHT_HORIZONTAL: char = '─';
const LIGHT_VERTICAL: char = '│';

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

struct Chatbot {
    index: HashMap<String, (String, String)>,
    api_key: String,
    memory: Vec<Message>,
}

impl Chatbot {
    fn new(index: HashMap<String, (String, String)>, api_key: String) -> Self {
        Chatbot {
            index,
            api_key,
            memory: Vec::new(),
        }
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
        let memory_json: Vec<Value> = self
            .memory
            .iter()
            .map(|m| {
                json!({
                    "role": m.role,
                    "content": m.content
                })
            })
            .collect();
        let (response, _) =
            generate_llm_response(&context, &self.api_key, &memory_json, user_query).await?;

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

    fn get_last_user_message(&self) -> Option<&Message> {
        self.memory.iter().rev().find(|m| m.role == "user")
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
    conversation_history: &Vec<Value>,
    user_query: &str,
) -> Result<(String, bool), Box<dyn std::error::Error>> {
    debug_print!("Generating LLM response");
    let client = reqwest::Client::new();

    let mut messages = conversation_history.clone();
    messages.push(json!({
        "role": "user",
        "content": format!("Based on the following context about a codebase, please answer the user's query:\n\nContext: {}\n\nUser query: {}", context, user_query)
    }));

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": messages,
            "max_tokens":4000
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
    print_header("Welcome to the Enhanced Codebase Explorer");
    let root_dir = "."; // Current directory
    println!("{}", "Root directory:".bold());
    println!("  {}", root_dir.cyan());

    let api_key = get_claude_api_key()?;
    println!("{}", "API key retrieved successfully".green());

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );

    let (index, _last_modification) = if let Some((cache_timestamp, last_mod, cached_index)) =
        load_index_cache()?
    {
        let current_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        if current_time - cache_timestamp < 3600 && !check_for_codebase_changes(root_dir, last_mod)?
        {
            println!("{}", "Using cached index".green());
            (cached_index, last_mod)
        } else {
            pb.set_message("Changes detected or cache outdated. Reindexing codebase...");
            index_codebase(root_dir, &api_key, &pb).await?
        }
    } else {
        pb.set_message("Indexing codebase...");
        index_codebase(root_dir, &api_key, &pb).await?
    };

    pb.finish_with_message("Indexing completed");

    println!(
        "{}",
        "Codebase indexed successfully. You can now explore the codebase."
            .bold()
            .green()
    );
    println!(
        "Number of indexed files: {}",
        index.len().to_string().yellow()
    );

    let mut rl = Editor::<MyHelper, DefaultHistory>::new()?;
    rl.set_helper(Some(MyHelper::new(FilenameCompleter::new())));

    loop {
        let choices = vec!["Chat", "Print Index", "Quit"];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose an action")
            .default(0)
            .items(&choices)
            .interact()?;

        match selection {
            0 => chat_mode(&index, &api_key, &mut rl).await?,
            1 => print_index(&index),
            2 => {
                println!("{}", "Exiting application. Goodbye!".bold().green());
                break;
            }
            _ => unreachable!(),
        }
    }

    Ok(())
}

async fn chat_mode(
    index: &HashMap<String, (String, String)>,
    api_key: &str,
    rl: &mut Editor<MyHelper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut chatbot = Chatbot::new(index.clone(), api_key.to_string());

    loop {
        print_header("Chat Mode");
        let chat_query = rl.readline(&format!(
            "{} ",
            "Enter your question (or type '/exit' to end chat, '/help' for commands):"
                .bold()
                .yellow()
        ))?;
        let chat_query = chat_query.trim();

        match chat_query {
            "/exit" => {
                println!("{}", "Ending chat session.".bold().green());
                break;
            }
            "/clear" => {
                chatbot.memory.clear();
                println!("{}", "Conversation history cleared.".bold().green());
                continue;
            }
            "/help" => {
                display_help();
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
            "/last" => {
                if let Some(last_message) = chatbot.get_last_user_message() {
                    println!("Your last message was: {}", last_message.content.cyan());
                } else {
                    println!("No previous messages found.");
                }
                continue;
            }
            _ => {}
        }

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.set_message("AI is thinking...");
        pb.enable_steady_tick(std::time::Duration::from_millis(120));

        let response = chat_with_system(&mut chatbot, chat_query).await?;

        pb.finish_and_clear();

        println!(
            "{}",
            LIGHT_DOWN_AND_RIGHT.to_string()
                + &LIGHT_HORIZONTAL.to_string().repeat(58)
                + &LIGHT_DOWN_AND_LEFT.to_string()
        );
        println!(
            "{} {: <56} {}",
            LIGHT_VERTICAL,
            "You:".bold().blue(),
            LIGHT_VERTICAL
        );
        for line in textwrap::wrap(chat_query, 56) {
            println!("{} {: <56} {}", LIGHT_VERTICAL, line, LIGHT_VERTICAL);
        }
        println!(
            "{}",
            LIGHT_VERTICAL_AND_RIGHT.to_string()
                + &LIGHT_HORIZONTAL.to_string().repeat(58)
                + &LIGHT_VERTICAL_AND_LEFT.to_string()
        );
        println!(
            "{} {: <56} {}",
            LIGHT_VERTICAL,
            "AI:".bold().green(),
            LIGHT_VERTICAL
        );
        for line in textwrap::wrap(&response, 56) {
            println!("{} {: <56} {}", LIGHT_VERTICAL, line, LIGHT_VERTICAL);
        }
        println!(
            "{}",
            LIGHT_UP_AND_RIGHT.to_string()
                + &LIGHT_HORIZONTAL.to_string().repeat(58)
                + &LIGHT_UP_AND_LEFT.to_string()
        );
        println!();

        // Prompt for copying output or saving to file
        loop {
            let action = rl.readline(&format!(
                "{} ",
                "Do you want to (c)opy to clipboard, (s)ave to file, or (n)either? [c/s/n]"
                    .bold()
                    .yellow()
            ))?;
            match action.trim().to_lowercase().as_str() {
                "c" => {
                    if let Err(e) = copy_to_clipboard(&response) {
                        eprintln!("Failed to copy to clipboard: {}", e);
                    }
                    break;
                }
                "s" => {
                    if let Err(e) = save_to_file(&response, api_key).await {
                        eprintln!("Failed to save to file: {}", e);
                    }
                    break;
                }
                "n" => break,
                _ => println!("Invalid option. Please choose 'c', 's', or 'n'."),
            }
        }
    }
    Ok(())
}

use textwrap::wrap;

fn display_conversation(conversation_history: &[Value], _skin: &MadSkin) -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, Clear(ClearType::All))?;

    let terminal_width = termimad::crossterm::terminal::size()?.0 as usize;
    let content_width = terminal_width.saturating_sub(4); // Account for minimal indentation

    for message in conversation_history {
        let role = message["role"].as_str().unwrap_or("unknown");
        let content = message["content"].as_str().unwrap_or("");

        let formatted_role = match role {
            "user" => "You:".blue().bold(),
            "assistant" => "AI:".green().bold(),
            _ => "Unknown:".yellow().bold(),
        };

        println!("{}", formatted_role);

        if role == "assistant" {
            format_ai_response(content, content_width)?;
        } else {
            let wrapped_content = wrap(content, content_width);
            for line in wrapped_content {
                println!("  {}", line.trim());
            }
        }
        println!();
    }

    stdout.flush()?;
    Ok(())
}

fn format_ai_response(content: &str, width: usize) -> std::io::Result<()> {
    let mut in_code_block = false;
    let mut code_block_content = String::new();
    let indent = "    ";

    for line in content.lines() {
        if line.trim().starts_with("```") {
            if in_code_block {
                // End of code block, print the collected content
                println!("{}```", indent);
                for code_line in code_block_content.lines() {
                    println!("{}{}", indent, code_line.yellow());
                }
                println!("{}```", indent);
                code_block_content.clear();
            } else {
                // Start of code block
                println!("{}{}", indent, line.trim());
            }
            in_code_block = !in_code_block;
        } else if in_code_block {
            code_block_content.push_str(line);
            code_block_content.push('\n');
        } else {
            let wrapped_lines = wrap(line.trim(), width.saturating_sub(indent.len()));
            for wrapped in wrapped_lines {
                println!("{}{}", indent, wrapped);
            }
        }
    }

    // Handle any remaining code block content
    if !code_block_content.is_empty() {
        for code_line in code_block_content.lines() {
            println!("{}{}", indent, code_line.yellow());
        }
        println!("{}```", indent);
    }

    Ok(())
}

fn display_typing_indicator(skin: &MadSkin) -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(
        stdout,
        MoveTo(0, termimad::crossterm::terminal::size()?.1 - 1)
    )?;
    skin.print_text("*System is typing...*");
    stdout.flush()?;
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

fn print_index(index: &HashMap<String, (String, String)>) {
    print_header("Index Browsing Mode");
    for (file, (summary, _)) in index {
        println!("{}", file.bold().blue());
        println!("{}", LIGHT_VERTICAL);
        for line in textwrap::wrap(summary, 80) {
            println!("{}  {}", LIGHT_VERTICAL, line);
        }
        println!("{}\n", LIGHT_VERTICAL);
    }
}

fn view_file_contents(file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let content = read_file_contents(file_path)?;

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ss
        .find_syntax_for_file(file_path)?
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    println!("{}", format!("Contents of {}:", file_path).bold().green());
    for line in LinesWithEndings::from(&content) {
        let ranges: Vec<(Style, &str)> = h.highlight_line(line, &ss)?;
        let escaped = as_24_bit_terminal_escaped(&ranges[..], true);
        print!("{}", escaped);
    }
    println!();
    Ok(())
}
