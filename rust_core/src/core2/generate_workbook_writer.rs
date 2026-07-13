
use crate::infra::config_loader::TranslatorConfig;
use crate::entry::job_plan_settings::CandidateConfig;
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
    restore_original_drawings_in_file,
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
    enabled_candidates: &[u8],
    candidate_configs: &[CandidateConfig; 3],
) -> Result<(), String> {
    let mut book = crate::infra::xlsx_safe::safe_read_xlsx(source_path, "generate_workbook_writer")?;

    // Generate step: write default-selected text into main sheets so users can
    // review the translation result directly in the workbook before running Apply.
    write_default_selected_into_main_sheets(&mut book, rows)?;

    write_ui_sheet_into_book(&mut book, rows, config, enabled_candidates)?;
    write_security_report_sheet_into_book(&mut book, security_report)?;

    // C-2: provider/method を真実源として __ETB_INTERNAL に保存する。
    // provider/method は CandidateConfig からのみ取得（C-3 引継ぎ方針）。
    // 候補の有効/無効は「設定(enabled_candidates)」だけで判定する。
    // 【重大インシデント対応 2026-07-12】以前は rows.iter().any(|r| r.candidate2/3.is_some())
    // という「実行結果（1件でも翻訳に成功したか）」を有効/無効の判定に使っていたため、
    // 「候補は正しく有効化・実行されたが、レート制限等で全件失敗した」場合に、
    // あたかも「候補が無効化されていた」かのように誤って記録・表示されるバグがあった
    // （QA重大インシデント報告：candidate3=Google が全件レート制限で失敗した際、
    // __ETB_INTERNAL に candidate3_provider=None と誤って記録され、
    // 「使わない設定なのに裏で呼ばれている」という誤解を招いた）。
    // 「有効/無効」と「実行結果（成功件数）」は独立した別概念であり、混同してはならない。
    let c2_enabled = enabled_candidates.contains(&2);
    let c3_enabled = enabled_candidates.contains(&3);
    let provider_label = |cc: &CandidateConfig, enabled: bool| -> String {
        if enabled {
            cc.provider.map(|p| p.as_label().to_string()).unwrap_or_else(|| "None".to_string())
        } else {
            "None".to_string()
        }
    };
    let method_label = |cc: &CandidateConfig, enabled: bool| -> String {
        if enabled { cc.method.as_label().to_string() } else { "none".to_string() }
    };
    let providers = [
        provider_label(&candidate_configs[0], true),
        provider_label(&candidate_configs[1], c2_enabled),
        provider_label(&candidate_configs[2], c3_enabled),
    ];
    let methods = [
        method_label(&candidate_configs[0], true),
        method_label(&candidate_configs[1], c2_enabled),
        method_label(&candidate_configs[2], c3_enabled),
    ];
    let internal = InternalMetadata::from_rows(rows, config, &providers, &methods);
    write_internal_metadata_sheet_into_book(&mut book, &internal)?;

    write_translation_warnings_sheet_into_book(&mut book, rows)?;

    // 体験コース(A1:D5のみ)でも「触らないシート/範囲」を含め全シートをパスワード保護する。
    // rows 由来だと体験コースで未処理シートが無保護になるため、ブック内の全シート
    //（特殊シートを除く）を対象にして標準コースと同一の保護を適用する。
    let main_sheet_names = collect_all_main_sheet_names(&book);

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

    // 図形・画像（drawing*.xml）は翻訳処理で変更する必要が無いため、
    // umyaの往復処理で壊れる可能性のあるグループ図形等を、元ファイルの
    // バイト列でそのまま復元する（QA報告: ネストしたグループ図形の位置ズレ対応）。
    if let Err(e) = restore_original_drawings_in_file(source_path, output_path, &main_sheet_names) {
        println!("[GENERATE][RESTORE_DRAWINGS][WARN] {e}");
    }

    Ok(())
}

fn collect_all_main_sheet_names(book: &umya_spreadsheet::Workbook) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for sheet in book.sheet_collection() {
        let name = sheet.name().to_string();

        // 特殊シートは protection_targets 側で個別に保護するため除外する。
        if name == UI_SHEET_NAME
            || name == SECURITY_REPORT_SHEET_NAME
            || name == INTERNAL_SHEET_NAME
            || name == WARNINGS_SHEET_NAME
        {
            continue;
        }

        names.push(name);
    }

    names
}

/// Generate時にメインシートへ DefaultSelect に基づいた翻訳テキストを書き込む。
/// - default_select == 0 (Original) → 書き込まない（原本のまま）
/// - default_select >= 1 (Candidate) → 対応するcandidateテキストを書き込む
/// - writeback_mode == "Preserve" または "SharedFormulaFollower" はスキップ
fn write_default_selected_into_main_sheets(
    book: &mut umya_spreadsheet::Workbook,
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

        let sheet = match book.sheet_by_name_mut(&row.sheet_name) {
            Ok(s) => s,
            Err(_) => continue,
        };

        if row.writeback_mode == "Formula" {
            let formula_body = if text.starts_with('=') {
                text[1..].to_string()
            } else {
                text.to_string()
            };
            if !formula_body.trim().is_empty() {
                sheet
                    .cell_mut(row.anchor_address.as_str())
                    .set_formula(formula_body);
            }
        } else {
            sheet
                .cell_mut(row.anchor_address.as_str())
                .set_value_string(text.to_string());
        }
    }

    Ok(())
}
