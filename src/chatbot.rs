// chatbot.rs

use crate::constants::*;
use chrono::{DateTime, Utc};
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

// Debug macro for easier logging
macro_rules! debug_print {
    ($($arg:tt)*) => {
        eprintln!("[DEBUG] {}", format!($($arg)*));
    };
}

// Struct to log API calls
#[derive(Debug)]
pub struct ApiCallLog {
    pub timestamp: DateTime<Utc>,
    pub endpoint: String,
    pub request_summary: String,
    pub response_status: u16,
    pub response_time_ms: u128,
}

// Struct for indexing cache
#[derive(Serialize, Deserialize)]
pub struct IndexCache {
    pub timestamp: u64,
    pub last_modification: u64,
    pub index: HashMap<String, (String, String)>,
    pub file_mod_times: HashMap<String, u64>,
}

// Struct for messages
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
}

// Struct for conversation sessions
pub struct ConversationSession {
    pub name: String,
    pub index: HashMap<String, (String, String)>,
    pub memory: Vec<Message>,
}

// Chatbot struct with API call logs and file modification times
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

    pub fn create_session(&mut self, name: String, index: HashMap<String, (String, String)>) {
        let session = ConversationSession {
            name,
            index,
            memory: Vec::new(),
        };
        self.sessions.push(session);
        self.current_session = Some(self.sessions.len() - 1);
    }

    pub async fn chat(&mut self, user_query: &str) -> Result<String, Box<dyn std::error::Error>> {
        debug_print!("Starting chat with system");

        // Step 1: Find relevant files
        let index_clone = self.index.clone();
        let api_key_clone = self.api_key.clone();
        let relevant_files = search_index(&index_clone, user_query, &api_key_clone, self).await?;

        // Step 2: Extract file paths and languages from relevant_files with proper handling
        let relevant_file_info: Vec<(String, String)> = relevant_files
            .into_iter()
            .filter_map(|(file, _)| {
                match self.index.get(&file) {
                    Some((_, language)) => Some((file.clone(), language.clone())),
                    None => {
                        debug_print!("Warning: File '{}' not found in index.", file);
                        None // Skip files not found in the index
                    }
                }
            })
            .collect();

        // Check if we have any relevant files after filtering
        if relevant_file_info.is_empty() {
            return Err("No relevant files found in the index for the given query.".into());
        }

        // Step 3: Prepare context for the LLM
        let context = prepare_context(&relevant_file_info, user_query)?;

        // Step 4: Generate response using the LLM
        let api_key_clone = self.api_key.clone();
        let memory_clone = self.memory.clone();
        let (response, _) =
            generate_llm_response(&context, &api_key_clone, &memory_clone, user_query, self)
                .await?;

        // Step 5: Update conversation history
        self.memory.push(Message {
            role: "user".to_string(),
            content: user_query.to_string(),
            timestamp: Utc::now(),
        });
        self.memory.push(Message {
            role: "assistant".to_string(),
            content: response.clone(),
            timestamp: Utc::now(),
        });

        Ok(response)
    }
}

// Helper functions related to Chatbot

// Function to summarize content with Claude API
pub async fn summarize_with_claude(
    content: &str,
    api_key: &str,
    language: &str,
    chatbot: &mut Chatbot,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Summarizing content with Claude");
    let client = reqwest::Client::new();
    let prompt = format!(
        "Provide a very concise summary (2-3 sentences max) of the following {} code, focusing on its main purpose and key functionalities:\n\n{}",
        language, content
    );

    let start_time = std::time::Instant::now();

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": DEFAULT_MODEL,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let elapsed_time = start_time.elapsed().as_millis();

    // Log the API call
    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "summarize_with_claude".to_string(),
        response_status: response.status().as_u16(),
        response_time_ms: elapsed_time,
    });

    debug_print!("Response status: {}", response.status());

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error response body: {}", e))?;
        debug_print!("Error response body: {}", error_body);
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    debug_print!(
        "Response body: {}",
        serde_json::to_string_pretty(&body).unwrap()
    );

    let summary = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_string();
    if summary.is_empty() {
        return Err("Empty summary received from Claude API".into());
    }
    debug_print!("Received summary: {}", summary);
    Ok(summary)
}

