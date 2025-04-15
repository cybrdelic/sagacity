use std::collections::HashMap;

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

#[derive(Debug, Clone)]
pub struct ContextEntry {
    pub file_path: String,
    pub summary: String,
    pub language: String,
    pub relevance_score: f32,
    pub in_context: bool,
    pub last_used: std::time::SystemTime,
}

impl ContextEntry {
    pub fn new(file_path: String, summary: String, language: String) -> Self {
        Self {
            file_path,
            summary,
            language,
            relevance_score: 0.0,
            in_context: true,
            last_used: std::time::SystemTime::now(),
        }
    }
}

#[derive(Debug)]
pub struct Chatbot {
    pub index: std::collections::HashMap<String, (String, String)>,
    pub context_entries: Vec<ContextEntry>,
    pub api_key: String,
    pub max_context_files: usize,
}

impl Chatbot {
    pub fn new(api_key: String) -> Self {
        Self {
            index: std::collections::HashMap::new(),
            context_entries: Vec::new(),
            api_key,
            max_context_files: 10, // Default to 10 files at most in context
        }
    }
    
    pub fn update_context_from_index(&mut self) {
        // Convert the index to context entries
        self.context_entries = self.index.iter()
            .map(|(path, (summary, language))| {
                ContextEntry::new(path.clone(), summary.clone(), language.clone())
            })
            .collect();
        
        // Sort by file path for initial display
        self.context_entries.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    }
    
    pub fn update_relevance_scores(&mut self, query: &str) {
        // Simple relevance scoring: check if query terms match in file path or summary
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split_whitespace().collect();
        
        for entry in &mut self.context_entries {
            let path_lower = entry.file_path.to_lowercase();
            let summary_lower = entry.summary.to_lowercase();
            
            // Initialize score
            let mut score = 0.0;
            
            // Check file path matches (weighted more)
            for term in &query_terms {
                if path_lower.contains(term) {
                    score += 0.5;
                }
            }
            
            // Check summary matches
            for term in &query_terms {
                if summary_lower.contains(term) {
                    score += 0.3;
                }
            }
            
            // Boost Rust files a bit (application code likely more relevant)
            if entry.language == "rust" {
                score += 0.1;
            }
            
            // Set the score
            entry.relevance_score = score;
        }
        
        // Sort by relevance score (highest first)
        self.context_entries.sort_by(|a, b| {
            b.relevance_score.partial_cmp(&a.relevance_score).unwrap_or(std::cmp::Ordering::Equal)
        });
        
        // Mark only the top N as in context
        for (i, entry) in self.context_entries.iter_mut().enumerate() {
            entry.in_context = i < self.max_context_files && entry.relevance_score > 0.0;
            if entry.in_context {
                entry.last_used = std::time::SystemTime::now();
            }
        }
    }
    
    pub fn get_context_string(&self) -> String {
        let mut context = String::new();
        
        for entry in &self.context_entries {
            if entry.in_context {
                context.push_str(&format!(
                    "File: {}\nSummary: {}\n\n", 
                    entry.file_path, 
                    entry.summary
                ));
            }
        }
        
        context
    }
    
    pub fn toggle_file_in_context(&mut self, index: usize) {
        if index < self.context_entries.len() {
            self.context_entries[index].in_context = !self.context_entries[index].in_context;
            
            if self.context_entries[index].in_context {
                self.context_entries[index].last_used = std::time::SystemTime::now();
            }
        }
    }
}
