// src/chatbot.rs

use chrono::Utc;

use crate::api::{generate_llm_response, search_index};
use crate::models::{Chatbot, Message};
use crate::utils::detect_language;
use std::error::Error;

/// Implements the Chatbot functionalities.
impl Chatbot {
    /// Handles user queries by finding relevant files, generating context, and obtaining AI responses.
    pub async fn chat(&mut self, user_query: &str) -> Result<String, Box<dyn Error>> {
        debug_print!("Starting chat with system");

        // Step 1: Find relevant files
        let index = self.index.clone();
        let api_key = self.api_key.clone();
        let relevant_files = search_index(&index, user_query, &api_key, self).await?;

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
        let context = generate_context(&relevant_file_info, user_query)?;

        // Step 4: Generate response using the LLM
        let api_key = self.api_key.clone();
        let memory = self.memory.clone();
        let (response, _) =
            generate_llm_response(&context, &api_key, &memory, user_query, self).await?;

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

/// Wrapper function to expose `chat` method.
pub async fn chat_with_system(
    chatbot: &mut Chatbot,
    user_query: &str,
) -> Result<String, Box<dyn Error>> {
    chatbot.chat(user_query).await
}

/// Generates the context string for the LLM based on relevant files and user query.
fn generate_context(
    relevant_files: &[(String, String)],
    user_query: &str,
) -> Result<String, Box<dyn Error>> {
    let mut context = format!("User query: {}\n\nRelevant file contents:\n", user_query);
    for (file_path, _) in relevant_files {
        let file_content = std::fs::read_to_string(file_path)?;
        context.push_str(&format!(
            "File: {}\nContent:\n{}\n\n",
            file_path, file_content
        ));
    }
    Ok(context)
}
