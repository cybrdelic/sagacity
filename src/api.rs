// src/api.rs

use crate::constants::*;
use crate::models::{ApiCallLog, Chatbot, Message};
use chrono::Utc;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::error::Error;
use std::fs;

/// Summarizes the content of a file using the Claude API.
///
/// # Arguments
///
/// * `content` - The content of the file to summarize.
/// * `api_key` - Your Claude API key.
/// * `language` - The programming language of the file.
/// * `chatbot` - Mutable reference to the Chatbot instance for logging.
///
/// # Returns
///
/// * `Result<String, Box<dyn Error>>` - The summary of the content or an error.
pub async fn summarize_with_claude(
    content: &str,
    api_key: &str,
    language: &str,
    chatbot: &mut Chatbot,
) -> Result<String, Box<dyn Error>> {
    debug_print!("Summarizing content with Claude");
    let client = Client::new();
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

    // Extract status before consuming the response
    let status = response.status();

    // Log the API call
    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "summarize_with_claude".to_string(),
        response_status: status.as_u16(),
        response_time_ms: elapsed_time,
    });

    debug_print!("Response status: {}", status);

    if !status.is_success() {
        // Consume response to get error body
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error body".to_string());
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    // It's safe to consume `response` here since we've already extracted `status`.
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
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

/// Generates a response from the LLM using the Claude API.
///
/// # Arguments
///
/// * `context` - The context based on relevant files and previous conversation.
/// * `api_key` - Your Claude API key.
/// * `conversation_history` - Slice of previous messages in the conversation.
/// * `user_query` - The current query from the user.
/// * `chatbot` - Mutable reference to the Chatbot instance for logging.
///
/// # Returns
///
/// * `Result<(String, bool), Box<dyn Error>>` - The AI's response and a boolean indicating completion or not.
pub async fn generate_llm_response(
    context: &str,
    api_key: &str,
    conversation_history: &[Message],
    user_query: &str,
    chatbot: &mut Chatbot,
) -> Result<(String, bool), Box<dyn Error>> {
    debug_print!("Generating LLM response");
    let client = Client::new();

    // Prepare conversation messages
    let messages: Vec<serde_json::Value> = conversation_history
        .iter()
        .map(|m| {
            json!({
                "role": m.role,
                "content": m.content
            })
        })
        .collect();

    // Add the current context and user query as a system message
    let system_message = json!({
        "role": "system",
        "content": format!(
            "Based on the following context about a codebase and our previous conversation, please answer the user's query:\n\nContext: {}\n\nUser query: {}",
            context, user_query
        )
    });

    let mut all_messages = messages.clone();
    all_messages.push(system_message);

    let start_time = std::time::Instant::now();

    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": DEFAULT_MODEL,
            "messages": all_messages,
            "system": "You are an AI assistant helping with a codebase. Use the provided context and conversation history to answer questions.",
            "max_tokens": DEFAULT_MAX_TOKENS
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to send request to Claude API: {}", e))?;

    let elapsed_time = start_time.elapsed().as_millis();

    // Extract status before consuming the response
    let status = response.status();

    // Log the API call
    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "generate_llm_response".to_string(),
        response_status: status.as_u16(),
        response_time_ms: elapsed_time,
    });

    debug_print!("Response status: {}", status);

    if !status.is_success() {
        // Consume response to get error body
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error body".to_string());
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    // It's safe to consume `response` here since we've already extracted `status`.
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;
    let answer = body["content"][0]["text"]
        .as_str()
        .ok_or_else(|| {
            debug_print!("Missing 'text' field in API response: {:?}", body);
            "Missing 'text' field in API response"
        })?
        .trim()
        .to_string();

    let is_complete = body["stop_reason"].is_string() && body["stop_reason"] == "stop_sequence";

    Ok((answer, is_complete))
}

/// Searches the index based on the user query using the Claude API.
///
/// # Arguments
///
/// * `index` - Reference to the codebase index mapping file paths to summaries and languages.
/// * `query` - The user's search query.
/// * `api_key` - Your Claude API key.
/// * `chatbot` - Mutable reference to the Chatbot instance for logging.
///
/// # Returns
///
/// * `Result<Vec<(String, f32)>, Box<dyn Error>>` - A vector of tuples containing file paths and their relevance scores.
pub async fn search_index(
    index: &HashMap<String, (String, String)>,
    query: &str,
    api_key: &str,
    chatbot: &mut Chatbot,
) -> Result<Vec<(String, f32)>, Box<dyn Error>> {
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

    let client = Client::new();
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

    // Extract status before consuming the response
    let status = response.status();

    // Log the API call
    chatbot.api_call_logs.push(ApiCallLog {
        timestamp: Utc::now(),
        endpoint: CLAUDE_API_URL.to_string(),
        request_summary: "search_index".to_string(),
        response_status: status.as_u16(),
        response_time_ms: elapsed_time,
    });

    debug_print!("Response status: {}", status);

    if !status.is_success() {
        // Consume response to get error body
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error body".to_string());
        debug_print!("Error response body: {}", error_body);
        return Err(format!("Claude API request failed: {} - {}", status, error_body).into());
    }

    // It's safe to consume `response` here since we've already extracted `status`.
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    let response_text = body["content"][0]["text"]
        .as_str()
        .ok_or("Missing 'text' field in API response")?
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

    relevant_files.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    relevant_files.truncate(5); // Limit to top 5 most relevant files
    Ok(relevant_files)
}

/// Generates the context string for the LLM based on relevant files and user query.
///
/// # Arguments
///
/// * `relevant_files` - A slice of tuples containing file paths and their languages.
/// * `user_query` - The user's query.
///
/// # Returns
///
/// * `Result<String, Box<dyn Error>>` - The formatted context string or an error.
fn generate_context(
    relevant_files: &[(String, String)],
    user_query: &str,
) -> Result<String, Box<dyn Error>> {
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
