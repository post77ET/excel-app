// ZH→JA 方向プロファイル（Phase 4A で新設）。
// 正式な方向IDは "zh2ja"。resolve は別表記 zh_ja / zhja も受理するが、
// id()・ログ上は必ず "zh2ja" に正規化される。

use crate::adapters::types::Lang;
use crate::core1::text_structure_analyzer::analyze_text_structure;
use crate::direction::DirectionProfile;

pub struct ZhJaProfile;

impl DirectionProfile for ZhJaProfile {
    fn id(&self) -> &'static str {
        "zh2ja"
    }

    fn lang_pair(&self) -> (Lang, Lang) {
        (Lang::Zh, Lang::Ja)
    }

    fn should_translate_by_text(&self, text: &str) -> bool {
        // zh2ja 初期基準：CJK漢字を1文字以上含むセルを翻訳対象とする。
        // これは厳密な中国語判定ではなく、Excelセル翻訳用の実用的な対象抽出基準である。
        // （kanji_count は CJK 漢字検出であり、日本語漢字・固有名詞・型式名なども拾い得る）
        //
        // 将来、簡体字・繁体字・日本語漢字の区別が必要になった場合は、
        // 中国語向け文字分類器を別途導入する。
        let s = analyze_text_structure(text);
        s.kanji_count >= 1
    }
}
