use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::Read;

use calamine::{open_workbook_auto, Data, Range, Reader};
use zip::ZipArchive;

use crate::core2::protected_text_detector::is_protected_transport_text;
use crate::core2::shared_formula_reader::{parse_formula_cells, FormulaMeta};
use crate::core2::structure_types::{LogicalCell, LogicalCellKind};
use crate::infra::app_error::AppError;

pub fn read_source_logical_cells() -> Result<Vec<LogicalCell>, AppError> {
    let source_path = env::var("ETB_INPUT_PATH").unwrap_or_else(|_| "TEST_work.xlsx".to_string());

    let mut workbook = open_workbook_auto(&source_path)
        .map_err(|e| AppError::WorkbookReadFailed(format!("open workbook failed: {e}")))?;

    let workbook_sheet_names = workbook.sheet_names().to_vec();
    if workbook_sheet_names.is_empty() {
        return Err(AppError::WorkbookReadFailed("workbook has no worksheets".to_string()));
    }

    let selected_sheets = resolve_reader_target_sheets(&workbook_sheet_names)?;

    let display_book = crate::infra::xlsx_safe::safe_read_xlsx(&source_path, "source_workbook_reader")
        .map_err(AppError::WorkbookReadFailed)?;

    println!(
        "[SOURCE_READER] workbook={} selected_sheets={}",
        source_path,
        selected_sheets.join(",")
    );

    let mut logical_cells = Vec::new();

    for target_sheet in selected_sheets {
        let range = workbook
            .worksheet_range(&target_sheet)
            .map_err(|e| AppError::WorkbookReadFailed(format!("sheet read failed: {e}")))?;

        let display_sheet = display_book.sheet_by_name(&target_sheet).map_err(|_| {
            AppError::WorkbookReadFailed(format!("sheet not found in umya: {target_sheet}"))
        })?;

        let sheet_xml = read_target_sheet_xml(&source_path, &target_sheet).unwrap_or_default();
        let formula_cells = parse_formula_cells(&sheet_xml);
        let hyperlink_cells = find_hyperlink_refs(&sheet_xml);
        let used_area = used_area_from_sheet_xml(&sheet_xml, &range);

        println!(
            "[SOURCE_READER] start file={} sheet={} range_h={} range_w={} used_start={}{} used_end={}{}",
            source_path,
            target_sheet,
            range.height(),
            range.width(),
            col_to_letters(used_area.start_col as u32),
            used_area.start_row,
            col_to_letters(used_area.end_col as u32),
            used_area.end_row,
        );

        for row in used_area.start_row..=used_area.end_row {
            for col in used_area.start_col..=used_area.end_col {
                let anchor_address = format!("{}{}", col_to_letters(col as u32), row);
                let abs = ((row - 1) as u32, (col - 1) as u32);

                let value = range.get_value(abs);
                let display_text = display_sheet.value(anchor_address.as_str()).to_string();
                let formula_meta = formula_cells
                    .get(&anchor_address)
                    .cloned()
                    .unwrap_or_default();

                let cell_kind = detect_cell_kind(
                    &anchor_address,
                    &formula_meta,
                    value,
                    &display_text,
                    &hyperlink_cells,
                );
                let source_text = build_source_text(cell_kind, &formula_meta, &display_text, value);

                if is_trace_target(&anchor_address) {
                    let value_debug = cell_to_string(value);
                    println!(
                        "[SOURCE_READER] sheet={} addr={} raw_value={:?} display={:?} has_f={} formula_xml={:?} resolved_formula={:?} shared_parent={} shared_follower={} si={:?} kind={:?} source_text={:?}",
                        target_sheet,
                        anchor_address,
                        value_debug,
                        display_text,
                        formula_meta.has_formula_tag,
                        formula_meta.formula_text,
                        formula_meta.resolved_formula_text,
                        formula_meta.is_shared_parent,
                        formula_meta.is_shared_follower,
                        formula_meta.shared_index,
                        cell_kind,
                        source_text
                    );
                }

                if cell_kind == LogicalCellKind::Empty && source_text.trim().is_empty() {
                    continue;
                }

                logical_cells.push(LogicalCell {
                    is_merged: false,
                    is_merge_anchor: false,
                    merge_anchor_address: None,
                    writeback_allowed: true,
                    logical_cell_id: format!("{}!{}", target_sheet, anchor_address),
                    sheet_name: target_sheet.clone(),
                    anchor_address,
                    cell_kind,
                    source_text,
                });
            }
        }

        println!("[SOURCE_READER] end sheet={} accumulated_logical_cells={}", target_sheet, logical_cells.len());
    }

    println!("[SOURCE_READER] end logical_cells={}", logical_cells.len());

    Ok(logical_cells)
}

