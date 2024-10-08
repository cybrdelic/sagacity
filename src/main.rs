use reqwest;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use walkdir::WalkDir;

use colored::*;
use dialoguer::{theme::ColorfulTheme, Select};
use indicatif::{ProgressBar, ProgressStyle};
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context, Editor};
use spinners::{Spinner, Spinners};
use std::env;
use std::time::Instant;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

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
            "max_tokens": 150
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

fn search_index(index: &HashMap<String, String>, query: &str) -> Vec<(String, String)> {
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
    index
        .iter()
        .filter(|(_, summary)| {
            let summary_lower = summary.to_lowercase();
            query_words.iter().any(|&word| summary_lower.contains(word))
        })
        .map(|(file, summary)| (file.clone(), summary.clone()))
        .collect()
}

async fn chat_with_system(
    index: &HashMap<String, String>,
    api_key: &str,
    user_query: &str,
    conversation_history: &mut Vec<Value>,
) -> Result<(String, bool), Box<dyn std::error::Error>> {
    debug_print!("Starting chat with system");

    // Step 1: Find relevant files
    let relevant_files = search_index(index, user_query);

    // Step 2: Extract file paths from relevant_files
    let relevant_file_paths: Vec<String> =
        relevant_files.into_iter().map(|(file, _)| file).collect();

    // Step 3: Prepare context for the LLM
    let context = prepare_context(&relevant_file_paths, user_query)?;

    // Step 4: Generate response using the LLM
    let (response, is_complete) =
        generate_llm_response(&context, api_key, conversation_history, user_query).await?;

    Ok((response, is_complete))
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
    conversation_history: &mut Vec<Value>,
    user_query: &str,
) -> Result<(String, bool), Box<dyn std::error::Error>> {
    debug_print!("Generating LLM response");
    let client = reqwest::Client::new();

    let mut messages = conversation_history.clone();
    if messages.is_empty() || messages.last().unwrap()["role"] != "user" {
        messages.push(json!({
            "role": "user",
            "content": format!("Based on the following context about a codebase, please answer the user's query:\n\nContext: {}\n\nUser query: {}", context, user_query)
        }));
    } else {
        // If the last message is already from the user, update its content
        let last_index = messages.len() - 1;
        messages[last_index] = json!({
            "role": "user",
            "content": format!("Based on the following context about a codebase, please answer the user's query:\n\nContext: {}\n\nUser query: {}", context, user_query)
        });
    }

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": messages,
            "max_tokens": 500
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

    // Update conversation history
    if conversation_history.is_empty() || conversation_history.last().unwrap()["role"] != "user" {
        conversation_history.push(json!({
            "role": "user",
            "content": user_query
        }));
    }
    conversation_history.push(json!({
        "role": "assistant",
        "content": answer.clone()
    }));

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
    println!(
        "{}",
        "Welcome to the Enhanced Codebase Explorer!".bold().green()
    );
    let root_dir = "."; // Current directory
    println!("Root directory: {}", root_dir.cyan());

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
        let choices = vec!["Search", "Chat", "Print Index", "Quit"];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose an action")
            .default(0)
            .items(&choices)
            .interact()?;

        match selection {
            0 => search_mode(&index, &mut rl)?,
            1 => chat_mode(&index, &api_key, &mut rl).await?,
            2 => print_index(&index),
            3 => {
                println!("{}", "Exiting application. Goodbye!".bold().green());
                break;
            }
            _ => unreachable!(),
        }
    }

    Ok(())
}

fn search_mode(
    index: &HashMap<String, String>,
    rl: &mut Editor<MyHelper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let query = rl.readline("Enter your search query (or 'back' to return to main menu): ")?;
        let query = query.trim();

        if query.to_lowercase() == "back" {
            break;
        }

        let results: Vec<(String, String)> = search_index(index, query);
        if results.is_empty() {
            println!("{}", "No results found for your query.".yellow());
        } else {
            println!("{}", format!("Found {} results:", results.len()).green());
            for (i, (file, summary)) in results.iter().enumerate() {
                println!("{}. {}", (i + 1).to_string().cyan(), file.bold());
                println!("   {}", summary);
            }

            println!(
                "\nEnter the number of a file to view its contents, or press Enter to continue:"
            );
            let choice = rl.readline("> ")?.trim().to_string();
            if let Ok(index) = choice.parse::<usize>() {
                if index > 0 && index <= results.len() {
                    if let Some((file, _)) = results.get(index - 1) {
                        view_file_contents(file)?;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn chat_mode(
    index: &HashMap<String, String>,
    api_key: &str,
    rl: &mut Editor<MyHelper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "{}",
        "Starting chat session. Type 'exit' to end the chat."
            .bold()
            .green()
    );
    let mut conversation_history: Vec<Value> = Vec::new();

    loop {
        let chat_query = rl.readline("Chat: Enter your question about the codebase: ")?;
        let chat_query = chat_query.trim();

        if chat_query.to_lowercase() == "exit" {
            println!("{}", "Ending chat session.".yellow());
            break;
        }

        let mut spinner = Spinner::new(Spinners::Dots9, "Thinking...".into());
        let (response, is_complete) =
            chat_with_system(index, api_key, chat_query, &mut conversation_history).await?;
        spinner.stop();

        println!("{}", "System:".bold().green());
        println!("{}", response);

        if !is_complete {
            let choice = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("The response was incomplete. What would you like to do?")
                .default(0)
                .items(&["Continue", "Skip"])
                .interact()?;

            if choice == 0 {
                let mut spinner = Spinner::new(Spinners::Dots9, "Continuing...".into());
                let (additional_response, _) = chat_with_system(
                    index,
                    api_key,
                    "Please continue your previous response.",
                    &mut conversation_history,
                )
                .await?;
                spinner.stop();
                println!("{}", "System (continued):".bold().green());
                println!("{}", additional_response);
            }
        }
    }
    Ok(())
}

fn print_index(index: &HashMap<String, String>) {
    println!("{}", "Full index:".bold().green());
    for (file, summary) in index {
        println!("{}", file.bold());
        println!("{}\n", summary);
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


