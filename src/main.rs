use reqwest;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use walkdir::WalkDir;

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
use std::io::{self, Write};
use std::time::Instant;
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
            matches!(extension, Some("rs") | Some("toml") | Some("md"))
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
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Summarizing content with Claude");
    let client = reqwest::Client::new();
    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION) // Add this line
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": [
                {
                    "role": "user",
                    "content": format!("Provide a very concise summary (2-3 sentences max) of the following code, focusing on its main purpose and key functionalities:\n\n{}", content)
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

async fn index_codebase(
    root_dir: &str,
    api_key: &str,
    pb: &ProgressBar,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut index = HashMap::new();
    let files = scan_codebase(root_dir);
    pb.set_length(files.len() as u64);

    for (i, file_path) in files.iter().enumerate() {
        pb.set_message(format!(
            "Processing file {}/{}: {}",
            i + 1,
            files.len(),
            file_path
        ));
        let content = read_file_contents(&file_path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

        let summary = match summarize_with_claude(&content, api_key).await {
            Ok(summary) => summary,
            Err(_e) => {
                format!(
                    "Failed to summarize. File content preview: {}",
                    &content[..std::cmp::min(content.len(), 100)]
                )
            }
        };

        index.insert(file_path.clone(), summary);
        pb.inc(1);
    }

    pb.finish_with_message(format!(
        "Indexing complete. Total files indexed: {}",
        index.len()
    ));
    Ok(index)
}

fn search_index(index: &HashMap<String, String>, query: &str) -> Vec<(String, f32)> {
    let mut relevant_files = Vec::new();
    for (file, summary) in index {
        let relevance = calculate_relevance(summary, query);
        if relevance > 0.0 {
            relevant_files.push((file.clone(), relevance));
        }
    }
    relevant_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    relevant_files.truncate(5); // Limit to top 5 most relevant files
    relevant_files
}

fn calculate_relevance(summary: &str, query: &str) -> f32 {
    let summary_words: Vec<&str> = summary.split_whitespace().collect();
    let query_words: Vec<&str> = query.split_whitespace().collect();

    let mut relevance = 0.0;
    for query_word in &query_words {
        if summary_words.contains(query_word) {
            relevance += 1.0;
        }
    }
    relevance / query_words.len() as f32
}

async fn chat_with_system(
    index: &HashMap<String, String>,
    api_key: &str,
    user_query: &str,
    conversation_history: &mut Vec<Value>,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Starting chat with system");

    // Step 1: Find relevant files
    let relevant_files = search_index(index, user_query);

    // Step 2: Extract file paths from relevant_files
    let relevant_file_paths: Vec<String> =
        relevant_files.into_iter().map(|(file, _)| file).collect();

    // Step 3: Prepare context for the LLM
    let context = prepare_context(&relevant_file_paths, user_query)?;

    // Step 4: Generate response using the LLM
    let (response, _) =
        generate_llm_response(&context, api_key, conversation_history, user_query).await?;

    // Step 5: Update conversation history
    conversation_history.push(json!({
        "role": "user",
        "content": user_query
    }));
    conversation_history.push(json!({
        "role": "assistant",
        "content": response.clone()
    }));

    Ok(response)
}

fn prepare_context(
    relevant_files: &[String],
    user_query: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut context = format!("User query: {}\n\nRelevant file contents:\n", user_query);
    for file_path in relevant_files {
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
    pb.set_message("Indexing codebase...");

    let start = Instant::now();
    let index = index_codebase(root_dir, &api_key, &pb).await?;
    let duration = start.elapsed();
    pb.finish_with_message(format!("Indexing completed in {:?}", duration));

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
    index: &HashMap<String, String>,
    api_key: &str,
    rl: &mut Editor<MyHelper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut conversation_history: Vec<Value> = Vec::new();

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
                conversation_history.clear();
                println!("{}", "Conversation history cleared.".bold().green());
                continue;
            }
            "/help" => {
                display_help();
                continue;
            }
            "/save" => {
                save_conversation(&conversation_history)?;
                continue;
            }
            "/load" => {
                conversation_history = load_conversation()?;
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

        let response =
            chat_with_system(index, api_key, chat_query, &mut conversation_history).await?;

        pb.finish_and_clear();

        conversation_history.push(json!({
            "role": "user",
            "content": chat_query
        }));
        conversation_history.push(json!({
            "role": "assistant",
            "content": response.clone()
        }));

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

fn save_conversation(conversation_history: &[Value]) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(conversation_history)?;
    std::fs::write("conversation_history.json", json)?;
    println!("Conversation saved successfully.");
    Ok(())
}

fn load_conversation() -> std::io::Result<Vec<Value>> {
    let json = std::fs::read_to_string("conversation_history.json")?;
    let conversation_history: Vec<Value> = serde_json::from_str(&json)?;
    println!("Conversation loaded successfully.");
    Ok(conversation_history)
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

fn print_index(index: &HashMap<String, String>) {
    print_header("Index Browsing Mode");
    for (file, summary) in index {
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