fn resolve_reader_target_sheets(workbook_sheet_names: &[String]) -> Result<Vec<String>, AppError> {
    if let Ok(target_sheet) = env::var("ETB_TARGET_SHEET") {
        let target_sheet = target_sheet.trim();
        if !target_sheet.is_empty() {
            if workbook_sheet_names.iter().any(|name| name == target_sheet) {
                return Ok(vec![target_sheet.to_string()]);
            }
            return Err(AppError::WorkbookReadFailed(format!("selected sheet not found: {target_sheet}")));
        }
    }

    let selected = env::var("ETB_SELECTED_SHEETS").unwrap_or_else(|_| "all".to_string());
    let selected = selected.trim();
    if selected.is_empty() || selected.eq_ignore_ascii_case("all") {
        return Ok(workbook_sheet_names.to_vec());
    }

    let mut resolved = Vec::new();
    let mut seen = HashSet::new();

    for raw_token in selected.split(',') {
        let token = raw_token.trim();
        if token.is_empty() {
            return Err(AppError::WorkbookReadFailed("ETB_SELECTED_SHEETS contains an empty token".to_string()));
        }

        let sheet_name = if let Ok(index) = token.parse::<usize>() {
            if index == 0 || index > workbook_sheet_names.len() {
                return Err(AppError::WorkbookReadFailed(format!(
                    "selected sheet index out of range: {} / sheet_count={}",
                    index,
                    workbook_sheet_names.len()
                )));
            }
            workbook_sheet_names[index - 1].clone()
        } else {
            workbook_sheet_names
                .iter()
                .find(|name| name.as_str() == token)
                .cloned()
                .ok_or_else(|| AppError::WorkbookReadFailed(format!("selected sheet not found: {token}")))?
        };

        if seen.insert(sheet_name.clone()) {
            resolved.push(sheet_name);
        }
    }

    if resolved.is_empty() {
        return Err(AppError::WorkbookReadFailed("no selected worksheets resolved".to_string()));
    }

    Ok(resolved)
}

fn detect_cell_kind(
    anchor_address: &str,
    formula_meta: &FormulaMeta,
    value: Option<&Data>,
    display_text: &str,
    hyperlink_cells: &HashSet<String>,
) -> LogicalCellKind {
    if hyperlink_cells.contains(anchor_address) {
        return LogicalCellKind::HyperlinkText;
    }

    // PROTECTED TEXT DETECTION LAYER
    //
    // This is intentionally placed in SOURCE_READER, before CORE1
    // segmentation/batching/provider dispatch. URL-like strings, mail
    // addresses, file paths, and HYPERLINK() targets are transport tokens,
    // not translation text. Treating this as a named reader rule prevents
    // future cleanup from deleting it as if it were disposable special-case IF logic.
    if is_protected_transport_text(display_text) || is_protected_transport_text(&cell_to_string(value)) {
        return LogicalCellKind::HyperlinkText;
    }

    if formula_meta.has_formula_tag {
        let formula_text = if !formula_meta.formula_text.trim().is_empty() {
            normalize_formula_text(&formula_meta.formula_text)
        } else if let Some(resolved) = &formula_meta.resolved_formula_text {
            normalize_formula_text(resolved)
        } else {
            String::new()
        };

        if is_protected_transport_text(&formula_text) {
            return LogicalCellKind::HyperlinkText;
        }
    }

    if formula_meta.is_shared_follower {
        return LogicalCellKind::SharedFormulaFollower;
    }

    if formula_meta.is_shared_parent {
        return LogicalCellKind::SharedFormulaParent;
    }

    if formula_meta.has_formula_tag {
        return LogicalCellKind::FormulaRaw;
    }


    match value {
        Some(Data::DateTime(_)) | Some(Data::DateTimeIso(_)) => LogicalCellKind::Date,
        Some(Data::Float(_)) | Some(Data::Int(_)) => LogicalCellKind::Number,
        Some(Data::Empty) | None => LogicalCellKind::Empty,
        _ => LogicalCellKind::Text,
    }
}

