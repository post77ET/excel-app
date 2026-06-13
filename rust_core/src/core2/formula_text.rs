use crate::core1::types::Segment;

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

fn is_japanese_like(ch: char) -> bool {
    matches!(ch as u32, 0x3040..=0x309F | 0x30A0..=0x30FF | 0x4E00..=0x9FFF)
}