use std::fs::File;
use zip::ZipArchive;
use crate::security::types::{SecurityCheckRecord, SecurityResult, SecuritySeverity};
pub fn run(file_path: &str) -> SecurityCheckRecord {
    match File::open(file_path) {
        Ok(file) => match ZipArchive::new(file) {
            Ok(zip) => {
                let mut found = None;
                for name in zip.file_names() {
                    if name.starts_with("xl/externalLinks/") { found = Some(name.to_string()); break; }
                }
                match found {
                    Some(name) => SecurityCheckRecord { check_name: "external_link_check", result: SecurityResult::Warn, reason: "external link detected".to_string(), evidence: name, severity: SecuritySeverity::Medium },
                    None => SecurityCheckRecord { check_name: "external_link_check", result: SecurityResult::Pass, reason: "no external link detected".to_string(), evidence: "xl/externalLinks not found".to_string(), severity: SecuritySeverity::Low },
                }
            }
            Err(e) => SecurityCheckRecord { check_name: "external_link_check", result: SecurityResult::Reject, reason: format!("zip open failed before external link check: {}", e), evidence: file_path.to_string(), severity: SecuritySeverity::High },
        },
        Err(e) => SecurityCheckRecord { check_name: "external_link_check", result: SecurityResult::Reject, reason: format!("file open failed: {}", e), evidence: file_path.to_string(), severity: SecuritySeverity::High },
    }
}
