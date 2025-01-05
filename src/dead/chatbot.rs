use crate::constants::*;
use chrono::{DateTime, Utc};
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

// Update the debug_print macro to write to a log file instead of stderr
macro_rules! debug_print {
    ($($arg:tt)*) => {{
        use std::fs::OpenOptions;
        use std::io::Write;
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("sagacity-debug.log")
        {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            writeln!(file, "[{}] {}", timestamp, format!($($arg)*)).ok();
        }
    }};
}
#[derive(Debug)]
pub struct ApiCallLog {
    pub timestamp: DateTime<Utc>,
    pub endpoint: String,
    pub request_summary: String,
    pub response_status: u16,
    pub response_time_ms: u128,
}

#[derive(Serialize, Deserialize)]
pub struct IndexCache {
    pub timestamp: u64,
    pub last_modification: u64,
    pub index: HashMap<String, (String, String)>,
    pub file_mod_times: HashMap<String, u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub timestamp: DateTime<Utc>,
}

pub struct ConversationSession {
    pub name: String,
    pub index: HashMap<String, (String, String)>,
    pub memory: Vec<Message>,
}

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

        // Create internal progress bar
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.set_message("Processing query...");
        pb.enable_steady_tick(std::time::Duration::from_millis(120));

        // Clone what we need before the mutable borrow
        let index_clone = self.index.clone();
        let api_key_clone = self.api_key.clone();
        let memory_clone = self.memory.clone();

        // Now use the cloned values
        pb.set_message("Searching relevant files...");
        let relevant_files = search_index(&index_clone, user_query, &api_key_clone, self).await?;

        let relevant_file_info: Vec<(String, String)> = relevant_files
            .into_iter()
            .filter_map(|(file, _)| {
                self.index
                    .get(&file)
                    .map(|(_, lang)| (file.clone(), lang.clone()))
            })
            .collect();

        if relevant_file_info.is_empty() {
            pb.finish_and_clear();
            return Err("No relevant files found in the index for the given query.".into());
        }

        pb.set_message("Preparing context...");
        let context = prepare_context(&relevant_file_info, user_query)?;

        pb.set_message("Generating response...");
        let (response, _) =
            generate_llm_response(&context, &api_key_clone, &memory_clone, user_query, self)
                .await?;

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

        pb.finish_and_clear();
        Ok(response)
    }
}

// Exporting these functions so main can use them
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

pub async fn index_codebase(
    root_dir: &str,
    api_key: &str,
    pb: &ProgressBar,
    chatbot: &mut Chatbot,
) -> Result<
    (HashMap<String, (String, String)>, u64, HashMap<String, u64>),
    Box<dyn std::error::Error>,
> {
    debug_print!("Starting recursive indexing from: {}", root_dir);

    let mut index = chatbot.index.clone();
    let mut file_mod_times = chatbot.file_mod_times.clone();

    // Configure recursive walker
    let walker = WalkBuilder::new(root_dir)
        .hidden(false) // Don't skip hidden files
        .ignore(false) // Don't use .gitignore rules
        .git_ignore(false) // Don't ignore .git directories
        .git_global(false)
        .git_exclude(false)
        .build();

    // Collect all files with relevant extensions
    let files: Vec<String> = walker
        .filter_map(|entry| {
            if let Ok(entry) = entry {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    let path = entry.path().to_string_lossy().to_string();
                    let extension = entry.path().extension().and_then(|e| e.to_str());

                    // Add more file extensions here
                    if matches!(
                        extension,
                        Some("rs")
                            | Some("toml")
                            | Some("py")
                            | Some("go")
                            | Some("js")
                            | Some("jsx")
                            | Some("ts")
                            | Some("tsx")
                            | Some("html")
                            | Some("css")
                            | Some("yaml")
                            | Some("yml")
                            | Some("sh")
                    ) {
                        debug_print!("Found file: {}", path);
                        return Some(path);
                    }
                }
            }
            None
        })
        .collect();

    debug_print!("Found {} files to process", files.len());
    pb.set_length(files.len() as u64);

    // Process each file
    for (i, file_path) in files.iter().enumerate() {
        pb.set_message(format!(
            "Processing file {}/{}: {}",
            i + 1,
            files.len(),
            file_path
        ));

        let metadata = fs::metadata(&file_path)?;
        let modified = metadata.modified()?;
        let modified_secs = modified.duration_since(UNIX_EPOCH)?.as_secs();

        let needs_reindex = match file_mod_times.get(file_path) {
            Some(&cached_time) => modified_secs > cached_time,
            None => true,
        };

        if needs_reindex {
            debug_print!("Indexing: {}", file_path);
            if let Ok(content) = fs::read_to_string(&file_path) {
                let language = detect_language(&file_path);
                let summary =
                    match summarize_with_claude(&content, api_key, &language, chatbot).await {
                        Ok(summary) => summary,
                        Err(e) => {
                            debug_print!("Error summarizing {}: {}", file_path, e);
                            format!(
                                "Content preview: {}",
                                &content[..std::cmp::min(content.len(), 100)]
                            )
                        }
                    };

                index.insert(file_path.clone(), (summary, language));
                file_mod_times.insert(file_path.clone(), modified_secs);
                debug_print!("Successfully indexed: {}", file_path);
            }
        } else {
            debug_print!("Using cached index for: {}", file_path);
        }

        pb.inc(1);
    }

    debug_print!("Finished indexing {} files", index.len());
    save_index_cache(
        &index,
        file_mod_times.values().max().copied().unwrap_or(0),
        &file_mod_times,
    )?;

    Ok((index, 0, file_mod_times))
}
// 5. Implement save_index_cache in chatbot.rs
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
pub async fn summarize_with_claude(
    content: &str,
    api_key: &str,
    language: &str,
    chatbot: &mut Chatbot,
) -> Result<String, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let prompt = format!(
        "Provide a very concise summary (2-3 sentences max) of the following {} code:\n\n{}",
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
        .await?;

    let status = response.status();
    let elapsed_time = start_time.elapsed().as_millis();

    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "summarize_with_claude".to_string(),
        response_status: status.as_u16(),
        response_time_ms: elapsed_time,
    });

    if !status.is_success() {
        let error_body = response.text().await?;
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    let body: Value = response.json().await?;
    let summary = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_string();

    if summary.is_empty() {
        return Err("Empty summary received from Claude API".into());
    }
    Ok(summary)
}

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

