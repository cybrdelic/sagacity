// src/conversation.rs

use crate::models::{Chatbot, Message};
use serde_json::json;
use std::fs;

/// Loads conversation history from a JSON file into the Chatbot.
pub fn load_conversation(chatbot: &mut Chatbot) -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(json_str) = fs::read_to_string("conversation_history.json") {
        let history: Vec<Message> = serde_json::from_str(&json_str)?;
        chatbot.memory = history;
        println!("Conversation history loaded successfully.");
    } else {
        println!("No existing conversation history found.");
    }
    Ok(())
}

/// Saves the current conversation history from the Chatbot to a JSON file.
pub fn save_conversation(chatbot: &Chatbot) -> Result<(), Box<dyn std::error::Error>> {
    let json_str = serde_json::to_string_pretty(&chatbot.memory)?;
    fs::write("conversation_history.json", json_str)?;
    println!("Conversation history saved successfully.");
    Ok(())
}