// Function to load index cache
pub fn load_index_cache() -> Result<Option<IndexCache>, Box<dyn std::error::Error>> {
    if let Ok(contents) = fs::read_to_string("index_cache.json") {
        let cache: IndexCache = serde_json::from_str(&contents)?;
        debug_print!("Index cache loaded successfully.");
        Ok(Some(cache))
    } else {
        debug_print!("No existing index cache found.");
        Ok(None)
    }
}

// Function to save index cache
pub fn save_index_cache(
    index: &HashMap<String, (String, String)>,
    last_modification: u64,
    file_mod_times: &HashMap<String, u64>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cache = IndexCache {
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        last_modification,
        index: index.clone(),
        file_mod_times: file_mod_times.clone(),
    };
    let serialized = serde_json::to_string_pretty(&cache)?;
    fs::write("index_cache.json", serialized)?;
    debug_print!("Index cache saved successfully.");
    Ok(())
}

// Function to index the codebase
pub async fn index_codebase(
    root_dir: &str,
    api_key: &str,
    pb: &indicatif::ProgressBar,
    chatbot: &mut Chatbot,
) -> Result<
    (HashMap<String, (String, String)>, u64, HashMap<String, u64>),
    Box<dyn std::error::Error>,
> {
    let mut index = chatbot.index.clone();
    let mut file_mod_times = chatbot.file_mod_times.clone();

    let walker = ignore::WalkBuilder::new(root_dir)
        .hidden(false)
        .ignore(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .build();

    let files: Vec<String> = walker
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map_or(false, |ft| ft.is_file()))
        .filter(|entry| {
            let extension = entry.path().extension().and_then(|e| e.to_str());
            matches!(
                extension,
                Some("rs") | Some("toml") | Some("md") | Some("py") | Some("go")
            )
        })
        .map(|entry| entry.path().to_string_lossy().to_string())
        .collect();

    pb.set_length(files.len() as u64);

    let mut last_modification = 0;
    let mut files_set = HashSet::new();

    for (i, file_path) in files.iter().enumerate() {
        pb.set_message(format!(
            "Processing file {}/{}: {}",
            i + 1,
            files.len(),
            file_path
        ));

        // Get the last modification time of the file
        let metadata = fs::metadata(&file_path)?;
        let modified = metadata.modified()?;
        let modified_secs = modified.duration_since(UNIX_EPOCH)?.as_secs();
        last_modification = std::cmp::max(last_modification, modified_secs);

        files_set.insert(file_path.clone());

        // Check if the file has been modified since last indexing
        let needs_reindex = match file_mod_times.get(file_path) {
            Some(&cached_mod_time) => modified_secs > cached_mod_time,
            None => true, // New file
        };

        if needs_reindex {
            debug_print!("Re-indexing file: {}", file_path);
            let content = fs::read_to_string(&file_path)
                .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

            let language = detect_language(&file_path);
            let summary = match summarize_with_claude(&content, api_key, &language, chatbot).await {
                Ok(summary) => summary,
                Err(e) => {
                    debug_print!("Error summarizing {}: {}", file_path, e);
                    format!(
                        "Failed to summarize. File content preview: {}",
                        &content[..std::cmp::min(content.len(), 100)]
                    )
                }
            };

            index.insert(file_path.clone(), (summary, language));
            file_mod_times.insert(file_path.clone(), modified_secs); // Update modification time
        } else {
            debug_print!("Skipping file (no changes): {}", file_path);
        }

        pb.inc(1);
    }

    // Remove entries for files that no longer exist
    index.retain(|file_path, _| files_set.contains(file_path));
    file_mod_times.retain(|file_path, _| files_set.contains(file_path));

    pb.finish_with_message(format!(
        "Indexing complete. Total files indexed: {}",
        index.len()
    ));

    // Save the index cache
    save_index_cache(&index, last_modification, &file_mod_times)?;

    Ok((index, last_modification, file_mod_times))
}

