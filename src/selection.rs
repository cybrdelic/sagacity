use crate::cache::{load_codebase_cache, save_codebase_cache};
use crate::constants::*;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use ignore::WalkBuilder;
use serde::Deserialize;
use shellexpand;
use skim::prelude::*;
use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

#[derive(Deserialize)]
struct GitHubRepo {
    full_name: String,
    clone_url: String,
}

// clone_github_repo defined here
pub fn clone_github_repo(
    clone_url: &str,
    repo_name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let clone_path = env::temp_dir().join(repo_name);
    if clone_path.exists() {
        println!("Repository already cloned.");
    } else {
        let status = Command::new("git")
            .args(&["clone", clone_url, clone_path.to_str().unwrap()])
            .status()?;
        if !status.success() {
            return Err("Failed to clone repository".into());
        }
    }
    Ok(clone_path)
}

// same scanning functions as before...
pub fn scan_custom_directory(path: &PathBuf) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut codebase_strings = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            codebase_strings.push(path.display().to_string());
        }
    }
    Ok(codebase_strings)
}

fn list_projects_in_home() -> Vec<PathBuf> {
    if let Some(cache) = load_codebase_cache() {
        return cache.codebases.iter().map(|p| PathBuf::from(p)).collect();
    }

    let mut projects = Vec::new();
    if let Some(home_path) = home::home_dir() {
        let walker = WalkBuilder::new(home_path)
            .follow_links(false)
            .max_depth(Some(4))
            .build();

        let source_extensions = [
            "rs", "py", "go", "js", "ts", "java", "c", "cpp", "md", "toml",
        ];

        let mut project_paths = HashSet::new();
        for entry in walker {
            if let Ok(entry) = entry {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if source_extensions.contains(&ext.to_string_lossy().as_ref()) {
                            if let Some(parent) = path.parent() {
                                project_paths.insert(parent.to_path_buf());
                            }
                        }
                    }
                }
            }
        }

        projects.extend(project_paths.into_iter());
    }

    let additional_paths = vec!["~/alexf/software-projects/.current"];
    for path_str in additional_paths {
        let expanded_path = shellexpand::tilde(path_str).into_owned();
        let path = PathBuf::from(expanded_path);
        if path.exists() && path.is_dir() {
            if path_str == "~/alexf/software-projects/.current" {
                if let Ok(entries) = fs::read_dir(&path) {
                    for entry in entries.flatten() {
                        if let Ok(file_type) = entry.file_type() {
                            if file_type.is_dir() {
                                projects.push(entry.path());
                            }
                        }
                    }
                }
            } else {
                projects.push(path);
            }
        }
    }

    let codebase_strings: Vec<String> = projects
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    if let Err(e) = save_codebase_cache(&codebase_strings) {
        println!("Failed to save codebase cache: {}", e);
    }

    projects
}

async fn search_github_repos(query: &str) -> Result<Vec<GitHubRepo>, Box<dyn std::error::Error>> {
    let url = format!("https://api.github.com/search/repositories?q={}", query);
    let client = reqwest::Client::new();
    let res = client
        .get(&url)
        .header(reqwest::header::USER_AGENT, "CodebaseExplorer")
        .header(reqwest::header::ACCEPT, "application/vnd.github.v3+json")
        .send()
        .await?;

    if res.status() == 403 {
        return Err("GitHub API rate limit exceeded.".into());
    }

    let json: serde_json::Value = res.json().await?;
    let repos = json["items"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|item| serde_json::from_value(item.clone()).ok())
        .collect();
    Ok(repos)
}

