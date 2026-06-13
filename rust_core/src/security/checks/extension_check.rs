use std::path::Path;
use crate::security::types::{SecurityCheckRecord, SecurityResult, SecuritySeverity};
pub fn run(file_path: &str) -> SecurityCheckRecord {
    let extension = Path::new(file_path).extension().and_then(|s| s.to_str()).unwrap_or("");
    if extension.eq_ignore_ascii_case("xlsx") {
        SecurityCheckRecord { check_name: "extension_check", result: SecurityResult::Pass, reason: "xlsx extension accepted".to_string(), evidence: file_path.to_string(), severity: SecuritySeverity::Low }
    } else {
        SecurityCheckRecord { check_name: "extension_check", result: SecurityResult::Reject, reason: "only .xlsx is accepted in phase 1".to_string(), evidence: file_path.to_string(), severity: SecuritySeverity::High }
    }
}
