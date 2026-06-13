#[derive(Debug, Clone, Default)]
pub struct TextStructure {
    pub kana_kana_count: usize,
    pub kanji_count: usize,
    pub contains_japanese_like: bool,
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
    }
    out
}

pub fn is_japanese_like(ch: char) -> bool {
    is_hiragana(ch) || is_katakana(ch) || is_cjk(ch)
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
