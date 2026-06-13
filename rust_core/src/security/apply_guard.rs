use std::collections::HashMap;

use calamine::{open_workbook_auto, Data, Reader};

use crate::security::internal_metadata::{compute_immutable_hash, INTERNAL_APP_ID, INTERNAL_SHEET_NAME, INTERNAL_VERSION};

pub fn validate_apply_input_workbook(input_path: &str) -> Result<(), String> {
    let mut workbook = open_workbook_auto(input_path)
        .map_err(|e| format!("apply security open failed: {e}"))?;

    let internal_range = workbook
        .worksheet_range(INTERNAL_SHEET_NAME)
        .map_err(|e| format!("apply security missing internal sheet: {e}"))?;
    let ui_range = workbook
        .worksheet_range("TRANSLATION_UI")
        .map_err(|e| format!("apply security missing TRANSLATION_UI: {e}"))?;

    let internal_map = read_internal_map(&internal_range);
    validate_internal_identity(&internal_map)?;
    validate_ui_headers(&ui_range, &internal_map)?;
    validate_ui_row_count(&ui_range, &internal_map)?;
    validate_immutable_hash(&ui_range, &internal_map)?;

    Ok(())
}

fn validate_internal_identity(map: &HashMap<String, String>) -> Result<(), String> {
    let app_id = map.get("app_id").cloned().unwrap_or_default();
    if app_id != INTERNAL_APP_ID {
        return Err(format!("apply security app_id mismatch: '{}'", app_id));
    }

    let version = map.get("version").cloned().unwrap_or_default();
    if version != INTERNAL_VERSION {
        return Err(format!("apply security version mismatch: '{}'", version));
    }

    let ui_sheet_name = map.get("ui_sheet_name").cloned().unwrap_or_default();
    if ui_sheet_name != "TRANSLATION_UI" {
        return Err(format!("apply security ui_sheet_name mismatch: '{}'", ui_sheet_name));
    }

    Ok(())
}

fn validate_ui_headers(
    ui_range: &calamine::Range<Data>,
    map: &HashMap<String, String>,
) -> Result<(), String> {
    let expected = [
        "Sheet".to_string(),
        "Cell".to_string(),
        "CellKind".to_string(),
        "Original".to_string(),
        "OriginalWriteback".to_string(),
        "WritebackMode".to_string(),
        map.get("candidate1_header").cloned().unwrap_or_default(),
        map.get("candidate2_header").cloned().unwrap_or_default(),
        map.get("candidate3_header").cloned().unwrap_or_default(),
        "DefaultSelect".to_string(),
        "UserSelect".to_string(),
        "Apply".to_string(),
        "Candidate4".to_string(),
        "Candidate1Alarm".to_string(),
        "Candidate2Alarm".to_string(),
        "Candidate3Alarm".to_string(),
        "Note".to_string(),
    ];

    for (idx, expected_value) in expected.iter().enumerate() {
        let actual = cell_string(ui_range, 0, idx);
        if actual != *expected_value {
            return Err(format!(
                "apply security header mismatch at col {}: expected='{}' actual='{}'",
                idx + 1,
                expected_value,
                actual
            ));
        }
    }

    Ok(())
}

fn validate_ui_row_count(
    ui_range: &calamine::Range<Data>,
    map: &HashMap<String, String>,
) -> Result<(), String> {
    let expected = map
        .get("row_count")
        .and_then(|v| v.parse::<usize>().ok())
        .ok_or_else(|| "apply security row_count missing".to_string())?;

    let actual = collect_ui_rows(ui_range).len();
    if actual != expected {
        return Err(format!(
            "apply security row_count mismatch: expected={} actual={}",
            expected, actual
        ));
    }

    Ok(())
}

fn validate_immutable_hash(
    ui_range: &calamine::Range<Data>,
    map: &HashMap<String, String>,
) -> Result<(), String> {
    let candidate1_header = map.get("candidate1_header").cloned().unwrap_or_default();
    let candidate2_header = map.get("candidate2_header").cloned().unwrap_or_default();
    let candidate3_header = map.get("candidate3_header").cloned().unwrap_or_default();
    let expected = map.get("immutable_hash").cloned().unwrap_or_default();
    if expected.is_empty() {
        return Err("apply security immutable_hash missing".to_string());
    }

    let rows = collect_ui_rows(ui_range);
    let actual = compute_immutable_hash(&rows, &candidate1_header, &candidate2_header, &candidate3_header);
    if actual != expected {
        log_immutable_hash_mismatch(
            &rows,
            &candidate1_header,
            &candidate2_header,
            &candidate3_header,
            &expected,
            &actual,
        );
        return Err(format!(
            "apply security immutable_hash mismatch: expected={} actual={}",
            expected, actual
        ));
    }

    println!(
        "[APPLY HASH] OK row_count={} immutable_hash={}",
        rows.len(), actual
    );
    Ok(())
}

