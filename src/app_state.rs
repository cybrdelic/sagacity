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
