use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::{
    App,
    AppScreen,
    indexing_view::indexing_task,
    splash_screen::SplashScreenAction,
    test_view::run_tests,
};

async fn handle_splash_input(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    app_arc: Arc<Mutex<App>>,
) -> Result<bool, Box<dyn Error + Send + Sync>> {
    if let Some(action) = app.splash_screen.handle_input(key) {
        match action {
            SplashScreenAction::Quit => return Ok(true),
            SplashScreenAction::StartChat => {
                app.screen = AppScreen::Indexing;
                let clone = app_arc.clone();
                tokio::spawn(async move {
                    indexing_task(clone).await;
                });
            }
            SplashScreenAction::DbDetails => {
                app.screen = AppScreen::DBDetails;
            }
            SplashScreenAction::RunTests => {
                app.screen = AppScreen::Tests;
                let clone = app_arc.clone();
                tokio::spawn(async move {
                    run_tests(clone).await;
                });
            }
        }
    }
    Ok(false)
}