fn log_immutable_hash_mismatch(
    rows: &[crate::ui::types::UiRow],
    candidate1_header: &str,
    candidate2_header: &str,
    candidate3_header: &str,
    expected: &str,
    actual: &str,
) {
    println!("[APPLY HASH] ===== MISMATCH DEBUG START =====");
    println!("[APPLY HASH] expected={}", expected);
    println!("[APPLY HASH] actual={}", actual);
    println!("[APPLY HASH] row_count={}", rows.len());
    println!("[APPLY HASH] candidate1_header={}", candidate1_header);
    println!("[APPLY HASH] candidate2_header={}", candidate2_header);
    println!("[APPLY HASH] candidate3_header={}", candidate3_header);

    let max_rows = rows.len().min(30);
    for (idx, row) in rows.iter().take(max_rows).enumerate() {
        println!(
            "[APPLY HASH][ROW {}] logical_cell_id='{}' sheet='{}' cell='{}' kind='{}' original='{}' original_writeback='{}' writeback_mode='{}' c1={:?} c2={:?} c3={:?} default_select={} alarm1={:?} alarm2={:?} alarm3={:?} note='{}'",
            idx + 1,
            row.logical_cell_id,
            row.sheet_name,
            row.anchor_address,
            row.cell_kind,
            row.original,
            row.original_writeback,
            row.writeback_mode,
            row.candidate1,
            row.candidate2,
            row.candidate3,
            row.default_select,
            row.alarms.candidate1_alarm,
            row.alarms.candidate2_alarm,
            row.alarms.candidate3_alarm,
            row.note
        );
    }

    if rows.len() > max_rows {
        println!(
            "[APPLY HASH] omitted_rows={} because debug log limit is 30",
            rows.len() - max_rows
        );
    }
    println!("[APPLY HASH] ===== MISMATCH DEBUG END =====");
}

fn read_internal_map(range: &calamine::Range<Data>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for row_idx in 0..range.height() {
        let key = cell_string(range, row_idx, 0);
        let value = cell_string(range, row_idx, 1);
        if !key.trim().is_empty() {
            map.insert(key, value);
        }
    }
    map
}

fn collect_ui_rows(ui_range: &calamine::Range<Data>) -> Vec<crate::ui::types::UiRow> {
    let mut rows = Vec::new();
    for row_idx in 1..ui_range.height() {
        let sheet_name_val = cell_string(ui_range, row_idx, 0);
        let anchor_address = cell_string(ui_range, row_idx, 1);
        if sheet_name_val.trim().is_empty() || anchor_address.trim().is_empty() {
            continue;
        }
        rows.push(crate::ui::types::UiRow {
            writeback_allowed: true,
            logical_cell_id: format!("{}!{}", sheet_name_val, anchor_address),
            sheet_name: sheet_name_val,
            anchor_address,
            cell_kind: cell_string(ui_range, row_idx, 2),
            original: cell_string(ui_range, row_idx, 3),
            original_writeback: cell_string(ui_range, row_idx, 4),
            writeback_mode: cell_string(ui_range, row_idx, 5),
            candidate1: opt_string(cell_string(ui_range, row_idx, 6)),
            candidate2: opt_string(cell_string(ui_range, row_idx, 7)),
            candidate3: opt_string(cell_string(ui_range, row_idx, 8)),
            default_select: opt_u8(cell_string(ui_range, row_idx, 9)).unwrap_or(0),
            user_select: opt_u8(cell_string(ui_range, row_idx, 10)),
            apply_flag: matches!(cell_string(ui_range, row_idx, 11).trim(), "Y"),
            candidate4: opt_string(cell_string(ui_range, row_idx, 12)),
            alarms: crate::core1::types::CandidateAlarms {
                candidate1_alarm: opt_string(cell_string(ui_range, row_idx, 13)),
                candidate2_alarm: opt_string(cell_string(ui_range, row_idx, 14)),
                candidate3_alarm: opt_string(cell_string(ui_range, row_idx, 15)),
            },
            note: cell_string(ui_range, row_idx, 16),
        });
    }
    rows
}

fn cell_string(range: &calamine::Range<Data>, row: usize, col: usize) -> String {
    match range.get((row, col)) {
        Some(Data::String(s)) => s.clone(),
        Some(Data::Float(v)) => v.to_string(),
        Some(Data::Int(v)) => v.to_string(),
        Some(Data::Bool(v)) => v.to_string(),
        Some(Data::DateTime(v)) => v.to_string(),
        Some(Data::DateTimeIso(s)) => s.clone(),
        Some(Data::DurationIso(s)) => s.clone(),
        Some(Data::Error(e)) => format!("{:?}", e),
        _ => String::new(),
    }
}

fn opt_string(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

fn opt_u8(value: String) -> Option<u8> {
    let trimmed = value.replace('　', "").trim().to_string();
    if trimmed.is_empty() { None } else { trimmed.parse::<u8>().ok() }
}
