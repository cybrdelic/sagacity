use colored::Colorize;
use home::home_dir;
use std::path::PathBuf;

use crate::ui::chat::Message;
use crate::ui::directory_tree::DirectoryTree;

// src/app.rs or within your main App module

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    MainMenu,
    Chat,
    BrowseIndex,
    GitHubRecommendations,
    Help,
    Settings,
    QuitConfirm,
    Quit,
    SelectCodebase, // New state for codebase selection
}

pub struct App {
    pub state: AppState,
    pub menu_items: Vec<&'static str>,
    pub selected_menu_item: usize,
    pub messages: Vec<Message>,
    pub input: String,
    // Add fields for directory tree navigation
    pub dir_tree: DirectoryTree,
}

impl App {
    pub fn new() -> App {
        App {
            state: AppState::MainMenu,
            menu_items: vec![
                "💬 Chat with any codebase in ~/",
                "💬 Chat with CWD",
                "💬 Chat with GitHub Repo",
                "📂 Browse Index",
                "🔍 Browse GitHub Recommendations",
                "❓ Help",
                "⚙️ Settings",
                "🚪 Quit",
            ],
            selected_menu_item: 0,
            messages: Vec::new(),
            input: String::new(),
            dir_tree: DirectoryTree::new(home_dir().unwrap_or(PathBuf::from("/"))),
        }
    }
}
