use crate::ui::types::UiRow;
use umya_spreadsheet::{Workbook, Worksheet};

const SHEET_NAME: &str = "TRANSLATION_WARNINGS";

pub fn write_translation_warnings_sheet_into_book(
    book: &mut Workbook,
    rows: &[UiRow],
) -> Result<(), String> {
    if book.sheet_by_name(SHEET_NAME).is_ok() {
        let _ = book.remove_sheet_by_name(SHEET_NAME);
    }

    let _ = book.new_sheet(SHEET_NAME);

    let sheet = book
        .sheet_by_name_mut(SHEET_NAME)
        .map_err(|_| "TRANSLATION_WARNINGS create error".to_string())?;

    write_headers(sheet);
    write_rows(sheet, rows);
    apply_basic_format(sheet, rows.len() as u32 + 1);

    Ok(())
}

fn write_headers(sheet: &mut Worksheet) {
    let headers = [
        "Sheet",
        "Cell",
        "CellKind",
        "Original",
        "Candidate1Alarm",
        "Candidate2Alarm",
        "Candidate3Alarm",
        "Note",
    ];

    for (idx, header) in headers.iter().enumerate() {
        let addr = format!("{}1", col_to_letters((idx + 1) as u32));
        sheet.cell_mut(addr.as_str()).set_value(*header);
    }
}

fn write_rows(sheet: &mut Worksheet, rows: &[UiRow]) {
    let mut out_row: u32 = 2;

    for row in rows {
        let a1 = row.alarms.candidate1_alarm.clone().unwrap_or_default();
        let a2 = row.alarms.candidate2_alarm.clone().unwrap_or_default();
        let a3 = row.alarms.candidate3_alarm.clone().unwrap_or_default();

        let has_warning =
            !a1.trim().is_empty() || !a2.trim().is_empty() || !a3.trim().is_empty() || !row.note.trim().is_empty();

        if !has_warning {
            continue;
        }

        sheet.cell_mut(format!("A{}", out_row)).set_value(&row.sheet_name);
        sheet.cell_mut(format!("B{}", out_row)).set_value(&row.anchor_address);
        sheet.cell_mut(format!("C{}", out_row)).set_value(&row.cell_kind);
        sheet.cell_mut(format!("D{}", out_row)).set_value(&row.original);
        sheet.cell_mut(format!("E{}", out_row)).set_value(a1);
        sheet.cell_mut(format!("F{}", out_row)).set_value(a2);
        sheet.cell_mut(format!("G{}", out_row)).set_value(a3);
        sheet.cell_mut(format!("H{}", out_row)).set_value(&row.note);

        out_row += 1;
    }
}

fn apply_basic_format(sheet: &mut Worksheet, max_row: u32) {
    for (col, width) in [
        ("A", 18.0),
        ("B", 12.0),
        ("C", 18.0),
        ("D", 42.0),
        ("E", 22.0),
        ("F", 22.0),
        ("G", 22.0),
        ("H", 42.0),
    ] {
        sheet.column_dimension_mut(col).set_width(width);
    }

    for row in 1..=max_row {
        for col in ["A", "B", "C", "D", "E", "F", "G", "H"] {
            let addr = format!("{}{}", col, row);
            let style = sheet.style_mut(addr.as_str());
            style.alignment_mut().set_wrap_text(true);
        }
    }
}

fn col_to_letters(mut col: u32) -> String {
    let mut s = String::new();
    while col > 0 {
        let r = ((col - 1) % 26) as u8;
        s.insert(0, (b'A' + r) as char);
        col = (col - 1) / 26;
    }
    s
}