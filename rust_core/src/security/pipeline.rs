use crate::security::checks::{extension_check, external_link_check, ooxml_structure_check, size_check, vba_check, zip_integrity_check};
use crate::security::types::{SecurityCheckRecord, SecurityReport, SecurityResult};

pub fn inspect_xlsx(file_path: &str) -> SecurityReport {
    let mut records = Vec::new();
    records.push(extension_check::run(file_path));
    if has_reject(&records) { return finalize(file_path, records); }
    records.push(size_check::run(file_path));
    if has_reject(&records) { return finalize(file_path, records); }
    records.push(zip_integrity_check::run(file_path));
    if has_reject(&records) { return finalize(file_path, records); }
    records.push(ooxml_structure_check::run(file_path));
    if has_reject(&records) { return finalize(file_path, records); }
    records.push(vba_check::run(file_path));
    if has_reject(&records) { return finalize(file_path, records); }
    records.push(external_link_check::run(file_path));
    finalize(file_path, records)
}

pub fn print_report(report: &SecurityReport) {
    println!("SECURITY file = {}", report.file_path);
    println!("SECURITY final = {:?}", report.final_result);
    for record in &report.records {
        println!("SECURITY {} => {:?} / {:?} / {} / {}", record.check_name, record.result, record.severity, record.reason, record.evidence);
    }
}

fn has_reject(records: &[SecurityCheckRecord]) -> bool { records.iter().any(|r| r.result == SecurityResult::Reject) }
fn finalize(file_path: &str, records: Vec<SecurityCheckRecord>) -> SecurityReport {
    let final_result = if has_reject(&records) { SecurityResult::Reject } else if records.iter().any(|r| r.result == SecurityResult::Warn) { SecurityResult::Warn } else { SecurityResult::Pass };
    SecurityReport { file_path: file_path.to_string(), final_result, records }
}
