// src/logging.rs

use crate::models::ApiCallLog;
use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;

/// Logs an API call to the `api_calls.log` file.
pub fn log_api_call(log: &ApiCallLog) {
    let log_entry = format!(
        "[{}] {} - {} - Status: {} - Time: {}ms\n",
        log.timestamp.to_rfc3339(),
        log.endpoint,
        log.request_summary,
        log.response_status,
        log.response_time_ms
    );

    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open("api_calls.log")
        .unwrap_or_else(|e| {
            eprintln!("Failed to open log file: {}", e);
            std::process::exit(1);
        });

    if let Err(e) = file.write_all(log_entry.as_bytes()) {
        eprintln!("Failed to write to log file: {}", e);
    }
}
