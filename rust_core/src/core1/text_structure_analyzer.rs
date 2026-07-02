#[derive(Debug, Clone, Default)]
pub struct TextStructure {
    pub kana_kana_count: usize,
    pub kanji_count: usize,
    pub contains_japanese_like: bool,
    /// ベトナム語声調記号付き文字（および ăâêôơư đ 等の固有字）の出現数。
    /// vi2ja の should_translate_by_text 判定用（Phase: ja2vi/vi2ja 追加）。
    pub viet_diacritic_count: usize,
}

pub fn analyze_text_structure(text: &str) -> TextStructure {
    let mut out = TextStructure::default();
    for ch in text.chars() {
        if is_hiragana(ch) || is_katakana(ch) {
            out.kana_kana_count += 1;
            out.contains_japanese_like = true;
        } else if is_cjk(ch) {
            out.kanji_count += 1;
            out.contains_japanese_like = true;
        }
        if is_vietnamese_diacritic(ch) {
            out.viet_diacritic_count += 1;
        }
    }
    out
}

pub fn is_japanese_like(ch: char) -> bool {
    is_hiragana(ch) || is_katakana(ch) || is_cjk(ch)
}

/// ベトナム語に特有の文字を検出する。
/// 「ラテン文字を含む」だけで判定すると英語・型番と衝突するため、
/// ベトナム語固有の基底字（đ ă â ê ô ơ ư とその大文字）と、
/// 声調記号を伴う合成済み母音（Unicode Latin Extended Additional: U+1EA0-U+1EF9）
/// のみを対象とする。過度に厳密ではないが、英数字だけのセルは拾わない設計。
pub fn is_vietnamese_diacritic(ch: char) -> bool {
    matches!(
        ch,
        'đ' | 'Đ' | 'ă' | 'Ă' | 'â' | 'Â' | 'ê' | 'Ê' | 'ô' | 'Ô' | 'ơ' | 'Ơ' | 'ư' | 'Ư'
    ) || matches!(ch as u32, 0x1EA0..=0x1EF9)
}

fn is_hiragana(ch: char) -> bool {
    matches!(ch as u32, 0x3040..=0x309F)
}

fn is_katakana(ch: char) -> bool {
    matches!(ch as u32, 0x30A0..=0x30FF)
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x4E00..=0x9FFF)
}
