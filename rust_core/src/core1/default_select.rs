use crate::core1::text_structure_analyzer::TextStructure;
use crate::core1::types::DefaultSelect;
use crate::core2::structure_types::LogicalCellKind;
use crate::direction::DirectionProfile;

/// DefaultSelect を決定する。
///
/// ルール（優先順）:
///   1. translate_candidates=false なら無条件で Original
///   2. 数式系セル（FormulaRaw / SharedFormulaParent / SharedFormulaFollower）は Original
///      ※ generate_entry_pipeline.rs でも Formula 系を弾いているが、
///        ここでも二重ガードとして維持する
///   3. candidate1 と original が一致する場合は Original（翻訳価値なし）
///   4. 翻訳価値がある日本語テキスト（漢字 or かなかな 3文字以上）なら Candidate1
///   5. それ以外は Original
pub fn decide_default_select(
    cell_kind: LogicalCellKind,
    structure: &TextStructure,
    translate_candidates: bool,
    candidate1_text: Option<&str>,
    original_text: &str,
    direction: &dyn DirectionProfile,
) -> DefaultSelect {
    // ルール1: 翻訳自体をスキップするセル
    if !translate_candidates {
        return DefaultSelect::Original;
    }

    // ルール2: 数式セルは原本維持
    if matches!(
        cell_kind,
        LogicalCellKind::FormulaRaw
            | LogicalCellKind::SharedFormulaParent
            | LogicalCellKind::SharedFormulaFollower
    ) {
        return DefaultSelect::Original;
    }

    // ルール3: candidate1 が original と同じなら翻訳価値なし
    if let Some(c1) = candidate1_text {
        let c1_trimmed = c1.trim();
        let orig_trimmed = original_text.trim();
        if !c1_trimmed.is_empty() && c1_trimmed == orig_trimmed {
            return DefaultSelect::Original;
        }
    }

    // ルール4: 翻訳価値がある日本語テキスト
    if direction.should_translate_by_text_structure(structure) {
        return DefaultSelect::Candidate1;
    }

    // ルール5: その他は原本維持
    DefaultSelect::Original
}
