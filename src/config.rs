use crate::errors::{SagacityError, SagacityResult};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{env, fs, path::PathBuf, sync::RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_key: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub concurrent_indexing_tasks: usize,
    pub db_path: String,
    pub token_limit_threshold: u32,
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "claude-3-opus-20240229".to_string(),
            max_tokens: 1024,
            temperature: 0.7,
            concurrent_indexing_tasks: 4,
            db_path: "myriad_db.sqlite".to_string(),
            token_limit_threshold: 100_000,
            log_level: "info".to_string(),
        }
    }
}

static CONFIG: Lazy<RwLock<Config>> = Lazy::new(|| RwLock::new(Config::default()));

pub fn initialize_config() -> SagacityResult<()> {
    let config_path = get_config_path()?;
    
    // If config exists, load it
    if config_path.exists() {
        let config_str = fs::read_to_string(&config_path)
            .map_err(|e| SagacityError::config_error(format!("Failed to read config file: {}", e)))?;
        
        let config: Config = serde_json::from_str(&config_str)
            .map_err(|e| SagacityError::config_error(format!("Failed to parse config: {}", e)))?;
        
        // Validate the config
        validate_config(&config)?;
        
        // Update the global config
        *CONFIG.write().unwrap() = config;
    } else {
        // Create default config
        let mut config = Config::default();
        
        // Try to get API key from env var
        if let Ok(key) = env::var("ANTHROPIC_API_KEY") {
            config.api_key = key;
        }
        
        // Save default config
        fs::create_dir_all(config_path.parent().unwrap())
            .map_err(|e| SagacityError::config_error(format!("Failed to create config directory: {}", e)))?;
        
        let config_str = serde_json::to_string_pretty(&config)
            .map_err(|e| SagacityError::config_error(format!("Failed to serialize config: {}", e)))?;
        
        fs::write(&config_path, config_str)
            .map_err(|e| SagacityError::config_error(format!("Failed to write config file: {}", e)))?;
        
        // Update the global config
        *CONFIG.write().unwrap() = config;
    }
    
    Ok(())
}

fn get_config_path() -> SagacityResult<PathBuf> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| SagacityError::config_error("Could not determine home directory"))?;
    
    Ok(home_dir.join(".config").join("sagacity").join("config.json"))
}

fn validate_config(config: &Config) -> SagacityResult<()> {
    // Check API key
    if config.api_key.is_empty() {
        return Err(SagacityError::config_error("API key is required"));
    }
    
    // Check model
    if config.model.is_empty() {
        return Err(SagacityError::config_error("Model name is required"));
    }
    
    // Check temperature range
    if config.temperature < 0.0 || config.temperature > 1.0 {
        return Err(SagacityError::config_error("Temperature must be between 0.0 and 1.0"));
    }
    
    // Check max_tokens
    if config.max_tokens == 0 {
        return Err(SagacityError::config_error("max_tokens must be greater than 0"));
    }
    
    // Check concurrent tasks
    if config.concurrent_indexing_tasks == 0 {
        return Err(SagacityError::config_error("concurrent_indexing_tasks must be greater than 0"));
    }
    
    Ok(())
}

pub fn get_config() -> Config {
    CONFIG.read().unwrap().clone()
}

pub fn update_config(updated_config: Config) -> SagacityResult<()> {
    validate_config(&updated_config)?;
    
    let config_path = get_config_path()?;
    let config_str = serde_json::to_string_pretty(&updated_config)
        .map_err(|e| SagacityError::config_error(format!("Failed to serialize config: {}", e)))?;
    
    fs::write(&config_path, config_str)
        .map_err(|e| SagacityError::config_error(format!("Failed to write config file: {}", e)))?;
    
    *CONFIG.write().unwrap() = updated_config;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[test]
    fn test_validate_config_valid() {
        let config = Config::default();
        assert!(validate_config(&config).is_ok());
    }
    
    #[test]
    fn test_validate_config_invalid_empty_api_key() {
        let mut config = Config::default();
        config.api_key = "".to_string();
        assert!(validate_config(&config).is_err());
    }
    
    #[test]
    fn test_validate_config_invalid_temperature() {
        let mut config = Config::default();
        config.temperature = 1.5;
        assert!(validate_config(&config).is_err());
    }
}