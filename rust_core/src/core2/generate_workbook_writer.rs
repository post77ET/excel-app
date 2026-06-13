use std::path::Path;

use crate::infra::config_loader::TranslatorConfig;
use crate::security::internal_metadata::{write_internal_metadata_sheet_into_book, InternalMetadata};
use crate::security::types::SecurityReport;
use crate::ui::security_report_sheet::write_security_report_sheet_into_book;
use crate::ui::translation_warnings_sheet::write_translation_warnings_sheet_into_book;
use crate::ui::types::UiRow;
use crate::ui::ui_protection::{
    apply_generate_protection,
    load_sheet_protection_password,
    patch_datavalidation_show_error_in_file,
    patch_named_sheet_protection_in_file,
    patch_shared_formula_masters_in_file,
    INTERNAL_SHEET_NAME,
    SECURITY_REPORT_SHEET_NAME,
    UI_SHEET_NAME,
    WARNINGS_SHEET_NAME,
};
use crate::ui::ui_sheet_builder::write_ui_sheet_into_book;

pub fn write_generate_workbook(
    source_path: &str,
    rows: &[UiRow],
    output_path: &str,
    config: &TranslatorConfig,
    security_report: &SecurityReport,
) -> Result<(), String> {
    let mut book = umya_spreadsheet::reader::xlsx::read(Path::new(source_path))
        .map_err(|e| format!("source workbook read failed: {e}"))?;

    // Generate step: write default-selected text into main sheets so users can
    // review the translation result directly in the workbook before running Apply.
    write_default_selected_into_main_sheets(&mut book, rows)?;

    write_ui_sheet_into_book(&mut book, rows, config)?;
    write_security_report_sheet_into_book(&mut book, security_report)?;

    let internal = InternalMetadata::from_rows(rows, config);
    write_internal_metadata_sheet_into_book(&mut book, &internal)?;

    write_translation_warnings_sheet_into_book(&mut book, rows)?;

    let main_sheet_names = collect_target_sheet_names(rows);

    println!("[PROTECT][GENERATE] row_count = {}", rows.len());
    println!("[PROTECT][GENERATE] main_sheet_names = {:?}", main_sheet_names);

    let sheet_password = load_sheet_protection_password();
    apply_generate_protection(
        &mut book,
        &main_sheet_names,
        rows.len() as u32 + 1,
        &sheet_password,
    )?;

    umya_spreadsheet::writer::xlsx::write(&book, output_path)
        .map_err(|e| format!("generate workbook write failed: {e}"))?;

    let mut protection_targets: Vec<(&str, Option<&str>)> = main_sheet_names
        .iter()
        .map(|name| (name.as_str(), Some(sheet_password.as_str())))
        .collect();

    protection_targets.push((UI_SHEET_NAME, Some(sheet_password.as_str())));
    protection_targets.push((SECURITY_REPORT_SHEET_NAME, Some(sheet_password.as_str())));
    protection_targets.push((INTERNAL_SHEET_NAME, Some(sheet_password.as_str())));
    protection_targets.push((WARNINGS_SHEET_NAME, Some(sheet_password.as_str())));

    println!(
        "[PROTECT][GENERATE] patch_named_sheet_protection_in_file targets = {:?}",
        protection_targets
            .iter()
            .map(|(name, pw)| format!("{}:{}", name, if pw.is_some() { "PASSWORD" } else { "NONE" }))
            .collect::<Vec<_>>()
    );

    patch_named_sheet_protection_in_file(output_path, &protection_targets)?;

    patch_shared_formula_masters_in_file(output_path)?;
    patch_datavalidation_show_error_in_file(output_path)?;

    Ok(())
}

fn collect_target_sheet_names(rows: &[UiRow]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for row in rows {
        if !names.contains(&row.sheet_name) {
            names.push(row.sheet_name.clone());
        }
    }

    names
}

/// Generate時にメインシートへ DefaultSelect に基づいた翻訳テキストを書き込む。
/// - default_select == 0 (Original) → 書き込まない（原本のまま）
/// - default_select >= 1 (Candidate) → 対応するcandidateテキストを書き込む
/// - writeback_mode == "Preserve" または "SharedFormulaFollower" はスキップ
fn write_default_selected_into_main_sheets(
    book: &mut umya_spreadsheet::Spreadsheet,
    rows: &[UiRow],
) -> Result<(), String> {
    for row in rows {
        if row.writeback_mode == "Preserve" || row.writeback_mode == "SharedFormulaFollower" {
            continue;
        }

        if row.default_select == 0 {
            continue;
        }

        let selected_text = match row.default_select {
            1 => row.candidate1.as_deref(),
            2 => row.candidate2.as_deref(),
            3 => row.candidate3.as_deref(),
            4 => row.candidate4.as_deref(),
            _ => None,
        };

        let text = match selected_text {
            Some(t) if !t.is_empty() => t,
            _ => continue,
        };

        let sheet = match book.get_sheet_by_name_mut(&row.sheet_name) {
            Some(s) => s,
            None => continue,
        };

        if row.writeback_mode == "Formula" {
            let formula_body = if text.starts_with('=') {
                text[1..].to_string()
            } else {
                text.to_string()
            };
            if !formula_body.trim().is_empty() {
                sheet
                    .get_cell_mut(row.anchor_address.as_str())
                    .set_formula(formula_body);
            }
        } else {
            sheet
                .get_cell_mut(row.anchor_address.as_str())
                .set_value_string(text.to_string());
        }
    }

    Ok(())
}
