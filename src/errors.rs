use thiserror::Error;

#[derive(Error, Debug)]
pub enum SagacityError {
    #[error("API Error: {0}")]
    ApiError(String),

    #[error("File access error: {0}")]
    FileAccessError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Token limit exceeded: {0}")]
    TokenLimitError(String),

    #[error("Clipboard operation failed: {0}")]
    ClipboardError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("SQLx error: {0}")]
    SqlxError(#[from] sqlx::Error),

    #[error("Reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("Environment variable error: {0}")]
    EnvError(#[from] std::env::VarError),

    #[error("Error during indexing: {0}")]
    IndexingError(String),

    #[error("Unknown error: {0}")]
    UnknownError(String),
}

impl SagacityError {
    pub fn api_error(message: impl Into<String>) -> Self {
        SagacityError::ApiError(message.into())
    }

    pub fn file_error(message: impl Into<String>) -> Self {
        SagacityError::FileAccessError(message.into())
    }

    pub fn db_error(message: impl Into<String>) -> Self {
        SagacityError::DatabaseError(message.into())
    }

    pub fn config_error(message: impl Into<String>) -> Self {
        SagacityError::ConfigError(message.into())
    }

    pub fn token_error(message: impl Into<String>) -> Self {
        SagacityError::TokenLimitError(message.into())
    }

    pub fn indexing_error(message: impl Into<String>) -> Self {
        SagacityError::IndexingError(message.into())
    }

    pub fn clipboard_error(message: impl Into<String>) -> Self {
        SagacityError::ClipboardError(message.into())
    }
    
    #[allow(dead_code)]
    pub fn to_boxed<E: std::error::Error + Send + Sync + 'static>(err: E) -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(err)
    }

    pub fn user_message(&self) -> String {
        match self {
            SagacityError::ApiError(msg) => format!("API error: {}", msg),
            SagacityError::FileAccessError(msg) => format!("File access error: {}", msg),
            SagacityError::DatabaseError(msg) => format!("Database error: {}", msg),
            SagacityError::ConfigError(msg) => format!("Configuration error: {}", msg),
            SagacityError::TokenLimitError(msg) => format!("Token limit exceeded: {}", msg),
            SagacityError::ClipboardError(msg) => format!("Clipboard error: {}", msg),
            SagacityError::IoError(e) => format!("IO error: {}", e),
            SagacityError::JsonError(e) => format!("JSON error: {}", e),
            SagacityError::SqlxError(e) => format!("Database error: {}", e),
            SagacityError::ReqwestError(e) => format!("Network error: {}", e),
            SagacityError::EnvError(e) => format!("Environment error: {}", e),
            SagacityError::IndexingError(msg) => format!("Indexing error: {}", msg),
            SagacityError::UnknownError(msg) => format!("Unknown error: {}", msg),
        }
    }
}

pub type SagacityResult<T> = Result<T, SagacityError>;