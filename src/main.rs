// src/main.rs

#[macro_use]
mod macros;

mod api;
mod chatbot;
mod constants;
mod conversation;
mod indexing;
mod logging;
mod models;
mod ui;
mod utils;

use chatbot::chat_with_system;
use chrono::Utc;
use clipboard::{ClipboardContext, ClipboardProvider};
use colored::Colorize;
use constants::*;
use conversation::{load_conversation, save_conversation};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use models::Chatbot;
use prettytable::{Cell, Row, Table};
use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context as RLContext, Editor};
use std::collections::HashMap;
use std::env; // For environment variables
use std::fs::{self, File}; // For file operations
use std::io::Write;
use tokio;
use ui::*; // Import the UI module

/// Helper struct for rustyline autocomplete and other features.
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

    fn hint(&self, _line: &str, _pos: usize, _ctx: &RLContext<'_>) -> Option<Self::Hint> {
        None
    }
}

impl Completer for MyHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &RLContext<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        self.completer.complete(line, pos, ctx)
    }
}

impl rustyline::Helper for MyHelper {}

/// Main function: initializes the application and handles the main event loop.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    clear_screen();
    display_welcome_screen();

    // Call the codebase selection menu
    /// Displays a menu for selecting a codebase and returns the selected path.
    async fn codebase_selection_menu() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let choices = vec!["Codebase1", "Codebase2", "Codebase3"];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select a codebase to work with:")
            .default(0)
            .items(&choices)
            .interact()?;

        // Convert the selection to a PathBuf (this is just a placeholder logic)
        let selected_path = std::path::PathBuf::from(choices[selection]);
        Ok(selected_path)
    }
    let selected_codebase = codebase_selection_menu().await?;
    println!("Selected codebase: {:?}", selected_codebase);

    // Proceed with initializing the selected codebase
    let root_dir = selected_codebase.to_str().unwrap_or(".");
    let api_key = get_claude_api_key()?;
    let mut chatbot = initialize_codebase_index(root_dir, &api_key).await?;

    let mut rl = Editor::<MyHelper, DefaultHistory>::new()?;
    rl.set_helper(Some(MyHelper::new(FilenameCompleter::new())));

    // Automatically load conversation history for the default session
    if let Ok(_) = load_conversation(&mut chatbot) {
        println!("{}", "Conversation history loaded successfully.".green());
    } else {
        chatbot.memory = Vec::new();
    }

    loop {
        clear_screen();
        match display_main_menu() {
            MainMenuOption::Chat => chat_mode(&mut chatbot, &mut rl).await?,
            MainMenuOption::BrowseIndex => browse_index(&chatbot.index),
            MainMenuOption::Debug => display_api_call_logs(&chatbot),
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

/// Retrieves the Claude API key from the `.zshrc` file.
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

/// Scans the codebase for relevant files based on specific extensions.
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

/// Reads the contents of a file.
fn read_file_contents(file_path: &str) -> Result<String, std::io::Error> {
    fs::read_to_string(file_path)
}

/// Initializes the codebase index by delegating to the chatbot module.
async fn initialize_codebase_index(
    root_dir: &str,
    api_key: &str,
) -> Result<Chatbot, Box<dyn std::error::Error>> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Indexing codebase...");

    let cache = indexing::load_index_cache()?;
    let index = cache.as_ref().map(|c| c.index.clone()).unwrap_or_default();
    let file_mod_times = cache
        .as_ref()
        .map(|c| c.file_mod_times.clone())
        .unwrap_or_default();

    let mut chatbot = Chatbot::new(index, file_mod_times, api_key.to_string());

    let (_new_index, _last_modification, updated_file_mod_times) =
        indexing::index_codebase(root_dir, api_key, &pb, &mut chatbot).await?;

    pb.finish_with_message("Indexing completed");

    // Update chatbot's index and file_mod_times with new data
    chatbot.index = _new_index;
    chatbot.file_mod_times = updated_file_mod_times;

    Ok(chatbot)
}

/// Enum representing the main menu options.
enum MainMenuOption {
    Chat,
    BrowseIndex,
    Debug,
    Help,
    Quit,
}

/// Displays the main menu and returns the selected option.
fn display_main_menu() -> MainMenuOption {
    let choices = vec!["Chat with AI", "Browse Index", "Debug Mode", "Help", "Quit"];
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("What would you like to do?")
        .default(0)
        .items(&choices)
        .interact()
        .unwrap();

    match selection {
        0 => MainMenuOption::Chat,
        1 => MainMenuOption::BrowseIndex,
        2 => MainMenuOption::Debug,
        3 => MainMenuOption::Help,
        4 => MainMenuOption::Quit,
        _ => unreachable!(),
    }
}

/// Pauses the execution and waits for the user to press Enter.
fn pause() {
    println!("\nPress Enter to continue...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}

/// Displays the goodbye message upon exiting.
fn display_goodbye_message() {
    clear_screen();
    println!("{}", "Thank you for using Codebase Explorer".bold().green());
    println!("Have a great day!");
}

/// Handles the main chat mode, including user input and AI responses.
async fn chat_mode(
    chatbot: &mut Chatbot,
    rl: &mut Editor<MyHelper, DefaultHistory>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Automatically load conversation history at the start of chat mode
    if let Ok(_) = load_conversation(chatbot) {
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
                save_conversation(chatbot)?;
                continue;
            }
            "/help" => {
                display_chat_help();
                pause();
                continue;
            }
            "/save" => {
                save_conversation(chatbot)?;
                pause();
                continue;
            }
            "/load" => {
                load_conversation(chatbot)?;
                pause();
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

        // Update conversation history
        save_conversation(chatbot)?;

        display_ai_response(&response);

        // Handle response actions without diff-related options
        handle_response_actions_simple(&response, &chatbot.api_key).await?;
    }
    Ok(())
}

/// Handles response actions such as copying to clipboard or saving to a file.
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

/// Copies the provided text to the system clipboard.
fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut ctx: ClipboardContext = ClipboardProvider::new()?;
    ctx.set_contents(text.to_owned())?;
    println!("Output copied to clipboard.");
    Ok(())
}

/// Saves the provided text to a file with an organized filename.
async fn save_to_file(text: &str, api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    let filename = generate_organized_filename(api_key, text).await?;
    let output_dir = "ai_responses";
    fs::create_dir_all(output_dir)?;
    let file_path = format!("{}/{}", output_dir, filename);
    let mut file = File::create(&file_path)?;
    file.write_all(text.as_bytes())?;
    println!("Output saved to file: {}", file_path);
    Ok(())
}

/// Generates an organized filename using the Claude API.
async fn generate_organized_filename(
    api_key: &str,
    content: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Generating organized filename");
    let client = reqwest::Client::new();

    let prompt = format!(
        "Based on the following content, generate a concise and descriptive filename (max 50 characters) that summarizes the main topic or purpose. Title it in all caps and keep it from 1 to 4 words. Include the .md extension. Only return the filename, nothing else:\n\n{}",
        content
    );

    let start_time = std::time::Instant::now();

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&serde_json::json!({
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

    let body: serde_json::Value = response.json().await?;

    let filename = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_string();

    Ok(filename)
}

/// Displays the API call logs in a formatted table.
pub fn display_api_call_logs(chatbot: &Chatbot) {
    if chatbot.api_call_logs.is_empty() {
        println!("{}", "No API calls have been made yet.".yellow());
        pause();
        return;
    }

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        Cell::new("Timestamp"),
        Cell::new("Endpoint"),
        Cell::new("Request Summary"),
        Cell::new("Status"),
        Cell::new("Response Time (ms)"),
    ]));

    for log in &chatbot.api_call_logs {
        table.add_row(Row::new(vec![
            Cell::new(&log.timestamp.to_rfc3339()),
            Cell::new(&log.endpoint),
            Cell::new(&log.request_summary),
            Cell::new(&log.response_status.to_string()),
            Cell::new(&log.response_time_ms.to_string()),
        ]));
    }

    table.printstd();
    pause();
}

/// Allows users to browse the indexed files and view their summaries.
pub fn browse_index(index: &HashMap<String, (String, String)>) {
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
