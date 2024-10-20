// src/cache.rs
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug)]
pub struct CodebaseCache {
    pub timestamp: u64,
    pub codebases: Vec<String>,
}

impl CodebaseCache {
    pub fn new(codebases: Vec<String>) -> Self {
        CodebaseCache {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_secs(),
            codebases,
        }
    }
}

pub const CACHE_FILE: &str = "codebase_cache.json";
pub const CACHE_EXPIRY_SECS: u64 = 3600; // 1 hour

pub fn load_codebase_cache() -> Option<CodebaseCache> {
    if let Ok(contents) = fs::read_to_string(CACHE_FILE) {
        if let Ok(cache) = serde_json::from_str::<CodebaseCache>(&contents) {
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_secs();
            if current_time - cache.timestamp < CACHE_EXPIRY_SECS {
                println!("Loaded codebase cache.");
                return Some(cache);
            } else {
                println!("Codebase cache expired.");
            }
        }
    }
    None
}

pub fn save_codebase_cache(codebases: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let cache = CodebaseCache::new(codebases.to_vec());
    let serialized = serde_json::to_string_pretty(&cache)?;
    fs::write(CACHE_FILE, serialized)?;
    println!("Saved codebase cache.");
    Ok(())
}
