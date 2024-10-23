use crate::ui::chat::Message;

/// Represents the different states of the application
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
}

pub struct App {
    pub state: AppState,
    pub menu_items: Vec<&'static str>,
    pub selected_menu_item: usize,
    pub messages: Vec<Message>,
    pub input: String,
}

impl App {
    pub fn new() -> App {
        App {
            state: AppState::MainMenu,
            menu_items: vec![
                "💬 Chat",
                "📂 Browse Index",
                "🔍 GitHub Recommendations",
                "❓ Help",
                "⚙️ Settings",
                "🚪 Quit",
            ],
            selected_menu_item: 0,
            messages: Vec::new(),
            input: String::new(),
        }
    }
}