// Search index
// Fix search_index function
pub async fn search_index(
    index: &HashMap<String, (String, String)>,
    query: &str,
    api_key: &str,
    chatbot: &mut Chatbot,
) -> Result<Vec<(String, f32)>, Box<dyn std::error::Error>> {
    let mut prompt = format!(
        "Score relevance of each summary (0 to 1) to query:\nQuery: {}\n\n",
        query
    );
    for (file, (summary, _)) in index {
        prompt.push_str(&format!("{}: {}\n\n", file, summary));
    }
    prompt.push_str("Format:\nfile_path,relevance\n");

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
                {"role": "user", "content": prompt}
            ],
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await?;

    let status = response.status();
    let elapsed_time = start_time.elapsed().as_millis();

    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "search_index".to_string(),
        response_status: status.as_u16(),
        response_time_ms: elapsed_time,
    });

    if !status.is_success() {
        let error_body = response.text().await?;
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    let body: Value = response.json().await?;
    let response_text = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field")?
        .trim()
        .to_string();

    let mut relevant_files = Vec::new();
    for line in response_text.lines() {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() == 2 {
            let file = parts[0].trim().to_string();
            let relevance: f32 = parts[1].trim().parse().unwrap_or(0.0);
            relevant_files.push((file, relevance));
        }
    }

    relevant_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    relevant_files.truncate(5);
    Ok(relevant_files)
}

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
// Fix generate_llm_response function
pub async fn generate_llm_response(
    context: &str,
    api_key: &str,
    conversation_history: &Vec<Message>,
    user_query: &str,
    chatbot: &mut Chatbot,
) -> Result<(String, bool), Box<dyn std::error::Error>> {
    // log all input data for debugging
    debug_print!("=== generate_llm_response called ===");
    debug_print!("context:\n{}", context);
    debug_print!("user_query: {}", user_query);
    debug_print!(
        "conversation_history (len={}): {:#?}",
        conversation_history.len(),
        conversation_history
    );

    let client = reqwest::Client::new();
    let mut messages: Vec<Value> = conversation_history
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();

    messages.push(json!({
        "role": "user",
        "content": format!(
            "based on the following context about a codebase and our previous conversation, please answer the user's query:\n\ncontext: {}\n\nuser query: {}",
            context,
            user_query
        )
    }));

    let start_time = std::time::Instant::now();
    let response = client
        .post(CLAUDE_API_URL)
        .header("content-type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": DEFAULT_MODEL,
            "messages": messages,
            "system": "you are an ai assistant helping with a codebase. use the provided context and conversation history to answer questions.",
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await?;

    let status = response.status();
    let elapsed_time = start_time.elapsed().as_millis();

    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "generate_llm_response".to_string(),
        response_status: status.as_u16(),
        response_time_ms: elapsed_time,
    });

    if !status.is_success() {
        let error_body = response.text().await?;
        debug_print!(
            "generate_llm_response: error from claude api status={} body:\n{}",
            status,
            error_body
        );
        return Err(format!("claude api request failed: {} - {}", status, error_body).into());
    }

    let body: Value = response.json().await?;
    debug_print!("generate_llm_response: raw api json response:\n{:#}", body);

    let answer = body["content"][0]["text"]
        .as_str()
        .ok_or("missing 'text' field in claude response")?
        .trim()
        .to_string();

    // log final answer
    debug_print!("generate_llm_response: final answer:\n{}", answer);

    let is_complete = !body["stop_reason"].is_null() && body["stop_reason"] == "stop_sequence";
    debug_print!("=== generate_llm_response returning ===\n\n");

    Ok((answer, is_complete))
}
