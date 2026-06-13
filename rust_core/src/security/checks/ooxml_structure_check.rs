use std::fs::File;
use zip::ZipArchive;
use crate::security::types::{SecurityCheckRecord, SecurityResult, SecuritySeverity};
pub fn run(file_path: &str) -> SecurityCheckRecord {
    match File::open(file_path) {
        Ok(file) => match ZipArchive::new(file) {
            Ok(mut zip) => {
                let has_content_types = zip.by_name("[Content_Types].xml").is_ok();
                let has_workbook = zip.by_name("xl/workbook.xml").is_ok();
                if has_content_types && has_workbook {
                    SecurityCheckRecord { check_name: "ooxml_structure_check", result: SecurityResult::Pass, reason: "minimum OOXML structure present".to_string(), evidence: "[Content_Types].xml + xl/workbook.xml".to_string(), severity: SecuritySeverity::Low }
                } else {
                    SecurityCheckRecord { check_name: "ooxml_structure_check", result: SecurityResult::Reject, reason: "required OOXML parts missing".to_string(), evidence: format!("content_types={} workbook={}", has_content_types, has_workbook), severity: SecuritySeverity::High }
                }
            }
            Err(e) => SecurityCheckRecord { check_name: "ooxml_structure_check", result: SecurityResult::Reject, reason: format!("zip open failed before OOXML check: {}", e), evidence: file_path.to_string(), severity: SecuritySeverity::High },
        },
        Err(e) => SecurityCheckRecord { check_name: "ooxml_structure_check", result: SecurityResult::Reject, reason: format!("file open failed: {}", e), evidence: file_path.to_string(), severity: SecuritySeverity::High },
    }
}