fn build_source_text(
    cell_kind: LogicalCellKind,
    formula_meta: &FormulaMeta,
    display_text: &str,
    value: Option<&Data>,
) -> String {
    match cell_kind {
        LogicalCellKind::FormulaRaw | LogicalCellKind::SharedFormulaParent => {
            if !formula_meta.formula_text.trim().is_empty() {
                normalize_formula_text(&formula_meta.formula_text)
            } else {
                normalize_formula_text(&cell_to_string(value))
            }
        }
        LogicalCellKind::SharedFormulaFollower => {
            if let Some(resolved) = &formula_meta.resolved_formula_text {
                normalize_formula_text(resolved)
            } else if !display_text.trim().is_empty() {
                display_text.to_string()
            } else {
                cell_to_string(value)
            }
        }
        LogicalCellKind::Date => {
            if !display_text.trim().is_empty() {
                display_text.to_string()
            } else {
                cell_to_string(value)
            }
        }
        LogicalCellKind::Number | LogicalCellKind::Text | LogicalCellKind::HyperlinkText => {
            if !display_text.is_empty() {
                display_text.to_string()
            } else {
                cell_to_string(value)
            }
        }
        LogicalCellKind::Empty | LogicalCellKind::NonTranslatable => String::new(),
    }
}

fn normalize_formula_text(formula_text: &str) -> String {
    let trimmed = formula_text.trim();
    if trimmed.is_empty() {
        String::new()
    } else if trimmed.starts_with('=') {
        trimmed.to_string()
    } else {
        format!("={trimmed}")
    }
}


fn cell_to_string(cell: Option<&Data>) -> String {
    match cell {
        Some(Data::String(s)) => s.clone(),
        Some(Data::Float(v)) => {
            if v.fract() == 0.0 {
                format!("{v:.0}")
            } else {
                v.to_string()
            }
        }
        Some(Data::Int(v)) => v.to_string(),
        Some(Data::Bool(v)) => v.to_string(),
        Some(Data::DateTimeIso(s)) => s.clone(),
        Some(Data::DurationIso(s)) => s.clone(),
        Some(Data::DateTime(_)) => String::new(),
        Some(Data::Error(e)) => format!("{e:?}"),
        _ => String::new(),
    }
}

