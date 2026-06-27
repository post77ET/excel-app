
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
    let mut book = crate::infra::xlsx_safe::safe_read_xlsx(base_workbook_path, "apply_workbook_writer")?;

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
    // F-3: 無効候補でベース数式を保持した行の警告（sheet, addr, message）
    let mut formula_guard_warnings: Vec<(String, String, String)> = Vec::new();

    for row in rows {
        if row.writeback_mode == "Preserve" || row.writeback_mode == "SharedFormulaFollower" {
            continue;
        }

        if !row.writeback_allowed {
            continue;
        }

        let sheet = book
            .sheet_by_name_mut(&row.sheet_name)
            .map_err(|_| format!("sheet not found: {}", row.sheet_name))?;

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

            // F-3: shared / 非shared の両経路に入る前に、候補数式を正規化＋構文検証する。
            // 無効（全角構文/空白混入など）ならベース数式を保持し警告（dt=s化を防ぐ）。
            let normalized = normalize_formula_syntax(&formula_body);
            if !is_formula_syntax_valid(&normalized) {
                println!(
                    "[F3][FORMULA][rejected->keep_base] sheet={} addr={} US={} mode={} candidate={:?}",
                    row.sheet_name, row.anchor_address, row.selected_source,
                    if shared_parent_lookup.contains(&shared_parent_key) { "shared" } else { "single" },
                    formula_body
                );
                formula_guard_warnings.push((
                    row.sheet_name.clone(),
                    row.anchor_address.clone(),
                    format!(
                        "数式候補が無効な構文のため不適用。元の数式を保持しました（候補: {}）",
                        formula_body
                    ),
                ));
                continue; // セルは変更しない（ベース数式のまま）
            }

            // shared formula parent も同じ正規化済み・検証済みの本文を使う。
            if shared_parent_lookup.contains(&shared_parent_key) {
                println!(
                    "[F3][FORMULA][shared_override] sheet={} addr={} formula={:?}",
                    row.sheet_name, row.anchor_address, normalized
                );
                shared_overrides.push(SharedFormulaOverride {
                    sheet_name: row.sheet_name.clone(),
                    anchor_address: row.anchor_address.clone(),
                    formula_body: normalized,
                });
                continue;
            }

            let addr = row.anchor_address.as_str();
            let base_formula = sheet
                .cell(addr)
                .map(|c| c.formula().to_string())
                .unwrap_or_default();

            sheet.cell_mut(addr).set_formula(normalized.clone());

            // 条件3: set_formula 後に get_formula() で read-back 確認。
            // 数式として保持されていなければベース数式へ戻し、警告する。
            let readback = sheet
                .cell(addr)
                .map(|c| (c.is_formula(), c.formula().to_string()))
                .unwrap_or((false, String::new()));
            if readback.0 && readback.1.trim() == normalized.trim() {
                println!(
                    "[F3][FORMULA][applied] sheet={} addr={} US={} formula={:?}",
                    row.sheet_name, addr, row.selected_source, normalized
                );
            } else {
                if !base_formula.trim().is_empty() {
                    sheet.cell_mut(addr).set_formula(base_formula.clone());
                }
                println!(
                    "[F3][FORMULA][readback_failed->keep_base] sheet={} addr={} US={} normalized={:?} readback={:?} base={:?}",
                    row.sheet_name, addr, row.selected_source, normalized, readback, base_formula
                );
                formula_guard_warnings.push((
                    row.sheet_name.clone(),
                    row.anchor_address.clone(),
                    format!(
                        "数式の書き戻し検証に失敗したため元の数式を保持しました（候補: {}）",
                        formula_body
                    ),
                ));
            }
        } else {
            sheet
                .cell_mut(row.anchor_address.as_str())
                .set_value_string(row.selected_text.clone());
        }
    }

    if book.sheet_by_name(INTERNAL_SHEET_NAME).is_ok() {
        let _ = book.remove_sheet_by_name(INTERNAL_SHEET_NAME);
    }

    if book.sheet_by_name(SECURITY_INTERNAL_SHEET_NAME).is_ok() {
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
    // Apply出力のUIシートは再編集しない最終記録のため、ドロップダウンはフル(1,2,3)で出力する。
    write_ui_sheet_into_book(&mut book, &ui_rows, &translator_config, &[1, 2, 3])
        .map_err(|e| format!("write_ui_sheet_into_book failed: {e}"))?;
    println!("[CL-01] TRANSLATION_UI sheet injected into apply book (single-write path)");

    // No.2 fix: ユーザーが明示選択した候補が空で原文/空にフォールバックした
    // ケースを TRANSLATION_WARNINGS シートに出力する（サイレントフェイル解消）。
    let warning_count = write_apply_warnings_sheet_into_book(&mut book, rows, &formula_guard_warnings)?;
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
        .filter(|name| book.sheet_by_name(name.as_str()).is_ok())
        .map(|name| (name.as_str(), None))
        .collect();

    // TRANSLATION_UI は上で必ず注入済みなので常に保護対象に含める。
    protection_targets.push((UI_SHEET_NAME, None));
    if book.sheet_by_name(WARNINGS_SHEET_NAME).is_ok() {
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

/// 数式構文の正規化：引用符の外側にある全角の構文記号・スマート引用符を ASCII に直す。
/// 引用符内（文字列リテラル、中国語など）は一切変更しない。
/// Excel数式の "" エスケープ（リテラル内の " 表現）に対応する。
fn normalize_formula_syntax(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut in_quote = false;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if !in_quote {
            let mapped = match ch {
                '（' => '(',
                '）' => ')',
                '：' => ':',
                '，' => ',',
                '；' => ';',
                '“' | '”' | '＂' => '"',
                other => other,
            };
            if mapped == '"' {
                in_quote = true;
            }
            out.push(mapped);
            i += 1;
        } else {
            // "" は文字列内の " （エスケープ）。引用符終端ではない。
            if ch == '"' && i + 1 < chars.len() && chars[i + 1] == '"' {
                out.push('"');
                out.push('"');
                i += 2;
                continue;
            }
            if ch == '"' || ch == '“' || ch == '”' || ch == '＂' {
                in_quote = false;
                out.push('"');
                i += 1;
            } else {
                out.push(ch);
                i += 1;
            }
        }
    }
    out
}

/// set_formula して安全な数式かを保守的に判定する。
/// NG の場合、呼び出し側はベース数式を保持し警告する（=安全側に倒す）。
/// Excel数式の "" エスケープに対応する。
fn is_formula_syntax_valid(body: &str) -> bool {
    if body.trim().is_empty() {
        return false;
    }
    let chars: Vec<char> = body.chars().collect();
    let mut depth: i32 = 0;
    let mut in_quote = false;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if in_quote {
            // "" はリテラル内のエスケープ。終端ではない。
            if ch == '"' && i + 1 < chars.len() && chars[i + 1] == '"' {
                i += 2;
                continue;
            }
            if ch == '"' {
                in_quote = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '"' => in_quote = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            // 引用符の外側に全角構文記号・スマート引用符・空白が残っていたら NG
            '（' | '）' | '：' | '，' | '；' | '“' | '”' | '＂' | '　' => return false,
            c if c.is_whitespace() => return false,
            _ => {}
        }
        i += 1;
    }
    !in_quote && depth == 0
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

fn collect_main_sheet_names_from_workbook(book: &umya_spreadsheet::Workbook) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for sheet in book.sheet_collection() {
        let name = sheet.name().to_string();

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
    book: &mut umya_spreadsheet::Workbook,
    rows: &[ApplyPayloadRow],
    extra_warnings: &[(String, String, String)],
) -> Result<usize, String> {
    let warned: Vec<&ApplyPayloadRow> = rows
        .iter()
        .filter(|r| r.apply_warning.is_some())
        .collect();

    if warned.is_empty() && extra_warnings.is_empty() {
        return Ok(0);
    }

    if book.sheet_by_name(WARNINGS_SHEET_NAME).is_ok() {
        let _ = book.remove_sheet_by_name(WARNINGS_SHEET_NAME);
    }
    let _ = book.new_sheet(WARNINGS_SHEET_NAME);

    let sheet = book
        .sheet_by_name_mut(WARNINGS_SHEET_NAME)
        .map_err(|_| "TRANSLATION_WARNINGS create error".to_string())?;

    let headers = ["Sheet", "Cell", "Source", "Warning"];
    for (idx, header) in headers.iter().enumerate() {
        let addr = format!("{}1", col_index_to_letters((idx + 1) as u32));
        sheet.cell_mut(addr.as_str()).set_value(*header);
    }

    let mut out_row: u32 = 2;
    for r in &warned {
        let msg = r.apply_warning.clone().unwrap_or_default();
        sheet.cell_mut(format!("A{}", out_row)).set_value(&r.sheet_name);
        sheet.cell_mut(format!("B{}", out_row)).set_value(&r.anchor_address);
        sheet.cell_mut(format!("C{}", out_row)).set_value(&r.selected_source);
        sheet.cell_mut(format!("D{}", out_row)).set_value(msg);
        out_row += 1;
    }
    for (sheet_name, addr, msg) in extra_warnings {
        sheet.cell_mut(format!("A{}", out_row)).set_value(sheet_name);
        sheet.cell_mut(format!("B{}", out_row)).set_value(addr);
        sheet.cell_mut(format!("C{}", out_row)).set_value("formula-guard");
        sheet.cell_mut(format!("D{}", out_row)).set_value(msg);
        out_row += 1;
    }

    for (col, width) in [("A", 18.0), ("B", 12.0), ("C", 16.0), ("D", 64.0)] {
        sheet.column_dimension_mut(col).set_width(width);
    }
    for row in 1..out_row {
        for col in ["A", "B", "C", "D"] {
            let addr = format!("{}{}", col, row);
            sheet
                .style_mut(addr.as_str())
                .alignment_mut()
                .set_wrap_text(true);
        }
    }

    Ok(warned.len() + extra_warnings.len())
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
