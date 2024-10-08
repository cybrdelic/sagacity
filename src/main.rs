use reqwest;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use walkdir::WalkDir;

use futures::future::join_all;
use std::env;
use std::time::Instant;

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01"; // Add this line

// Add a debug macro for easier logging
macro_rules! debug_print {
    ($($arg:tt)*) => {
        eprintln!("[DEBUG] {}", format!($($arg)*));
    };
}

fn get_claude_api_key() -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Getting Claude API key");
    let home_dir = env::var("HOME")?;
    let zshrc_path = format!("{}/.zshrc", home_dir);
    debug_print!("Reading .zshrc from: {}", zshrc_path);
    let zshrc_content =
        fs::read_to_string(&zshrc_path).map_err(|e| format!("Failed to read .zshrc: {}", e))?;

    for line in zshrc_content.lines() {
        if line.starts_with("export ANTHROPIC_API_KEY=") {
            let key = line
                .split('=')
                .nth(1)
                .ok_or("Invalid ANTHROPIC_API_KEY format")?
                .trim_matches('"')
                .to_string();
            debug_print!("API key found");
            return Ok(key);
        }
    }

    Err("ANTHROPIC_API_KEY not found in .zshrc".into())
}

fn scan_codebase(root_dir: &str) -> Vec<String> {
    WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            let extension = entry.path().extension().and_then(|e| e.to_str());
            matches!(extension, Some("rs") | Some("toml") | Some("md"))
        })
        .map(|entry| entry.path().to_string_lossy().to_string())
        .collect()
}

fn read_file_contents(file_path: &str) -> Result<String, std::io::Error> {
    fs::read_to_string(file_path)
}

async fn summarize_with_claude(
    content: &str,
    api_key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Summarizing content with Claude");
    let client = reqwest::Client::new();
    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION) // Add this line
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": [
                {
                    "role": "user",
                    "content": format!("Provide a very concise summary (2-3 sentences max) of the following code, focusing on its main purpose and key functionalities:\n\n{}", content)
                }
            ],
            "max_tokens": 150
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

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

async fn index_codebase(
    root_dir: &str,
    api_key: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    debug_print!("Indexing codebase in directory: {}", root_dir);
    let mut index = HashMap::new();
    let files = scan_codebase(root_dir);
    debug_print!("Found {} files to index", files.len());

    for (i, file_path) in files.iter().enumerate() {
        debug_print!("Processing file {}/{}: {}", i + 1, files.len(), file_path);
        let content = read_file_contents(&file_path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;
        debug_print!("File content length: {} characters", content.len());

        let start = Instant::now();
        let summary = match summarize_with_claude(&content, api_key).await {
            Ok(summary) => summary,
            Err(e) => {
                debug_print!("Error summarizing file {}: {}", file_path, e);
                format!(
                    "Failed to summarize. File content preview: {}",
                    &content[..std::cmp::min(content.len(), 100)]
                )
            }
        };
        let duration = start.elapsed();
        debug_print!("Summarization took {:?}", duration);

        index.insert(file_path.clone(), summary);
    }

    debug_print!("Indexing complete. Total files indexed: {}", index.len());
    Ok(index)
}

fn search_index(index: &HashMap<String, String>, query: &str) -> Vec<(String, String)> {
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();
    index
        .iter()
        .filter(|(_, summary)| {
            let summary_lower = summary.to_lowercase();
            query_words.iter().any(|&word| summary_lower.contains(word))
        })
        .map(|(file, summary)| (file.clone(), summary.clone()))
        .collect()
}

async fn chat_with_system(
    index: &HashMap<String, String>,
    api_key: &str,
    user_query: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Starting chat with system");

    // Step 1: Find relevant files
    let relevant_files = search_index(index, user_query);

    // Step 2: Prepare context for the LLM
    let context = prepare_context(&relevant_files, user_query);

    // Step 3: Generate response using the LLM
    let response = generate_llm_response(&context, api_key).await?;

    Ok(response)
}

fn prepare_context(relevant_files: &[(String, String)], user_query: &str) -> String {
    let mut context = format!(
        "User query: {}\n\nRelevant files and summaries:\n",
        user_query
    );
    for (file, summary) in relevant_files {
        context.push_str(&format!("File: {}\nSummary: {}\n\n", file, summary));
    }
    context
}

async fn generate_llm_response(
    context: &str,
    api_key: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    debug_print!("Generating LLM response");
    let client = reqwest::Client::new();
    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": "claude-3-sonnet-20240229",
            "messages": [
                {
                    "role": "user",
                    "content": format!("Based on the following context about a codebase, please answer the user's query:\n\n{}", context)
                }
            ],
            "max_tokens": 500
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    let answer = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
        .trim()
        .to_string();

    Ok(answer)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    debug_print!("Starting application");
    let root_dir = "."; // Current directory
    debug_print!("Root directory: {}", root_dir);

    let api_key = get_claude_api_key()?;
    debug_print!("API key retrieved successfully");

    let start = Instant::now();
    let index = index_codebase(root_dir, &api_key).await?;
    let duration = start.elapsed();
    debug_print!("Indexing completed in {:?}", duration);

    println!("Codebase indexed successfully. You can now ask questions about the codebase.");
    debug_print!("Number of indexed files: {}", index.len());

    loop {
        println!("Enter your query ('print index' to see all entries, 'chat' to start a chat session, or 'quit' to exit):");
        let mut query = String::new();
        std::io::stdin().read_line(&mut query)?;
        let query = query.trim();
        debug_print!("User query: {}", query);

        match query.to_lowercase().as_str() {
            "quit" => {
                debug_print!("Exiting application");
                break;
            }
            "print index" => {
                println!("Full index:");
                for (file, summary) in &index {
                    println!("File: {}\nSummary: {}\n", file, summary);
                }
            }
            "chat" => {
                println!("Starting chat session. Type 'exit' to end the chat.");
                loop {
                    println!("Chat: Enter your question about the codebase:");
                    let mut chat_query = String::new();
                    std::io::stdin().read_line(&mut chat_query)?;
                    let chat_query = chat_query.trim();

                    if chat_query.to_lowercase() == "exit" {
                        println!("Ending chat session.");
                        break;
                    }

                    match chat_with_system(&index, &api_key, chat_query).await {
                        Ok(response) => println!("System: {}", response),
                        Err(e) => println!("Error: {}", e),
                    }
                }
            }
            _ => {
                let results = search_index(&index, query);
                debug_print!("Search results count: {}", results.len());
                if results.is_empty() {
                    println!("No results found for your query.");
                } else {
                    println!("Search results:");
                    for (file, summary) in results {
                        println!("File: {}\nSummary: {}\n", file, summary);
                    }
                }
            }
        }
    }

    Ok(())
}
