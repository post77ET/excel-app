use std::path::Path;

use crate::core2::shared_formula_apply_patch::{
    load_shared_formula_parent_lookup,
    patch_apply_shared_formula_groups,
    SharedFormulaOverride,
};
use crate::security::internal_metadata::INTERNAL_SHEET_NAME;
use crate::infra::config_loader::load_translator_config;
use crate::ui::ui_apply_payload::ApplyPayloadRow;
use crate::ui::ui_sheet_builder::write_ui_sheet_into_book;
use crate::ui::ui_sheet_reader::read_ui_rows;
use crate::ui::ui_protection::{
    apply_apply_output_protection,
    patch_named_sheet_protection_in_file,
    UI_SHEET_NAME,
    WARNINGS_SHEET_NAME,
};

const SECURITY_INTERNAL_SHEET_NAME: &str = "SECURITY_REPORT";

pub fn write_apply_workbook(
    base_workbook_path: &str,
    ui_workbook_path: &str,
    rows: &[ApplyPayloadRow],
    output_path: &str,
) -> Result<(), String> {
    let mut book = umya_spreadsheet::reader::xlsx::read(Path::new(base_workbook_path))
        .map_err(|e| format!("base workbook read failed: {e}"))?;

    let unlock_sheet_names = collect_main_sheet_names_from_workbook(&book);

    // No.1 fix: UIシートはApply出力を「再読込→再保存」せず、同一 book に注入する。
    // UIファイルからUiRow一覧を読み込んでおく（注入時に使用）。
    let ui_rows = read_ui_rows(ui_workbook_path)
        .map_err(|e| format!("read_ui_rows failed: {e}"))?;
    let translator_config = load_translator_config();

    println!("[PROTECT][APPLY] base_workbook_path = {}", base_workbook_path);
    println!("[PROTECT][APPLY] ui_workbook_path = {}", ui_workbook_path);
    println!("[PROTECT][APPLY] writeback_row_count = {}", rows.len());
    println!(
        "[PROTECT][APPLY] writeback_row_sheets = {:?}",
        collect_target_sheet_names(rows)
    );
    println!(
        "[PROTECT][APPLY] unlock_sheet_names_from_workbook = {:?}",
        unlock_sheet_names
    );

    let shared_parent_lookup = load_shared_formula_parent_lookup(base_workbook_path)
        .map_err(|e| format!("shared formula parent lookup failed: {e}"))?;

    let mut shared_overrides: Vec<SharedFormulaOverride> = Vec::new();

    for row in rows {
        if row.writeback_mode == "Preserve" || row.writeback_mode == "SharedFormulaFollower" {
            continue;
        }

        if !row.writeback_allowed {
            continue;
        }

        let sheet = book
            .get_sheet_by_name_mut(&row.sheet_name)
            .ok_or_else(|| format!("sheet not found: {}", row.sheet_name))?;

        if row.writeback_mode == "Formula" {
            let formula_body = normalize_formula_body(&row.selected_text);

            // CL-03: formula_body が空の場合はApply全体を止めずにスキップ
            // （selected_text が空になるケース: 疑似数式・文字列なし数式セルなど）
            if formula_body.trim().is_empty() {
                println!(
                    "[CL-03][SKIP] formula body empty: logical_id={} sheet={} addr={} selected_text={:?}",
                    row.logical_cell_id, row.sheet_name, row.anchor_address, row.selected_text
                );
                continue;
            }

            let shared_parent_key = format!("{}!{}", row.sheet_name, row.anchor_address);

            if shared_parent_lookup.contains(&shared_parent_key) {
                shared_overrides.push(SharedFormulaOverride {
                    sheet_name: row.sheet_name.clone(),
                    anchor_address: row.anchor_address.clone(),
                    formula_body,
                });
                continue;
            }

            sheet
                .get_cell_mut(row.anchor_address.as_str())
                .set_formula(formula_body);
        } else {
            sheet
                .get_cell_mut(row.anchor_address.as_str())
                .set_value_string(row.selected_text.clone());
        }
    }

    if book.get_sheet_by_name(INTERNAL_SHEET_NAME).is_some() {
        let _ = book.remove_sheet_by_name(INTERNAL_SHEET_NAME);
    }

    if book.get_sheet_by_name(SECURITY_INTERNAL_SHEET_NAME).is_some() {
        let _ = book.remove_sheet_by_name(SECURITY_INTERNAL_SHEET_NAME);
    }

    // ---------------------------------------------------------------------
    // No.1 fix: TRANSLATION_UI シートを「同じ book」に注入する。
    //
    // 旧実装は (1) メインシートを書いて保存 → (2) その出力を再読込して UI シート
    // を足して再保存、という二段書き込みだった。この「書く→読む→書く」の往復で
    // メインセル文字列が umya により再エンコードされ、改行倍増（4→8）・全角スペース
    // (U+3000) 消失が発生し、「Apply後テキスト != C1」になっていた。
    // UI シート(C1)は2回目の書き込みで1回だけエンコードされるため正しく、メイン
    // セルだけが余分な往復で壊れる、という症状と一致する。
    //
    // 注入を保存前に行い、保存を1回だけにすることで往復を排除する。
    // ---------------------------------------------------------------------
    write_ui_sheet_into_book(&mut book, &ui_rows, &translator_config)
        .map_err(|e| format!("write_ui_sheet_into_book failed: {e}"))?;
    println!("[CL-01] TRANSLATION_UI sheet injected into apply book (single-write path)");

    // No.2 fix: ユーザーが明示選択した候補が空で原文/空にフォールバックした
    // ケースを TRANSLATION_WARNINGS シートに出力する（サイレントフェイル解消）。
    let warning_count = write_apply_warnings_sheet_into_book(&mut book, rows)?;
    if warning_count > 0 {
        println!(
            "[WARN][APPLY] silent-fallback warnings written to {} sheet: {} row(s)",
            WARNINGS_SHEET_NAME, warning_count
        );
    }

    apply_apply_output_protection(&mut book, &unlock_sheet_names)?;

    // 保存は1回だけ（再読込・再保存はしない）
    umya_spreadsheet::writer::xlsx::write(&book, output_path)
        .map_err(|e| format!("apply workbook write failed: {e}"))?;

    let mut protection_targets: Vec<(&str, Option<&str>)> = unlock_sheet_names
        .iter()
        .filter(|name| book.get_sheet_by_name(name.as_str()).is_some())
        .map(|name| (name.as_str(), None))
        .collect();

    // TRANSLATION_UI は上で必ず注入済みなので常に保護対象に含める。
    protection_targets.push((UI_SHEET_NAME, None));
    if book.get_sheet_by_name(WARNINGS_SHEET_NAME).is_some() {
        protection_targets.push((WARNINGS_SHEET_NAME, None));
    }

    println!(
        "[PROTECT][APPLY] patch_named_sheet_protection_in_file targets = {:?}",
        protection_targets
            .iter()
            .map(|(name, pw)| format!("{}:{}", name, if pw.is_some() { "PASSWORD" } else { "NONE" }))
            .collect::<Vec<_>>()
    );

    patch_named_sheet_protection_in_file(output_path, &protection_targets)?;

    patch_apply_shared_formula_groups(base_workbook_path, output_path, &shared_overrides)?;

    Ok(())
}

