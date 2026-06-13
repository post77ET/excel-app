use std::fs::File;
use zip::ZipArchive;
use crate::security::types::{SecurityCheckRecord, SecurityResult, SecuritySeverity};
pub fn run(file_path: &str) -> SecurityCheckRecord {
    match File::open(file_path) {
        Ok(file) => match ZipArchive::new(file) {
            Ok(zip) => SecurityCheckRecord { check_name: "zip_integrity_check", result: SecurityResult::Pass, reason: "xlsx zip opened successfully".to_string(), evidence: format!("zip entries={}", zip.len()), severity: SecuritySeverity::Low },
            Err(e) => SecurityCheckRecord { check_name: "zip_integrity_check", result: SecurityResult::Reject, reason: format!("zip open failed: {}", e), evidence: file_path.to_string(), severity: SecuritySeverity::High },
        },
        Err(e) => SecurityCheckRecord { check_name: "zip_integrity_check", result: SecurityResult::Reject, reason: format!("file open failed: {}", e), evidence: file_path.to_string(), severity: SecuritySeverity::High },
    }
}
