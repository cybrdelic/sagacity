use crate::{
    config::get_config,
    errors::{SagacityError, SagacityResult},
};
use lru::LruCache;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Mutex;

// Constants for API endpoints and versions
pub const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

// Token counter - temporarily commented out until we can set up claude-tokenizer properly
// static TOKENIZER: Lazy<ClaudeTokenizer> = Lazy::new(ClaudeTokenizer::new);

// Response cache
static API_CACHE: Lazy<Mutex<LruCache<String, ApiResponse>>> =
    Lazy::new(|| Mutex::new(LruCache::new(std::num::NonZeroUsize::new(100).unwrap())));

#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub content: String,
    pub warning: Option<String>,
    pub usage: Option<TokenUsage>,
}

/// Makes a request to the Claude API and returns the response.
/// Caches responses to reduce duplicate API calls.
pub async fn get_claude_response(
    user_input: &str,
    history: &[Value],
) -> SagacityResult<ApiResponse> {
    // Check if response is in cache
    let cache_key = format!("{:?}:{}", history, user_input);
    if let Some(cached_response) = API_CACHE.lock().unwrap().get(&cache_key) {
        return Ok(cached_response.clone());
    }

    // Get config
    let config = get_config();
    
    // Check token count and provide warning if close to limit
    let mut messages = history.to_vec();
    messages.push(json!({ "role": "user", "content": user_input }));
    let messages_json = serde_json::to_string(&messages)?;
    
    // Temporary placeholder until we can properly implement token counting
    let token_count = messages_json.len() / 4; // Rough approximation
    
    if token_count > config.token_limit_threshold as usize {
        return Err(SagacityError::token_error(format!(
            "Input exceeds token limit threshold: {} tokens (limit: {})",
            token_count, config.token_limit_threshold
        )));
    }

    // Prepare request payload
    let payload = json!({
        "model": config.model,
        "max_tokens": config.max_tokens,
        "messages": messages,
        "temperature": config.temperature
    });

    // Make API request
    let client = Client::new();
    let response = client
        .post(CLAUDE_API_URL)
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&payload)
        .send()
        .await
        .map_err(|e| SagacityError::api_error(format!("Request failed: {}", e)))?;

    // Check for API errors and clone the status for error reporting
    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(SagacityError::api_error(format!(
            "API returned error: {} - {}",
            status,
            error_text
        )));
    }

    // Parse response
    let response_data: Value = response
        .json()
        .await
        .map_err(|e| SagacityError::api_error(format!("Failed to parse API response: {}", e)))?;

    // Check for API-reported errors
    if let Some(error) = response_data["error"].as_object() {
        return Err(SagacityError::api_error(format!(
            "{}: {}",
            error["type"].as_str().unwrap_or("unknown"),
            error["message"].as_str().unwrap_or("no message")
        )));
    }

    // Extract content and metadata
    let content = response_data["content"][0]["text"]
        .as_str()
        .ok_or_else(|| SagacityError::api_error("Response missing expected content"))?
        .to_string();

    let warning = response_data["warning"].as_str().map(|s| s.to_string());
    
    let usage = if let (Some(input), Some(output)) = (
        response_data["usage"]["input_tokens"].as_u64(),
        response_data["usage"]["output_tokens"].as_u64(),
    ) {
        Some(TokenUsage {
            input_tokens: input as u32,
            output_tokens: output as u32,
        })
    } else {
        None
    };

    let api_response = ApiResponse {
        content,
        warning,
        usage,
    };

    // Cache the response
    API_CACHE.lock().unwrap().put(cache_key, api_response.clone());

    Ok(api_response)
}

/// Summarizes a file by sending its content to the Claude API.
pub async fn summarize_file(
    content: &str,
    language: &str,
) -> SagacityResult<String> {
    let config = get_config();
    
    // Check token count
    let prompt = format!(
        "please analyze this {} code and provide a brief summary of its purpose and functionality.\n\ncode:\n{}",
        language, content
    );
    
    // Temporary placeholder until we can properly implement token counting
    let token_count = prompt.len() / 4; // Rough approximation
    
    if token_count > config.token_limit_threshold as usize {
        return Err(SagacityError::token_error(format!(
            "File too large to summarize: {} tokens (limit: {})",
            token_count, config.token_limit_threshold
        )));
    }

    // Prepare request payload
    let payload = json!({
        "model": config.model,
        "max_tokens": config.max_tokens,
        "messages": [{ "role": "user", "content": prompt }],
        "temperature": config.temperature
    });

    // Make API request
    let client = Client::new();
    let response = client
        .post(CLAUDE_API_URL)
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&payload)
        .send()
        .await
        .map_err(|e| SagacityError::api_error(format!("Request failed: {}", e)))?;

    // Check for API errors and clone the status for error reporting
    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(SagacityError::api_error(format!(
            "API returned error: {} - {}",
            status,
            error_text
        )));
    }

    // Parse response
    let body: Value = response
        .json()
        .await
        .map_err(|e| SagacityError::api_error(format!("Failed to parse API response: {}", e)))?;

    // Check for API-reported errors
    if let Some(error) = body["error"].as_object() {
        return Err(SagacityError::api_error(format!(
            "{}: {}",
            error["type"].as_str().unwrap_or("unknown"),
            error["message"].as_str().unwrap_or("no message")
        )));
    }

    // Extract content
    Ok(body["content"][0]["text"]
        .as_str()
        .unwrap_or("Sorry, I couldn't process that request.")
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use once_cell::sync::OnceCell;
    use std::sync::Mutex;
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // Mock config for tests
    static TEST_CONFIG: OnceCell<Mutex<Config>> = OnceCell::new();

    fn init_test_config() -> &'static Mutex<Config> {
        TEST_CONFIG.get_or_init(|| {
            let mut config = Config::default();
            config.api_key = "test-api-key".to_string();
            Mutex::new(config)
        })
    }

    #[tokio::test]
    async fn test_claude_response_success() {
        // Start mock server
        let mock_server = MockServer::start().await;

        // Set up mock response
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-api-key"))
            .and(header("anthropic-version", ANTHROPIC_VERSION))
            .respond_with(ResponseTemplate::new(200).json(json!({
                "content": [{"text": "This is a test response", "type": "text"}],
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 20
                }
            })))
            .mount(&mock_server)
            .await;

        // Override API URL for test
        let original_url = CLAUDE_API_URL;
        let test_url = mock_server.uri();
        
        // TODO: In a real implementation, we would need to modify how constants are accessed
        // For this test, we'll just note this limitation
        
        // Test would be implemented here to verify API response handling
    }
}