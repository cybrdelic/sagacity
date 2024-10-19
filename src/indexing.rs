// src/indexing.rs

use crate::api::summarize_with_claude;
use crate::models::{Chatbot, IndexCache};
use crate::utils::detect_language;
use ignore::WalkBuilder;
use indicatif::ProgressBar;
use log::debug;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

/// Loads the index cache from a JSON file.
pub fn load_index_cache() -> Result<Option<IndexCache>, Box<dyn Error>> {
    if let Ok(contents) = fs::read_to_string("index_cache.json") {
        let cache: IndexCache = serde_json::from_str(&contents)?;
        debug!("Index cache loaded successfully.");
        Ok(Some(cache))
    } else {
        debug!("No existing index cache found.");
        Ok(None)
    }
}

/// Saves the index cache to a JSON file.
pub fn save_index_cache(
    index: &HashMap<String, (String, String)>,
    last_modification: u64,
    file_mod_times: &HashMap<String, u64>,
) -> Result<(), Box<dyn Error>> {
    let cache = IndexCache {
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        last_modification,
        index: index.clone(),
        file_mod_times: file_mod_times.clone(),
    };
    let serialized = serde_json::to_string_pretty(&cache)?;
    fs::write("index_cache.json", serialized)?;
    debug!("Index cache saved successfully.");
    Ok(())
}

/// Indexes the codebase by summarizing relevant files.
pub async fn index_codebase(
    root_dir: &str,
    api_key: &str,
    pb: &ProgressBar,
    chatbot: &mut Chatbot,
) -> Result<(HashMap<String, (String, String)>, u64, HashMap<String, u64>), Box<dyn Error>> {
    let mut index = chatbot.index.clone();
    let mut file_mod_times = chatbot.file_mod_times.clone();

    let walker = WalkBuilder::new(root_dir)
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

        let metadata = fs::metadata(&file_path)?;
        let modified = metadata.modified()?;
        let modified_secs = modified.duration_since(UNIX_EPOCH)?.as_secs();
        last_modification = std::cmp::max(last_modification, modified_secs);

        files_set.insert(file_path.clone());

        let needs_reindex = match file_mod_times.get(file_path) {
            Some(&cached_mod_time) => modified_secs > cached_mod_time,
            None => true,
        };

        if needs_reindex {
            debug!("Re-indexing file: {}", file_path);
            let content = fs::read_to_string(&file_path)
                .map_err(|e| format!("Failed to read file {}: {}", file_path, e))?;

            let language = detect_language(&file_path);
            let summary = match summarize_with_claude(&content, api_key, &language, chatbot).await {
                Ok(summary) => summary,
                Err(e) => {
                    debug!("Error summarizing {}: {}", file_path, e);
                    format!(
                        "Failed to summarize. File content preview: {}",
                        &content[..std::cmp::min(content.len(), 100)]
                    )
                }
            };

            index.insert(file_path.clone(), (summary, language));
            file_mod_times.insert(file_path.clone(), modified_secs);
        } else {
            debug!("Skipping file (no changes): {}", file_path);
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
