// JA→VI 方向プロファイル（日本語→ベトナム語追加タスクで新設）。
// 正式な方向IDは "ja2vi"。

use crate::adapters::types::Lang;
use crate::core1::text_structure_analyzer::analyze_text_structure;
use crate::direction::DirectionProfile;

pub struct JaViProfile;

impl DirectionProfile for JaViProfile {
    fn id(&self) -> &'static str {
        "ja2vi"
    }

    fn lang_pair(&self) -> (Lang, Lang) {
        (Lang::Ja, Lang::Vi)
    }

    fn should_translate_by_text(&self, text: &str) -> bool {
        // 翻訳元は日本語。判定は ja2zh と同一基準（漢字1以上 or かな3以上）を流用。
        let s = analyze_text_structure(text);
        s.kanji_count >= 1 || s.kana_kana_count >= 3
    }
}
