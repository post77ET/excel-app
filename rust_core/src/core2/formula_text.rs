use crate::core1::types::Segment;
use crate::core1::text_structure_analyzer::is_japanese_like;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CharClass {
    Japanese,
    Connector,
    Other,
}

pub fn split_preserving_structure(text: &str) -> Vec<Segment> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Segment> = Vec::new();
    let mut buf = String::new();
    let mut cur_target: Option<bool> = None;

    for i in 0..chars.len() {
        let ch = chars[i];
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let target = belongs_to_japanese_segment(ch, prev);

        match cur_target {
            None => {
                cur_target = Some(target);
                buf.push(ch);
            }
            Some(ct) if ct == target => {
                buf.push(ch);
            }
            Some(ct) => {
                out.push(Segment {
                    text: std::mem::take(&mut buf),
                    target: ct,
                });
                buf.push(ch);
                cur_target = Some(target);
            }
        }
    }

    if let Some(ct) = cur_target {
        if !buf.is_empty() {
            out.push(Segment {
                text: buf,
                target: ct,
            });
        }
    }

    out
}

pub fn contains_japanese_like(text: &str) -> bool {
    text.chars().any(is_japanese_like)
}

fn belongs_to_japanese_segment(ch: char, prev: Option<char>) -> bool {
    match classify_char(ch) {
        CharClass::Japanese => true,
        CharClass::Connector => prev.map(is_japanese_like).unwrap_or(false),
        CharClass::Other => false,
    }
}

fn classify_char(ch: char) -> CharClass {
    if is_japanese_like(ch) {
        CharClass::Japanese
    } else if is_connector(ch) {
        CharClass::Connector
    } else {
        CharClass::Other
    }
}

fn is_connector(ch: char) -> bool {
    matches!(ch, '、' | '。' | '？' | '！')
}
// =============================================================================
// F-2: 数式セルの構文保護（whole経路用）
// 数式の構文（=, 関数名, セル参照, 括弧, 演算子, 区切り）は翻訳機へ渡さず、
// 二重引用符内の文字列リテラルのみを翻訳対象とする。
// =============================================================================

const LIT_OPEN: char = '\u{E020}';
const LIT_CLOSE: char = '\u{E021}';

/// 数式セルかどうか（先頭が '='）。
pub fn is_formula_text(text: &str) -> bool {
    text.trim_start().starts_with('=')
}

/// 数式から "..." リテラルを抽出し、プレースホルダ化したテンプレートとリテラル列を返す。
/// Excel数式の "" エスケープはリテラル内の " として保持する。
pub fn extract_quoted_literals(formula: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = formula.chars().collect();
    let mut template = String::new();
    let mut literals: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            let mut lit = String::new();
            i += 1;
            loop {
                if i >= chars.len() {
                    break;
                }
                if chars[i] == '"' {
                    if i + 1 < chars.len() && chars[i + 1] == '"' {
                        lit.push('"');
                        i += 2;
                        continue;
                    }
                    i += 1; // 閉じ引用符
                    break;
                }
                lit.push(chars[i]);
                i += 1;
            }
            template.push(LIT_OPEN);
            template.push_str(&literals.len().to_string());
            template.push(LIT_CLOSE);
            literals.push(lit);
        } else {
            template.push(ch);
            i += 1;
        }
    }
    (template, literals)
}

/// extract_quoted_literals のテンプレートに翻訳済みリテラルを戻す。
/// リテラル内の " は Excel数式の "" に再エスケープする。
pub fn reassemble_formula(template: &str, translated: &[String]) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == LIT_OPEN {
            let mut num = String::new();
            i += 1;
            while i < chars.len() && chars[i] != LIT_CLOSE {
                num.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1; // LIT_CLOSE をスキップ
            }
            out.push('"');
            if let Ok(idx) = num.parse::<usize>() {
                if let Some(lit) = translated.get(idx) {
                    for c in lit.chars() {
                        if c == '"' {
                            out.push('"');
                            out.push('"');
                        } else {
                            out.push(c);
                        }
                    }
                }
            }
            out.push('"');
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}
