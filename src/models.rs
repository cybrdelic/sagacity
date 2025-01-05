use std::collections::HashMap;

/// Represents a file node with progress and status.
#[derive(Debug)]
pub struct TreeNode {
    pub filename: String,
    pub progress: f32,
    pub status: String,
}

impl TreeNode {
    pub fn new(filename: String) -> Self {
        Self {
            filename,
            progress: 0.0,
            status: "pending".into(),
        }
    }
}

/// Manages log entries.
#[derive(Debug)]
pub struct LogPanel {
    pub entries: Vec<String>,
    pub visible: bool,
}

impl LogPanel {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            visible: true,
        }
    }

    pub fn add(&mut self, msg: impl Into<String>) {
        self.entries.push(msg.into());
        if self.entries.len() > 200 {
            self.entries.remove(0);
        }
    }
}

/// Handles chatbot-related data and indexing.
#[derive(Debug)]
pub struct Chatbot {
    pub index: HashMap<String, (String, String)>,
    pub api_key: String,
}

impl Chatbot {
    pub fn new(api_key: String) -> Self {
        Self {
            index: HashMap::new(),
            api_key,
        }
    }
}