fn is_trace_target(addr: &str) -> bool {
    matches!(
        addr,
        "A1" | "A2" | "A3" | "A4" | "A5" | "A6" | "A7" | "A8" | "A9" | "A10" | "A11" | "A12"
    )
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

fn read_target_sheet_xml(source_path: &str, target_sheet: &str) -> Result<String, String> {
    let file = File::open(source_path).map_err(|e| format!("source open failed: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("zip open failed: {e}"))?;

    let workbook_xml = read_zip_entry(&mut zip, "xl/workbook.xml")?;
    let workbook_rels = read_zip_entry(&mut zip, "xl/_rels/workbook.xml.rels")?;

    let sheet_rid = find_sheet_rid(&workbook_xml, target_sheet)
        .ok_or_else(|| format!("sheet rid not found: {target_sheet}"))?;
    let sheet_target = find_relationship_target(&workbook_rels, &sheet_rid)
        .ok_or_else(|| format!("sheet target not found: {sheet_rid}"))?;

    read_zip_entry(&mut zip, &normalize_xl_path(&sheet_target))
}

fn read_zip_entry(zip: &mut ZipArchive<File>, path: &str) -> Result<String, String> {
    let mut file = zip.by_name(path).map_err(|e| format!("zip entry not found {path}: {e}"))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .map_err(|e| format!("zip entry read failed {path}: {e}"))?;
    Ok(buf)
}

fn find_sheet_rid(workbook_xml: &str, target_sheet: &str) -> Option<String> {
    for tag in find_tags(workbook_xml, "sheet") {
        if extract_attr(&tag, "name").as_deref() == Some(target_sheet) {
            return extract_attr(&tag, "r:id");
        }
    }
    None
}

fn find_relationship_target(rels_xml: &str, rid: &str) -> Option<String> {
    for tag in find_tags(rels_xml, "Relationship") {
        if extract_attr(&tag, "Id").as_deref() == Some(rid) {
            return extract_attr(&tag, "Target");
        }
    }
    None
}

fn find_hyperlink_refs(sheet_xml: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for tag in find_tags(sheet_xml, "hyperlink") {
        if let Some(reference) = extract_attr(&tag, "ref") {
            for expanded_ref in expand_cell_ref_or_range(&reference) {
                out.insert(expanded_ref);
            }
        }
    }
    out
}

fn expand_cell_ref_or_range(reference: &str) -> Vec<String> {
    let trimmed = reference.trim();
    if !trimmed.contains(':') {
        return vec![trimmed.to_string()];
    }

    let mut parts = trimmed.split(':');
    let Some(start_ref) = parts.next() else {
        return vec![trimmed.to_string()];
    };
    let Some(end_ref) = parts.next() else {
        return vec![trimmed.to_string()];
    };

    let Some((start_row, start_col)) = parse_cell_ref(start_ref) else {
        return vec![trimmed.to_string()];
    };
    let Some((end_row, end_col)) = parse_cell_ref(end_ref) else {
        return vec![trimmed.to_string()];
    };

    let row_min = start_row.min(end_row);
    let row_max = start_row.max(end_row);
    let col_min = start_col.min(end_col);
    let col_max = start_col.max(end_col);

    let mut refs = Vec::new();
    for row in row_min..=row_max {
        for col in col_min..=col_max {
            refs.push(format!("{}{}", col_to_letters(col as u32), row));
        }
    }

    refs
}

fn find_tags(xml: &str, tag_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = format!("<{tag_name} ");
    let mut pos = 0usize;
    while let Some(start_rel) = xml[pos..].find(&needle) {
        let start = pos + start_rel;
        let rest = &xml[start..];
        if let Some(end_rel) = rest.find('>') {
            out.push(rest[..=end_rel].to_string());
            pos = start + end_rel + 1;
        } else {
            break;
        }
    }
    out
}

fn extract_attr(tag: &str, attr_name: &str) -> Option<String> {
    let needle = format!("{attr_name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn normalize_xl_path(target: &str) -> String {
    let trimmed = target.trim_start_matches("../");
    if trimmed.starts_with("xl/") {
        trimmed.to_string()
    } else {
        format!("xl/{trimmed}")
    }
}

#[derive(Debug, Clone, Copy)]
struct UsedArea {
    start_row: usize,
    end_row: usize,
    start_col: usize,
    end_col: usize,
}

fn used_area_from_sheet_xml(sheet_xml: &str, range: &Range<Data>) -> UsedArea {
    let mut start_row = usize::MAX;
    let mut end_row = 0usize;
    let mut start_col = usize::MAX;
    let mut end_col = 0usize;

    for tag in find_tags(sheet_xml, "dimension") {
        if let Some(reference) = extract_attr(&tag, "ref") {
            if let Some((ref_start_row, ref_start_col, ref_end_row, ref_end_col)) =
                parse_dimension_ref(&reference)
            {
                start_row = start_row.min(ref_start_row);
                end_row = end_row.max(ref_end_row);
                start_col = start_col.min(ref_start_col);
                end_col = end_col.max(ref_end_col);
            }
        }
    }

    for tag in find_tags(sheet_xml, "c") {
        if let Some(reference) = extract_attr(&tag, "r") {
            if let Some((row, col)) = parse_cell_ref(&reference) {
                start_row = start_row.min(row);
                end_row = end_row.max(row);
                start_col = start_col.min(col);
                end_col = end_col.max(col);
            }
        }
    }

    if start_row == usize::MAX || start_col == usize::MAX || end_row == 0 || end_col == 0 {
        let fallback_start_row = 1usize;
        let fallback_start_col = 1usize;
        let fallback_end_row = range.height().max(1);
        let fallback_end_col = range.width().max(1);
        return UsedArea {
            start_row: fallback_start_row,
            end_row: fallback_end_row,
            start_col: fallback_start_col,
            end_col: fallback_end_col,
        };
    }

    UsedArea {
        start_row,
        end_row,
        start_col,
        end_col,
    }
}

fn parse_dimension_ref(reference: &str) -> Option<(usize, usize, usize, usize)> {
    let mut parts = reference.split(':');
    let start_ref = parts.next()?;
    let end_ref = parts.next().unwrap_or(start_ref);

    let (start_row, start_col) = parse_cell_ref(start_ref)?;
    let (end_row, end_col) = parse_cell_ref(end_ref)?;

    Some((start_row, start_col, end_row, end_col))
}

fn parse_cell_ref(reference: &str) -> Option<(usize, usize)> {
    let mut letters = String::new();
    let mut digits = String::new();
    for ch in reference.chars() {
        if ch.is_ascii_alphabetic() {
            letters.push(ch);
        } else if ch.is_ascii_digit() {
            digits.push(ch);
        }
    }
    if letters.is_empty() || digits.is_empty() {
        return None;
    }
    let row = digits.parse::<usize>().ok()?;
    let col = letters_to_col(&letters)?;
    Some((row, col))
}

fn letters_to_col(letters: &str) -> Option<usize> {
    let mut value: usize = 0;
    for ch in letters.chars() {
        if !ch.is_ascii_alphabetic() {
            return None;
        }
        value = value * 26 + ((ch.to_ascii_uppercase() as u8 - b'A' + 1) as usize);
    }
    Some(value)
}