use crate::chatbot::{detect_language, generate_llm_response, summarize_with_claude, IndexCache};
use crate::Chatbot;
use chrono::{DateTime, Utc};
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Select};
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use prettytable::{Cell, Row, Table};
use reqwest::header::{ACCEPT, USER_AGENT};
use serde::Deserialize;
use serde_json::Value;
use shellexpand;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH; // for Chatbot struct

#[derive(Deserialize, Debug)]
pub struct GitHubRepo {
    pub full_name: String,
    pub clone_url: String,
    pub description: Option<String>,
    pub html_url: String,
    pub stargazers_count: u32,
    pub language: Option<String>,
}

macro_rules! debug_print {
    ($($arg:tt)*) => {
        eprintln!("[DEBUG] {}", format!($($arg)*));
    };
}

pub async fn generate_github_recommendations(
    chatbot: &mut Chatbot,
) -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = shellexpand::tilde("~/alexf/software-projects/.current/").to_string();
    let current_path = PathBuf::from(current_dir);

    if !current_path.exists() || !current_path.is_dir() {
        println!(
            "{}",
            "The .current/ directory does not exist or is not a directory.".red()
        );
        return Ok(());
    }

    let codebases = scan_current_directory(&current_path)?;

    if codebases.is_empty() {
        println!(
            "{}",
            "No codebases found in the .current/ directory.".yellow()
        );
        return Ok(());
    }

    let mut aggregated_context = String::new();
    let pb = ProgressBar::new(codebases.len() as u64);
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} [{elapsed_precise}] {msg}")
            .unwrap(),
    );
    pb.set_message("Aggregating codebase indexes...");

    for codebase in codebases {
        pb.set_message(format!("Processing: {}", codebase.display()));
        let index = load_or_create_index_cache(&codebase, chatbot).await?;
        for (_file, (summary, _language)) in index {
            aggregated_context.push_str(&format!("{}\n", summary));
        }
        pb.inc(1);
    }

    pb.finish_with_message("Aggregation complete.");

    println!(
        "{}",
        "Generating GitHub recommendations based on your codebases..."
            .bold()
            .cyan()
    );

    let github_repos = search_github_repos(&aggregated_context).await?;

    if github_repos.is_empty() {
        println!("{}", "No relevant GitHub repositories found.".yellow());
        return Ok(());
    }

    present_github_recommendations(&github_repos);

    Ok(())
}

fn scan_current_directory(
    current_path: &PathBuf,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let mut codebases = Vec::new();
    for entry in fs::read_dir(current_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            codebases.push(path);
        }
    }
    Ok(codebases)
}

async fn load_or_create_index_cache(
    codebase_path: &PathBuf,
    chatbot: &mut Chatbot,
) -> Result<HashMap<String, (String, String)>, Box<dyn std::error::Error>> {
    let cache_path = codebase_path.join("index_cache.json");

    if cache_path.exists() {
        let cache_content = fs::read_to_string(&cache_path)?;
        let cache: IndexCache = serde_json::from_str(&cache_content)?;
        debug_print!("Loaded index cache for {}", codebase_path.display());
        Ok(cache.index)
    } else {
        let index = index_codebase_specific(codebase_path, chatbot).await?;
        let cache = IndexCache {
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            last_modification: 0,
            index: index.clone(),
            file_mod_times: HashMap::new(),
        };
        let serialized = serde_json::to_string_pretty(&cache)?;
        fs::write(&cache_path, serialized)?;
        debug_print!(
            "Created and saved new index cache for {}",
            codebase_path.display()
        );
        Ok(index)
    }
}

