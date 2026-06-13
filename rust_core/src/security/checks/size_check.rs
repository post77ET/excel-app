use std::fs;
use crate::security::types::{SecurityCheckRecord, SecurityResult, SecuritySeverity};
const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024;
pub fn run(file_path: &str) -> SecurityCheckRecord {
    let max_bytes = std::env::var("ETB_MAX_FILE_BYTES").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(DEFAULT_MAX_BYTES);
    match fs::metadata(file_path) {
        Ok(meta) => {
            let len = meta.len();
            if len > max_bytes {
                SecurityCheckRecord { check_name: "size_check", result: SecurityResult::Reject, reason: format!("file size exceeds limit: {} > {}", len, max_bytes), evidence: format!("{} bytes", len), severity: SecuritySeverity::High }
            } else {
                SecurityCheckRecord { check_name: "size_check", result: SecurityResult::Pass, reason: "file size within limit".to_string(), evidence: format!("{} bytes", len), severity: SecuritySeverity::Low }
            }
        }
        Err(e) => SecurityCheckRecord { check_name: "size_check", result: SecurityResult::Reject, reason: format!("metadata read failed: {}", e), evidence: file_path.to_string(), severity: SecuritySeverity::High },
    }
}
