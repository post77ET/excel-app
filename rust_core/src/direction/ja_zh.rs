// JA→ZH 方向プロファイル。
// Phase 1 では現行ハードコード (Lang::Ja, Lang::Zh) と完全に同一。

use crate::adapters::types::Lang;
use crate::core1::text_structure_analyzer::analyze_text_structure;
use crate::direction::DirectionProfile;

pub struct JaZhProfile;

impl DirectionProfile for JaZhProfile {
    fn id(&self) -> &'static str {
        "ja2zh"
    }

    fn lang_pair(&self) -> (Lang, Lang) {
        (Lang::Ja, Lang::Zh)
    }

    fn should_translate_by_text(&self, text: &str) -> bool {
        // Phase 3.6: 計測を内部化。判定は現行と完全一致（漢字1以上 or かな3以上）。
        let s = analyze_text_structure(text);
        s.kanji_count >= 1 || s.kana_kana_count >= 3
    }
}