pub async fn codebase_selection_menu() -> Result<PathBuf, Box<dyn std::error::Error>> {
    loop {
        let choices = vec![
            "Select from local projects",
            "Search GitHub",
            "Specify Custom Directory",
            "Quit",
        ];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Please select a codebase to index")
            .default(0)
            .items(&choices)
            .interact()?;

        match selection {
            0 => {
                let projects = list_projects_in_home();
                if projects.is_empty() {
                    println!("No projects found in your home directory.");
                    if !Confirm::with_theme(&ColorfulTheme::default())
                        .with_prompt("Would you like to search GitHub instead?")
                        .interact()?
                    {
                        continue;
                    } else {
                        continue;
                    }
                }

                let project_names: Vec<String> =
                    projects.iter().map(|p| p.display().to_string()).collect();

                let options = SkimOptionsBuilder::default()
                    .height(Some("50%"))
                    .multi(false)
                    .prompt(Some("Search Projects > "))
                    .build()
                    .unwrap();

                let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
                for project in project_names.clone() {
                    let _ = tx.send(Arc::new(project) as Arc<dyn SkimItem>);
                }
                drop(tx);
                let selected = Skim::run_with(&options, Some(rx))
                    .map(|out| out.selected_items)
                    .unwrap_or_else(|| Vec::new());

                if selected.is_empty() {
                    println!("No project selected.");
                    continue;
                }

                let selected_project = selected[0].output().to_string();
                if let Some(path) = projects
                    .iter()
                    .find(|p| p.display().to_string() == selected_project)
                {
                    return Ok(path.clone());
                } else {
                    println!("Selected project not found.");
                }
            }
            1 => {
                let query: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Enter GitHub repository search query")
                    .interact_text()?;

                let repos = search_github_repos(&query).await?;
                if repos.is_empty() {
                    println!("No repositories found for query '{}'.", query);
                    if !Confirm::with_theme(&ColorfulTheme::default())
                        .with_prompt("Would you like to try a different query?")
                        .interact()?
                    {
                        continue;
                    } else {
                        continue;
                    }
                }
                let repo_names: Vec<String> = repos.iter().map(|r| r.full_name.clone()).collect();

                let repo_selection = Select::with_theme(&ColorfulTheme::default())
                    .with_prompt("Select a repository")
                    .default(0)
                    .items(&repo_names)
                    .interact()?;

                let repo = &repos[repo_selection];
                let clone_path = clone_github_repo(&repo.clone_url, &repo.full_name)?;
                return Ok(clone_path);
            }
            2 => {
                let custom_path: String = Input::with_theme(&ColorfulTheme::default())
                    .with_prompt("Enter the full path of the directory to search")
                    .interact_text()?;
                let expanded_path = shellexpand::tilde(&custom_path).to_string();
                let path = PathBuf::from(expanded_path);
                if path.exists() && path.is_dir() {
                    println!("Scanning custom directory: {}", path.display());
                    let codebase_strings = scan_custom_directory(&path)?;
                    if !codebase_strings.is_empty() {
                        if let Err(e) = save_codebase_cache(&codebase_strings) {
                            println!("Failed to save codebase cache: {}", e);
                        }
                        let codebases: Vec<PathBuf> =
                            codebase_strings.iter().map(PathBuf::from).collect();
                        let options = SkimOptionsBuilder::default()
                            .height(Some("50%"))
                            .multi(false)
                            .prompt(Some("Select Project > "))
                            .build()
                            .unwrap();

                        let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
                        for codebase in codebase_strings.clone() {
                            let _ = tx.send(Arc::new(codebase) as Arc<dyn SkimItem>);
                        }
                        drop(tx);
                        let selected = Skim::run_with(&options, Some(rx))
                            .map(|out| out.selected_items)
                            .unwrap_or_else(|| Vec::new());

                        if selected.is_empty() {
                            println!("No project selected.");
                            continue;
                        }

                        let selected_project = selected[0].output().to_string();
                        if let Some(path) = codebases
                            .iter()
                            .find(|p| p.display().to_string() == selected_project)
                        {
                            return Ok(path.clone());
                        } else {
                            println!("Selected project not found.");
                        }
                    } else {
                        println!("No valid projects found in the specified directory.");
                        continue;
                    }
                } else {
                    println!("The specified path does not exist or is not a directory.");
                    continue;
                }
            }
            3 => {
                println!("Exiting...");
                std::process::exit(0);
            }
            _ => unreachable!(),
        }
    }
}
