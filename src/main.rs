mod chatbot;
mod constants;
mod ui;

use chatbot::*;
use chrono::{DateTime, Utc};
use clipboard::{ClipboardContext, ClipboardProvider};
use colored::Colorize;
use constants::*;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use home::home_dir;
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
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio;
use ui::*; // Import the UI module

// Define the debug_print macro
macro_rules! debug_print {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            println!($($arg)*);
        }
    };
}

// Struct for GitHub repository information
#[derive(Deserialize)]
struct GitHubRepo {
    full_name: String,
    clone_url: String,
}

// Main function
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    clear_screen();
    display_welcome_screen();

    // Call the codebase selection menu
    let selected_codebase = codebase_selection_menu().await?;
    println!("Selected codebase: {:?}", selected_codebase);

    // Proceed with initializing the selected codebase
    let root_dir = selected_codebase.to_str().unwrap_or(".");
    let api_key = get_claude_api_key()?;
    let mut chatbot = initialize_codebase_index(root_dir, &api_key).await?;

    let mut rl = Editor::<MyHelper, DefaultHistory>::new()?;
    rl.set_helper(Some(MyHelper::new(FilenameCompleter::new())));

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
