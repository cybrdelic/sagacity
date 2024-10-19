// src/models.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a GitHub repository.
#[derive(Debug, Deserialize)]
pub struct GitHubRepo {
    pub full_name: String,
    pub clone_url: String,
}

/// Represents a message in the conversation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String, // "user" or "assistant"
    pub content: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
}

/// Logs details of each API call.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiCallLog {
    pub timestamp: DateTime<Utc>,
    pub endpoint: String,
    pub request_summary: String,
    pub response_status: u16,
    pub response_time_ms: u128,
}

/// Caches the index to avoid reprocessing unchanged files.
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexCache {
    pub timestamp: u64,
    pub last_modification: u64,
    pub index: HashMap<String, (String, String)>, // file_path -> (summary, language)
    pub file_mod_times: HashMap<String, u64>,     // file_path -> last_modified_time
}

/// Represents a conversation session.
#[derive(Debug)]
pub struct ConversationSession {
    pub name: String,
    pub index: HashMap<String, (String, String)>,
    pub memory: Vec<Message>,
}

/// Core chatbot structure managing the conversation, index, and API interactions.
#[derive(Debug)]
pub struct Chatbot {
    pub index: HashMap<String, (String, String)>,
    pub api_key: String,
    pub memory: Vec<Message>,
    pub sessions: Vec<ConversationSession>,
    pub current_session: Option<usize>,
    pub api_call_logs: Vec<ApiCallLog>,
    pub file_mod_times: HashMap<String, u64>,
}

impl Chatbot {
    /// Creates a new Chatbot instance.
    pub fn new(
        index: HashMap<String, (String, String)>,
        file_mod_times: HashMap<String, u64>,
        api_key: String,
    ) -> Self {
        Chatbot {
            index,
            api_key,
            memory: Vec::new(),
            sessions: Vec::new(),
            current_session: None,
            api_call_logs: Vec::new(),
            file_mod_times,
        }
    }

    /// Creates a new conversation session.
    pub fn create_session(&mut self, name: String, index: HashMap<String, (String, String)>) {
        let session = ConversationSession {
            name,
            index,
            memory: Vec::new(),
        };
        self.sessions.push(session);
        self.current_session = Some(self.sessions.len() - 1);
    }
}