fn normalize_formula_body(input: &str) -> String {
    let mut text = input.trim();

    if let Some(s) = text.strip_prefix('\'') {
        text = s.trim_start();
    }

    while let Some(s) = text.strip_prefix('=') {
        text = s.trim_start();
    }

    text.to_string()
}

fn collect_target_sheet_names(rows: &[ApplyPayloadRow]) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for row in rows {
        if !names.contains(&row.sheet_name) {
            names.push(row.sheet_name.clone());
        }
    }

    names
}

fn collect_main_sheet_names_from_workbook(book: &umya_spreadsheet::Spreadsheet) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for sheet in book.get_sheet_collection() {
        let name = sheet.get_name().to_string();

        if name == UI_SHEET_NAME
            || name == WARNINGS_SHEET_NAME
            || name == INTERNAL_SHEET_NAME
            || name == SECURITY_INTERNAL_SHEET_NAME
        {
            continue;
        }

        names.push(name);
    }

    names
}

// =============================================================================
// No.2: Apply時のサイレントフォールバック警告を TRANSLATION_WARNINGS に出力する
// =============================================================================

/// apply_warning を持つ行を TRANSLATION_WARNINGS シートに書き出す。
/// 警告が1件もない場合はシートを作成しない。書き出した件数を返す。
fn write_apply_warnings_sheet_into_book(
    book: &mut umya_spreadsheet::Spreadsheet,
    rows: &[ApplyPayloadRow],
) -> Result<usize, String> {
    let warned: Vec<&ApplyPayloadRow> = rows
        .iter()
        .filter(|r| r.apply_warning.is_some())
        .collect();

    if warned.is_empty() {
        return Ok(0);
    }

    if book.get_sheet_by_name(WARNINGS_SHEET_NAME).is_some() {
        let _ = book.remove_sheet_by_name(WARNINGS_SHEET_NAME);
    }
    let _ = book.new_sheet(WARNINGS_SHEET_NAME);

    let sheet = book
        .get_sheet_by_name_mut(WARNINGS_SHEET_NAME)
        .ok_or_else(|| "TRANSLATION_WARNINGS create error".to_string())?;

    let headers = ["Sheet", "Cell", "Source", "Warning"];
    for (idx, header) in headers.iter().enumerate() {
        let addr = format!("{}1", col_index_to_letters((idx + 1) as u32));
        sheet.get_cell_mut(addr.as_str()).set_value(*header);
    }

    let mut out_row: u32 = 2;
    for r in &warned {
        let msg = r.apply_warning.clone().unwrap_or_default();
        sheet.get_cell_mut(format!("A{}", out_row)).set_value(&r.sheet_name);
        sheet.get_cell_mut(format!("B{}", out_row)).set_value(&r.anchor_address);
        sheet.get_cell_mut(format!("C{}", out_row)).set_value(&r.selected_source);
        sheet.get_cell_mut(format!("D{}", out_row)).set_value(msg);
        out_row += 1;
    }

    for (col, width) in [("A", 18.0), ("B", 12.0), ("C", 16.0), ("D", 64.0)] {
        sheet.get_column_dimension_mut(col).set_width(width);
    }
    for row in 1..out_row {
        for col in ["A", "B", "C", "D"] {
            let addr = format!("{}{}", col, row);
            sheet
                .get_style_mut(addr.as_str())
                .get_alignment_mut()
                .set_wrap_text(true);
        }
    }

    Ok(warned.len())
}

fn col_index_to_letters(mut col: u32) -> String {
    let mut s = String::new();
    while col > 0 {
        let r = ((col - 1) % 26) as u8;
        s.insert(0, (b'A' + r) as char);
        col = (col - 1) / 26;
    }
    s
}
