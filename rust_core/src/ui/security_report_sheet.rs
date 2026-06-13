use crate::security::types::{SecurityReport, SecurityResult, SecuritySeverity};
use umya_spreadsheet::{Spreadsheet, Worksheet};

pub fn write_security_report_sheet_into_book(
    book: &mut Spreadsheet,
    report: &SecurityReport,
) -> Result<(), String> {
    let sheet_name = "SECURITY_REPORT";

    if book.get_sheet_by_name(sheet_name).is_some() {
        let _ = book.remove_sheet_by_name(sheet_name);
    }

    let _ = book.new_sheet(sheet_name);

    let sheet = book
        .get_sheet_by_name_mut(sheet_name)
        .ok_or_else(|| "SECURITY_REPORT create error".to_string())?;

    write_overview(sheet, report);
    write_records(sheet, report);
    apply_basic_format(sheet, report.records.len() as u32);

    Ok(())
}

fn write_overview(sheet: &mut Worksheet, report: &SecurityReport) {
    sheet.get_cell_mut("A1").set_value("SecurityFinal");
    sheet
        .get_cell_mut("B1")
        .set_value(result_to_label(report.final_result));

    sheet.get_cell_mut("A2").set_value("File");
    sheet.get_cell_mut("B2").set_value(&report.file_path);

    sheet.get_cell_mut("A4").set_value("CheckName");
    sheet.get_cell_mut("B4").set_value("Result");
    sheet.get_cell_mut("C4").set_value("Severity");
    sheet.get_cell_mut("D4").set_value("Reason");
    sheet.get_cell_mut("E4").set_value("Evidence");
}

fn write_records(sheet: &mut Worksheet, report: &SecurityReport) {
    for (idx, record) in report.records.iter().enumerate() {
        let row = (idx + 5) as u32;

        sheet
            .get_cell_mut(format!("A{}", row))
            .set_value(record.check_name);
        sheet
            .get_cell_mut(format!("B{}", row))
            .set_value(result_to_label(record.result));
        sheet
            .get_cell_mut(format!("C{}", row))
            .set_value(severity_to_label(record.severity));
        sheet
            .get_cell_mut(format!("D{}", row))
            .set_value(&record.reason);
        sheet
            .get_cell_mut(format!("E{}", row))
            .set_value(&record.evidence);
    }
}

fn apply_basic_format(sheet: &mut Worksheet, record_count: u32) {
    let last_row = 4 + record_count.max(1);

    sheet.get_column_dimension_mut("A").set_width(24.0);
    sheet.get_column_dimension_mut("B").set_width(14.0);
    sheet.get_column_dimension_mut("C").set_width(14.0);
    sheet.get_column_dimension_mut("D").set_width(48.0);
    sheet.get_column_dimension_mut("E").set_width(48.0);

    for row in 1..=last_row {
        for col in ["A", "B", "C", "D", "E"] {
            let addr = format!("{}{}", col, row);
            let style = sheet.get_style_mut(addr.as_str());
            style.get_alignment_mut().set_wrap_text(true);
        }
    }
}

fn result_to_label(value: SecurityResult) -> &'static str {
    match value {
        SecurityResult::Reject => "Reject",
        SecurityResult::Warn => "Warn",
        SecurityResult::Pass => "Pass",
    }
}

fn severity_to_label(value: SecuritySeverity) -> &'static str {
    match value {
        SecuritySeverity::Low => "Low",
        SecuritySeverity::Medium => "Medium",
        SecuritySeverity::High => "High",
    }
}