// Function to detect programming language based on file extension
pub fn detect_language(file_path: &str) -> String {
    let extension = std::path::Path::new(file_path)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");

    match extension {
        "rs" => "rust",
        "py" => "python",
        "go" => "go",
        "ts" => "typescript",
        "js" => "javascript",
        "java" => "java",
        "c" => "c",
        "cpp" => "cpp",
        _ => "unknown",
    }
    .to_string()
}

// Function to search the index based on a query
pub async fn search_index(
    index: &HashMap<String, (String, String)>,
    query: &str,
    api_key: &str,
    chatbot: &mut Chatbot,
) -> Result<Vec<(String, f32)>, Box<dyn std::error::Error>> {
    let mut prompt = format!(
        "Based on the following query, score the relevance of each summary on a scale of 0 to 1:\n\nQuery: {}\n\n",
        query
    );

    for (file, (summary, _)) in index {
        prompt.push_str(&format!("Summary for {}: {}\n\n", file, summary));
    }

    prompt.push_str(
        "Provide your response in the following format:\n\n<file_path_1>,<relevance_score_1>\n<file_path_2>,<relevance_score_2>\n...\n",
    );

    let client = reqwest::Client::new();
    let start_time = std::time::Instant::now();

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": DEFAULT_MODEL,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let elapsed_time = start_time.elapsed().as_millis();

    // Log the API call
    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "search_index".to_string(),
        response_status: response.status().as_u16(),
        response_time_ms: elapsed_time,
    });

    let status = response.status();
    if !status.is_success() {
        let error_body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read error response body: {}", e))?;
        debug_print!("Error response body: {}", error_body);
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    let body: Value = response.json().await?;
    let response_text = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_string();

    let mut relevant_files = Vec::new();
    for line in response_text.lines() {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() == 2 {
            let file = parts[0].to_string();
            let relevance: f32 = parts[1].parse().unwrap_or(0.0);
            relevant_files.push((file, relevance));
        }
    }

    relevant_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    relevant_files.truncate(5); // Limit to top 5 most relevant files
    Ok(relevant_files)
}

// Function to prepare context for the LLM
pub fn prepare_context(
    relevant_files: &[(String, String)],
    user_query: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut context = format!("User query: {}\n\nRelevant file contents:\n", user_query);
    for (file_path, _) in relevant_files {
        let file_content = fs::read_to_string(file_path)?;
        context.push_str(&format!(
            "File: {}\nContent:\n{}\n\n",
            file_path, file_content
        ));
    }
    Ok(context)
}

// Function to generate LLM response using Claude API
pub async fn generate_llm_response(
    context: &str,
    api_key: &str,
    conversation_history: &Vec<Message>,
    user_query: &str,
    chatbot: &mut Chatbot,
) -> Result<(String, bool), Box<dyn std::error::Error>> {
    debug_print!("Generating LLM response");
    let client = reqwest::Client::new();

    let mut messages: Vec<Value> = conversation_history
        .iter()
        .map(|m| {
            json!({
                "role": m.role,
                "content": m.content
            })
        })
        .collect();

    // Add the current context and user query
    messages.push(json!({
        "role": "user",
        "content": format!("Based on the following context about a codebase and our previous conversation, please answer the user's query:\n\nContext: {}\n\nUser query: {}", context, user_query)
    }));

    let start_time = std::time::Instant::now();

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": DEFAULT_MODEL,
            "messages": messages,
            "system": "You are an AI assistant helping with a codebase. Use the provided context and conversation history to answer questions.",
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let elapsed_time = start_time.elapsed().as_millis();

    // Log the API call
    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "generate_llm_response".to_string(),
        response_status: response.status().as_u16(),
        response_time_ms: elapsed_time,
    });

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    debug_print!("API Response: {:?}", body);

    let answer = body["content"][0]["text"]
        .as_str()
        .ok_or_else(|| {
            debug_print!("Missing 'text' field in API response: {:?}", body);
            "Missing 'text' field in API response"
        })?
        .trim()
        .to_string();

    let is_complete = !body["stop_reason"].is_null() && body["stop_reason"] == "stop_sequence";

    Ok((answer, is_complete))
}

// Function to chat with the system
pub async fn chat_with_system(
    chatbot: &mut Chatbot,
    user_query: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    chatbot.chat(user_query).await
}
