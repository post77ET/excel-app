use crate::core1::types::CandidateBundle;
use crate::core2::structure_types::{LogicalCell, LogicalCellKind};
use crate::infra::config_loader::TranslatorConfig;
use crate::ui::types::UiRow;
use crate::ui::ui_format::apply_ui_format;
use crate::ui::ui_protection::apply_ui_protection;

use umya_spreadsheet::structs::{
    Color, DataValidation, DataValidations, DataValidationValues, Fill, PatternFill,
};
use umya_spreadsheet::structs::PatternValues;
use umya_spreadsheet::structs::Style;
use umya_spreadsheet::{Spreadsheet, Worksheet};

pub fn build_ui_row(logical_cell: &LogicalCell, bundle: &CandidateBundle) -> UiRow {
    let (cell_kind, writeback_mode, default_select, note) = match logical_cell.cell_kind {
        LogicalCellKind::FormulaRaw | LogicalCellKind::SharedFormulaParent => (
            "Formula".to_string(),
            "Formula".to_string(),
            0u8,
            bundle.note.clone(),
        ),

        LogicalCellKind::SharedFormulaFollower => (
            "Formula".to_string(),
            "SharedFormulaFollower".to_string(),
            0u8,
            if bundle.note.trim().is_empty() {
                "shared formula follower: formula candidates shown, direct apply disabled (parent/group apply only)".to_string()
            } else {
                format!("{} / shared formula follower: direct apply disabled (parent/group apply only)", bundle.note)
            },
        ),

        LogicalCellKind::Date => (
            "Date".to_string(),
            "Preserve".to_string(),
            0u8,
            bundle.note.clone(),
        ),

        LogicalCellKind::Number => (
            "Number".to_string(),
            "Preserve".to_string(),
            0u8,
            bundle.note.clone(),
        ),

        LogicalCellKind::HyperlinkText => (
            "HyperlinkText".to_string(),
            "Preserve".to_string(),
            0u8,
            bundle.note.clone(),
        ),

        LogicalCellKind::Empty => (
            "Empty".to_string(),
            "Preserve".to_string(),
            0u8,
            bundle.note.clone(),
        ),

        LogicalCellKind::NonTranslatable => (
            "NonTranslatable".to_string(),
            "Preserve".to_string(),
            0u8,
            bundle.note.clone(),
        ),

        LogicalCellKind::Text => (
            "Text".to_string(),
            "DirectReplace".to_string(),
            bundle.default_select as u8,
            bundle.note.clone(),
        ),
    };

    let (candidate1, candidate2, candidate3, alarms) = (
        bundle.candidate1.clone(),
        bundle.candidate2.clone(),
        bundle.candidate3.clone(),
        bundle.alarms.clone(),
    );

    let (original, original_writeback) = match logical_cell.cell_kind {
        LogicalCellKind::SharedFormulaFollower => (String::new(), String::new()),
        _ => (bundle.original.clone(), logical_cell.source_text.clone()),
    };

    UiRow {
        writeback_allowed: logical_cell.writeback_allowed,
        logical_cell_id: logical_cell.logical_cell_id.clone(),
        sheet_name: logical_cell.sheet_name.clone(),
        anchor_address: logical_cell.anchor_address.clone(),
        cell_kind,
        original,
        original_writeback,
        writeback_mode,
        candidate1,
        candidate2,
        candidate3,
        default_select,
        user_select: None,
        apply_flag: false,
        candidate4: None,
        alarms,
        note,
    }
}

pub fn build_candidate_headers(config: &TranslatorConfig, enabled_candidates: &[u8]) -> (String, String, String) {
    let c1 = format!("candidate1 = {}", config.candidate1_provider.as_label());
    let c2 = if enabled_candidates.contains(&2) {
        format!("candidate2 = {}", config.candidate2_provider.as_label())
    } else {
        "candidate2 = None".to_string()
    };
    let c3 = if enabled_candidates.contains(&3) {
        format!("candidate3 = {}", config.candidate3_provider.as_label())
    } else {
        "candidate3 = None".to_string()
    };
    (c1, c2, c3)
}

