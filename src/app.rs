use crate::chatbot::Chatbot;
use crate::ui::chat::Message;
use crate::ui::directory_tree::DirectoryTree;
use home::home_dir;
use std::path::PathBuf;
use std::time::Instant;

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
    SelectCodebase,
    Indexing,
}

pub struct App {
    pub state: AppState,
    pub menu_items: Vec<&'static str>,
    pub selected_menu_item: usize,
    pub messages: Vec<Message>,
    pub input: String,
    pub dir_tree: DirectoryTree,
    pub selected_codebase: Option<PathBuf>,
    pub chatbot: Option<Chatbot>,
    pub scroll: usize,
    pub is_processing: bool,
    pub processing_frame: usize,
    pub last_frame_update: Instant,
}

impl App {
    pub fn new() -> App {
        let home_directory = home_dir().unwrap_or(PathBuf::from("/"));

        App {
            state: AppState::MainMenu,
            menu_items: vec!["💬 Chat with CWD", "❓ Help", "⚙️ Settings", "🚪 Quit"],
            selected_menu_item: 0,
            messages: Vec::new(),
            input: String::new(),
            dir_tree: DirectoryTree::new(home_directory),
            selected_codebase: None,
            chatbot: None,
            scroll: 0,
            is_processing: false,
            processing_frame: 0,
            last_frame_update: Instant::now(),
        }
    }

    pub fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll -= 1;
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll += 1;
    }

    pub fn update_processing_animation(&mut self) {
        if self.is_processing
            && self.last_frame_update.elapsed() >= std::time::Duration::from_millis(80)
        {
            self.processing_frame = (self.processing_frame + 1) % 10;
            self.last_frame_update = Instant::now();
        }
    }
}
