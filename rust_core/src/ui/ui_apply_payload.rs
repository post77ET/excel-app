use crate::ui::types::UiRow;

#[derive(Debug, Clone)]
pub struct ApplyPayloadRow {
    pub writeback_allowed: bool,
    pub logical_cell_id: String,
    pub sheet_name: String,
    pub anchor_address: String,
    pub selected_text: String,
    pub selected_source: String,
    pub writeback_mode: String,
    /// No.2: ユーザーが明示選択した候補が空で、通知なく原文維持/空書き込みに
    /// フォールバックした場合の警告メッセージ。なければ None。
    pub apply_warning: Option<String>,
}

pub fn build_apply_payload(rows: &[UiRow]) -> Vec<ApplyPayloadRow> {
    rows.iter()
        .map(|row| {
            // apply_flag=trueの場合はUSを優先、falseの場合はDefaultSelectを使用
            let select_code = if row.apply_flag {
                row.user_select.unwrap_or(row.default_select)
            } else {
                row.default_select
            };
            let (selected_text, selected_source) = resolve_selected_value(row, select_code);

            // No.2: ユーザーが明示的に候補を選んだ（apply_flag=true かつ US 指定あり）のに
            // その候補が空で、原文維持/空書き込みへ"通知なく"フォールバックしたケースを検出する。
            let apply_warning =
                detect_silent_fallback(row, select_code);

            if let Some(msg) = &apply_warning {
                println!(
                    "[WARN][APPLY] silent fallback: sheet={} addr={} {}",
                    row.sheet_name, row.anchor_address, msg
                );
            }

            ApplyPayloadRow {
                writeback_allowed: row.writeback_allowed,
                logical_cell_id: row.logical_cell_id.clone(),
                sheet_name: row.sheet_name.clone(),
                anchor_address: row.anchor_address.clone(),
                selected_text,
                selected_source,
                writeback_mode: row.writeback_mode.clone(),
                apply_warning,
            }
        })
        .collect()
}

/// ユーザーが明示選択した候補が空のままフォールバックしたかを判定する。
/// 明示選択でない（DefaultSelectのみ）の場合や、code=0(原文)選択は警告対象外。
fn detect_silent_fallback(row: &UiRow, select_code: u8) -> Option<String> {
    // 明示的なユーザー選択でなければ警告しない（DefaultSelectのフォールバックは仕様内）。
    if !(row.apply_flag && row.user_select.is_some()) {
        return None;
    }

    match select_code {
        1 if row.candidate1.is_none() => Some(
            "UserSelect=1 を指定しましたが Candidate1 が未生成のため、DefaultSelectの候補にクランプしました（Apply時フォールバック）".to_string(),
        ),
        2 if row.candidate2.is_none() => Some(
            "UserSelect=2 を指定しましたが Candidate2 が未生成のため、DefaultSelectの候補にクランプしました（Apply時フォールバック）".to_string(),
        ),
        3 if row.candidate3.is_none() => Some(
            "UserSelect=3 を指定しましたが Candidate3 が未生成のため、DefaultSelectの候補にクランプしました（Apply時フォールバック）".to_string(),
        ),
        4 if row
            .candidate4
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true) =>
        {
            Some(
                "UserSelect=4 を指定しましたが ユーザー入力（M列/Candidate4）が空のため、空文字を書き込みました（Apply時フォールバック）".to_string(),
            )
        }
        _ => None,
    }
}

fn resolve_selected_value(row: &UiRow, select_code: u8) -> (String, String) {
    // まず選択コードで解決を試みる。
    if let Some(v) = resolve_candidate_text(row, select_code) {
        return v;
    }

    // 選択された候補がコース未生成(None)/範囲外だった場合：
    // オートフィルや貼り付けで不正値が入っても結果が壊れないよう、
    // DefaultSelect（必ず生成済みの候補）にクランプする。
    if let Some((text, source)) = resolve_candidate_text(row, row.default_select) {
        return (text, format!("{} (clamped from US={})", source, select_code));
    }

    // 最終手段：原文を書き戻す（DefaultSelectすら解決できない稀なケース）。
    (row.original_writeback.clone(), "Original".to_string())
}

/// 指定コードの候補テキストを返す。生成されていない候補(None)は None を返す。
/// code 0(原文) と code 4(ユーザー入力) はユーザーの明示意図として常に Some を返し、
/// クランプ対象外とする（意図的な原文/空欄を勝手に書き換えない）。
fn resolve_candidate_text(row: &UiRow, select_code: u8) -> Option<(String, String)> {
    match select_code {
        0 => Some((row.original_writeback.clone(), "Original".to_string())),
        1 => row.candidate1.clone().map(|t| (t, "Candidate1".to_string())),
        2 => row.candidate2.clone().map(|t| (t, "Candidate2".to_string())),
        3 => row.candidate3.clone().map(|t| (t, "Candidate3".to_string())),
        4 => Some((
            row.candidate4.clone().unwrap_or_default(),
            "Candidate4".to_string(),
        )),
        _ => None,
    }
}
