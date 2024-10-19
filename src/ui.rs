// src/ui.rs

use crate::constants::*;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use prettytable::{Cell, Row, Table};
use std::collections::HashMap;

/// Clears the terminal screen.
pub fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
}

/// Displays the welcome screen.
pub fn display_welcome_screen() {
    println!("{}", "Welcome to Codebase Explorer".bold().cyan());
    println!("{}", "Your intelligent coding companion".italic());
    println!("\nInitializing...");
}

/// Displays the goodbye message upon exiting.
pub fn display_goodbye_message() {
    clear_screen();
    println!("{}", "Thank you for using Codebase Explorer".bold().green());
    println!("Have a great day!");
}

/// Enum representing the main menu options.
pub enum MainMenuOption {
    Chat,
    BrowseIndex,
    Debug,
    Help,
    Quit,
}

/// Displays the main menu and returns the selected option.
pub fn display_main_menu() -> MainMenuOption {
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

/// Displays the help menu.
pub fn display_help() {
    println!("{}", "Help Menu".bold().yellow());
    println!("Available commands: /help, /exit, /clear, /save, /load");
}

/// Pauses the execution and waits for the user to press Enter.
pub fn pause() {
    println!("\nPress Enter to continue...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}

/// Displays the AI response in a formatted manner.
pub fn display_ai_response(response: &str) {
    println!("{}", "AI Response:".bold().green());
    for line in textwrap::wrap(response, 80) {
        println!("  {}", line);
    }
    println!();
}

/// Displays the chat history in the chat mode.
pub fn display_chat_history(chatbot: &crate::models::Chatbot) {
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

/// Displays the chat help menu.
pub fn display_chat_help() {
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

/// Prints a header with decorative borders.
pub fn print_header(title: &str) {
    let width = 80; // Adjusted width for better display
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

/// Displays the chat history in the main chat mode.
pub fn display_chat_history_table(index: &HashMap<String, (String, String)>) {
    let mut table = Table::new();
    table.add_row(Row::new(vec![
        Cell::new("File Path"),
        Cell::new("Language"),
        Cell::new("Summary"),
    ]));

    for (file_path, (summary, language)) in index {
        table.add_row(Row::new(vec![
            Cell::new(file_path),
            Cell::new(language),
            Cell::new(summary),
        ]));
    }

    table.printstd();
}
