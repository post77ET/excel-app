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
            "UserSelect=1 を指定しましたが Candidate1 が未設定のため、原文を維持しました（Apply時フォールバック）".to_string(),
        ),
        2 if row.candidate2.is_none() => Some(
            "UserSelect=2 を指定しましたが Candidate2 が未設定のため、原文を維持しました（Apply時フォールバック）".to_string(),
        ),
        3 if row.candidate3.is_none() => Some(
            "UserSelect=3 を指定しましたが Candidate3 が未設定のため、原文を維持しました（Apply時フォールバック）".to_string(),
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
    match select_code {
        0 => (row.original_writeback.clone(), "Original".to_string()),
        1 => (
            row.candidate1
                .clone()
                .unwrap_or_else(|| row.original_writeback.clone()),
            "Candidate1".to_string(),
        ),
        2 => (
            row.candidate2
                .clone()
                .unwrap_or_else(|| row.original_writeback.clone()),
            "Candidate2".to_string(),
        ),
        3 => (
            row.candidate3
                .clone()
                .unwrap_or_else(|| row.original_writeback.clone()),
            "Candidate3".to_string(),
        ),
        4 => (
            row.candidate4.clone().unwrap_or_default(),
            "Candidate4".to_string(),
        ),
        _ => (row.original_writeback.clone(), "Original".to_string()),
    }
}
