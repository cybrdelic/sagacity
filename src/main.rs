use reqwest;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

use std::env;

const CLAUDE_API_URL: &str = "https://api.anthropic.com/v1/completions";

fn get_claude_api_key() -> Result<String, Box<dyn std::error::Error>> {
    let home_dir = env::var("HOME")?;
    let zshrc_path = format!("{}/.zshrc", home_dir);
    let zshrc_content = fs::read_to_string(zshrc_path)?;

    for line in zshrc_content.lines() {
        if line.starts_with("export CLAUDE_API_KEY=") {
            return Ok(line
                .split('=')
                .nth(1)
                .unwrap()
                .trim_matches('"')
                .to_string());
        }
    }

    Err("CLAUDE_API_KEY not found in .zshrc".into())
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
    let client = reqwest::Client::new();
    let response = client
        .post(CLAUDE_API_URL)
        .header("Content-Type", "application/json")
        .header("X-API-Key", api_key)
        .json(&json!({
            "prompt": format!("Summarize the following code:\n\n{}\n\nSummary:", content),
            "max_tokens_to_sample": 300,
            "model": "claude-v1"
        }))
        .send()
        .await?;

    let body: Value = response.json().await?;
    Ok(body["completion"].as_str().unwrap_or("").trim().to_string())
}

async fn index_codebase(
    root_dir: &str,
    api_key: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut index = HashMap::new();
    let files = scan_codebase(root_dir);

    for file_path in files {
        let content = read_file_contents(&file_path)?;
        let summary = summarize_with_claude(&content, api_key).await?;
        index.insert(file_path, summary);
    }

    Ok(index)
}

fn search_index(index: &HashMap<String, String>, query: &str) -> Vec<(String, String)> {
    index
        .iter()
        .filter(|(_, summary)| summary.to_lowercase().contains(&query.to_lowercase()))
        .map(|(file, summary)| (file.clone(), summary.clone()))
        .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root_dir = "."; // Current directory
    let api_key = get_claude_api_key()?;
    let index = index_codebase(root_dir, &api_key).await?;

    println!("Codebase indexed successfully. You can now ask questions about the codebase.");

    loop {
        println!("Enter your query (or 'quit' to exit):");
        let mut query = String::new();
        std::io::stdin().read_line(&mut query)?;
        let query = query.trim();

        if query.to_lowercase() == "quit" {
            break;
        }

        let results = search_index(&index, query);
        if results.is_empty() {
            println!("No results found for your query.");
        } else {
            println!("Search results:");
            for (file, summary) in results {
                println!("File: {}\nSummary: {}\n", file, summary);
            }
        }
    }

    Ok(())
}