pub fn write_ui_sheet_into_book(
    book: &mut Spreadsheet,
    rows: &[UiRow],
    config: &TranslatorConfig,
    enabled_candidates: &[u8],
) -> Result<(), String> {
    let sheet_name = "TRANSLATION_UI";

    if book.get_sheet_by_name(sheet_name).is_some() {
        let _ = book.remove_sheet_by_name(sheet_name);
    }

    let _ = book.new_sheet(sheet_name);
    let sheet = book
        .get_sheet_by_name_mut(sheet_name)
        .ok_or_else(|| "TRANSLATION_UI create error".to_string())?;

    // enabled_candidates を rows から判定
    let has_c2 = rows.iter().any(|r| r.candidate2.is_some());
    let has_c3 = rows.iter().any(|r| r.candidate3.is_some());
    let mut enabled: Vec<u8> = vec![1];
    if has_c2 { enabled.push(2); }
    if has_c3 { enabled.push(3); }

    write_headers(sheet, config, &enabled);
    write_rows(sheet, rows);
    apply_ui_format(sheet, rows.len() as u32 + 1, 17);
    apply_ui_input_validation(sheet, rows.len() as u32 + 1, enabled_candidates);
    apply_ui_protection(book, rows.len() as u32 + 1);
    Ok(())
}

fn write_headers(sheet: &mut Worksheet, config: &TranslatorConfig, enabled: &[u8]) {
    let (candidate1_header, candidate2_header, candidate3_header) = build_candidate_headers(config, enabled);
    let headers = [
        "Sheet",
        "Cell",
        "CellKind",
        "Original",
        "OriginalWriteback",
        "WritebackMode",
        candidate1_header.as_str(),
        candidate2_header.as_str(),
        candidate3_header.as_str(),
        "DefaultSelect",
        "UserSelect",
        "Apply",
        "Candidate4",
        "Candidate1Alarm",
        "Candidate2Alarm",
        "Candidate3Alarm",
        "Note",
        "WritebackAllowed",
    ];
    for (idx, header) in headers.iter().enumerate() {
        let addr = format!("{}1", col_to_letters((idx + 1) as u32));
        sheet.get_cell_mut(addr.as_str()).set_value(*header);
    }
}

fn write_rows(sheet: &mut Worksheet, rows: &[UiRow]) {
    for (idx, row_data) in rows.iter().enumerate() {
        let row = (idx + 2) as u32;

        sheet.get_cell_mut(format!("A{}", row)).set_value(&row_data.sheet_name);
        sheet.get_cell_mut(format!("B{}", row)).set_value(&row_data.anchor_address);
        sheet.get_cell_mut(format!("C{}", row)).set_value(&row_data.cell_kind);
        sheet.get_cell_mut(format!("D{}", row)).set_value(&row_data.original);
        sheet.get_cell_mut(format!("E{}", row)).set_value(&row_data.original_writeback);
        sheet.get_cell_mut(format!("F{}", row)).set_value(&row_data.writeback_mode);

        sheet.get_cell_mut(format!("G{}", row))
            .set_value(row_data.candidate1.clone().unwrap_or_default());
        sheet.get_cell_mut(format!("H{}", row))
            .set_value(row_data.candidate2.clone().unwrap_or_default());
        sheet.get_cell_mut(format!("I{}", row))
            .set_value(row_data.candidate3.clone().unwrap_or_default());

        sheet.get_cell_mut(format!("J{}", row))
            .set_value_number(row_data.default_select as i32);

        sheet.get_cell_mut(format!("K{}", row))
            .set_value(row_data.user_select.map(|v: u8| v.to_string()).unwrap_or_default());

        sheet.get_cell_mut(format!("L{}", row))
            .set_value(if row_data.apply_flag { "Y" } else { "" });

        sheet.get_cell_mut(format!("M{}", row)).set_value("");

        sheet.get_cell_mut(format!("N{}", row))
            .set_value(row_data.alarms.candidate1_alarm.clone().unwrap_or_default());
        sheet.get_cell_mut(format!("O{}", row))
            .set_value(row_data.alarms.candidate2_alarm.clone().unwrap_or_default());
        sheet.get_cell_mut(format!("P{}", row))
            .set_value(row_data.alarms.candidate3_alarm.clone().unwrap_or_default());

        sheet.get_cell_mut(format!("Q{}", row)).set_value(&row_data.note);

        sheet.get_cell_mut(format!("R{}", row))
            .set_value(if row_data.writeback_allowed { "1" } else { "0" });

        // CL-02: Preserve行でOriginalに日本語が含まれる場合はピンク背景を適用
        // （翻訳対象外だが日本語テキストが残っているセルを目視で識別しやすくする）
        if row_data.writeback_mode == "Preserve" && contains_japanese(&row_data.original) {
            let pink_style = pink_locked_style();
            for col in ["A","B","C","D","E","F","G","H","I","J","K","L","M","N","O","P","Q","R"] {
                let addr = format!("{col}{row}");
                sheet.get_cell_mut(addr.as_str()).set_style(pink_style.clone());
            }
        }
    }
}

