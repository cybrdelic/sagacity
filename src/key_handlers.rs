use crate::ui::chat::{Message, Sender};
use crate::AppState;
use crate::{ui, App};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time::sleep;

pub async fn handle_chat_input<B: ratatui::backend::Backend>(
    key: KeyEvent,
    app: &mut App,
    terminal: &mut Terminal<B>,
) -> Result<(), Box<dyn Error>> {
    match key.code {
        KeyCode::Esc => {
            app.state = AppState::MainMenu;
        }
        KeyCode::Enter => {
            let user_message = app.input.drain(..).collect::<String>();
            if !user_message.trim().is_empty() {
                // Add user message to chat history
                app.messages.push(Message {
                    sender: Sender::User,
                    content: user_message.clone(),
                });

                // Start processing state
                app.is_processing = true;
                app.processing_frame = 0;
                app.last_frame_update = Instant::now();

                // Use a loop to maintain animation while processing
                if let Some(ref mut chatbot) = app.chatbot {
                    let mut last_redraw = Instant::now();
                    let response = {
                        // Process in a loop while maintaining animation
                        let chat_response = chatbot.chat(&user_message).await;

                        // Update UI while waiting for response
                        loop {
                            // Update animation frame
                            app.update_processing_animation();

                            // Redraw if needed
                            if last_redraw.elapsed() >= Duration::from_millis(50) {
                                terminal.draw(|f| ui(f, app))?;
                                last_redraw = Instant::now();
                            }

                            // Give some time to other tasks
                            sleep(Duration::from_millis(10)).await;

                            // Break when we have a response
                            match &chat_response {
                                Ok(response) => break response.clone(),
                                Err(e) => break format!("Error: {}", e),
                            }
                        }
                    };

                    // Add AI response to chat history
                    app.messages.push(Message {
                        sender: Sender::AI,
                        content: response,
                    });
                } else {
                    app.messages.push(Message {
                        sender: Sender::AI,
                        content: "Error: Chatbot not initialized".to_string(),
                    });
                }

                // Clear processing state
                app.is_processing = false;

                // Auto-scroll to bottom when new messages arrive
                app.scroll = app.messages.len();
            }
        }
        KeyCode::PageUp => app.scroll_up(),
        KeyCode::PageDown => app.scroll_down(),
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c {
                    'c' => app.state = AppState::QuitConfirm,
                    'u' => app.scroll_up(),
                    'd' => app.scroll_down(),
                    _ => {}
                }
            } else {
                app.input.push(c);
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn handle_quit_confirm_input(key: KeyEvent, app: &mut App) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => {
            app.state = AppState::Quit;
        }
        KeyCode::Char('n') | KeyCode::Esc => {
            app.state = AppState::MainMenu;
        }
        _ => {}
    }
}
