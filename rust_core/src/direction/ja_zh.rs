// JA→ZH 方向プロファイル。
// Phase 1 では現行ハードコード (Lang::Ja, Lang::Zh) と完全に同一。

use crate::adapters::types::Lang;
use crate::core1::text_structure_analyzer::TextStructure;
use crate::direction::DirectionProfile;

pub struct JaZhProfile;

impl DirectionProfile for JaZhProfile {
    fn id(&self) -> &'static str {
        "ja2zh"
    }

    fn lang_pair(&self) -> (Lang, Lang) {
        (Lang::Ja, Lang::Zh)
    }

    fn should_translate_by_text_structure(&self, s: &TextStructure) -> bool {
        // 現行挙動と完全一致（漢字1以上 or かな3以上）
        s.kanji_count >= 1 || s.kana_kana_count >= 3
    }
}
