// VI→JA 方向プロファイル（日本語→ベトナム語追加タスクで新設）。
// 正式な方向IDは "vi2ja"。

use crate::adapters::types::Lang;
use crate::core1::text_structure_analyzer::analyze_text_structure;
use crate::direction::DirectionProfile;

pub struct ViJaProfile;

impl DirectionProfile for ViJaProfile {
    fn id(&self) -> &'static str {
        "vi2ja"
    }

    fn lang_pair(&self) -> (Lang, Lang) {
        (Lang::Vi, Lang::Ja)
    }

    fn should_translate_by_text(&self, text: &str) -> bool {
        // ベトナム語はラテン文字＋声調記号。「ラテン文字を含む」だけだと英語や型番と
        // 衝突するため、ベトナム語特有の声調記号付き文字（viet_diacritic_count）で判定する。
        // 過度に厳密ではないが、英数字だけのセル（型番等）を誤って翻訳対象にしない。
        let s = analyze_text_structure(text);
        s.viet_diacritic_count >= 1
    }
}