async fn index_codebase_specific(
    codebase_path: &PathBuf,
    chatbot: &mut Chatbot,
) -> Result<HashMap<String, (String, String)>, Box<dyn std::error::Error>> {
    let mut index = HashMap::new();
    let walker = WalkBuilder::new(codebase_path)
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

    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} [{elapsed_precise}] {msg}")
            .unwrap(),
    );
    pb.set_message(format!("Indexing: {}", codebase_path.display()));

    for file_path in files {
        pb.set_message(format!("Processing file: {}", file_path));
        let content = fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;
        let language = detect_language(&file_path);
        let api_key = chatbot.api_key.clone();

        let summary = match summarize_with_claude(&content, &api_key, &language, chatbot).await {
            Ok(s) => s,
            Err(e) => {
                debug_print!("Error summarizing {}: {}", file_path, e);
                format!(
                    "Failed to summarize. File content preview: {}",
                    &content[..std::cmp::min(content.len(), 100)]
                )
            }
        };

        index.insert(file_path.clone(), (summary, language));
        pb.inc(1);
    }

    pb.finish_with_message(format!("Indexing complete for {}", codebase_path.display()));

    Ok(index)
}

async fn search_github_repos(
    aggregated_context: &str,
) -> Result<Vec<GitHubRepo>, Box<dyn std::error::Error>> {
    let keywords = extract_keywords(aggregated_context);
    if keywords.is_empty() {
        return Ok(Vec::new());
    }

    let query = keywords.join("+");
    let url = format!(
        "https://api.github.com/search/repositories?q={}&sort=stars&order=desc&per_page=10",
        query
    );
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header(USER_AGENT, "CodebaseExplorer")
        .header(ACCEPT, "application/vnd.github.v3+json")
        .send()
        .await?;

    if response.status() == 403 {
        return Err("GitHub API rate limit exceeded.".into());
    }

    let body: Value = response.json().await?;
    let repos: Vec<GitHubRepo> =
        serde_json::from_value(body["items"].clone()).unwrap_or(Vec::new());
    Ok(repos)
}

fn extract_keywords(context: &str) -> Vec<String> {
    let mut keywords = HashSet::new();
    for word in context.split_whitespace() {
        let w = word.trim_matches(|c: char| !c.is_alphanumeric());
        if w.len() > 4 {
            keywords.insert(w.to_lowercase());
        }
    }
    keywords.into_iter().collect()
}

fn present_github_recommendations(repos: &[GitHubRepo]) {
    println!("{}", "\n--- GitHub Recommendations ---".bold().green());

    let mut table = Table::new();
    table.add_row(Row::new(vec![
        Cell::new("Name").style_spec("b"),
        Cell::new("Description"),
        Cell::new("Stars"),
        Cell::new("Language"),
        Cell::new("URL"),
    ]));

    for repo in repos {
        table.add_row(Row::new(vec![
            Cell::new(&repo.full_name),
            Cell::new(repo.description.as_deref().unwrap_or("No description")),
            Cell::new(&repo.stargazers_count.to_string()),
            Cell::new(repo.language.as_deref().unwrap_or("N/A")),
            Cell::new(&repo.html_url),
        ]));
    }

    table.printstd();

    let choices: Vec<String> = repos.iter().map(|r| r.full_name.clone()).collect();
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a repository to clone or open")
        .default(0)
        .items(&choices)
        .item("Back to Main Menu")
        .interact()
        .unwrap();

    if selection < repos.len() {
        let selected_repo = &repos[selection];
        let action_choices = vec!["Clone Repository", "Open in Browser", "Back"];
        let action = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("What would you like to do with this repository?")
            .default(0)
            .items(&action_choices)
            .interact()
            .unwrap();

        match action {
            0 => {
                // Clone the repository
                match clone_github_repo(&selected_repo.clone_url, &selected_repo.full_name) {
                    Ok(path) => println!("Repository cloned to {:?}", path),
                    Err(e) => println!("Failed to clone repository: {}", e),
                }
            }
            1 => {
                // Open in browser
                if let Err(e) = open::that(&selected_repo.html_url) {
                    println!("Failed to open browser: {}", e);
                }
            }
            2 => {}
            _ => unreachable!(),
        }
    }
}

// Define clone_github_repo locally here to fix the unresolved import issue
fn clone_github_repo(
    clone_url: &str,
    repo_name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let clone_path = env::temp_dir().join(repo_name);
    if clone_path.exists() {
        println!("Repository already cloned.");
    } else {
        let status = std::process::Command::new("git")
            .args(&["clone", clone_url, clone_path.to_str().unwrap()])
            .status()?;
        if !status.success() {
            return Err("Failed to clone repository".into());
        }
    }
    Ok(clone_path)
}