/// UserSelect(K列)ドロップダウンの選択肢を、生成されたコースに応じて組み立てる。
/// 0(原文)と4(ユーザー入力)は常に表示し、生成された候補番号(1/2/3)だけを間に入れる。
/// 例: enabled=[1]    -> "0,1,4"
///     enabled=[1,3]  -> "0,1,3,4"
///     enabled=[1,2,3]-> "0,1,2,3,4"
fn build_user_select_options(enabled_candidates: &[u8]) -> String {
    let mut codes: Vec<u8> = vec![0];
    for n in [1u8, 2, 3] {
        if enabled_candidates.contains(&n) {
            codes.push(n);
        }
    }
    codes.push(4);
    let list = codes
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("\"{}\"", list)
}

fn apply_ui_input_validation(sheet: &mut Worksheet, max_row: u32, enabled_candidates: &[u8]) {
    sheet.remove_data_validations();
    if max_row < 2 {
        return;
    }

    let mut validations = DataValidations::default();

    let user_select_options = build_user_select_options(enabled_candidates);
    let mut user_select = DataValidation::default();
    user_select
        .set_type(DataValidationValues::List)
        .set_allow_blank(true)
        .set_formula1(user_select_options);
    user_select
        .get_sequence_of_references_mut()
        .set_sqref(format!("K2:K{}", max_row));
    validations.add_data_validation_list(user_select);

    let mut apply_flag = DataValidation::default();
    apply_flag
        .set_type(DataValidationValues::List)
        .set_allow_blank(true)
        .set_formula1("\"Y\"");
    apply_flag
        .get_sequence_of_references_mut()
        .set_sqref(format!("L2:L{}", max_row));
    validations.add_data_validation_list(apply_flag);

    sheet.set_data_validations(validations);
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

// =============================================================================
// CL-02: ピンク背景スタイルと日本語検出
// =============================================================================

/// Preserve行かつOriginalに日本語(かな・カナ)が含まれる場合のロック済みピンクスタイル
fn pink_locked_style() -> Style {
    let mut style = Style::default();
    style.get_protection_mut().set_locked(true);

    let mut color = Color::default();
    color.set_argb("FFFFC7CE"); // 薄いピンク (Excel標準の「悪い」セル色)

    let mut pattern = PatternFill::default();
    pattern.set_pattern_type(PatternValues::Solid);
    pattern.set_foreground_color(color);

    let mut fill = Fill::default();
    fill.set_pattern_fill(pattern);
    style.set_fill(fill);

    style
}

/// ひらがな・カタカナ・漢字を含むかチェック（翻訳対象外だが日本語が残っている場合の検出用）
fn contains_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c,
            '\u{3040}'..='\u{309F}' | // ひらがな
            '\u{30A0}'..='\u{30FF}' | // カタカナ
            '\u{4E00}'..='\u{9FFF}'   // CJK統合漢字（基本）
        )
    })
}
