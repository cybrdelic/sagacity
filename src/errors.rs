// logging.rs

use crate::models::ApiCallLog;
use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;

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
        .unwrap();

    file.write_all(log_entry.as_bytes()).unwrap();
}
