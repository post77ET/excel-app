// ============================================================
// PROTECTED TEXT DETECTION LAYER
//
// IMPORTANT:
//
// This module is NOT a special-case patch and must not be
// removed during future "if cleanup" or split/batch refactoring.
//
// URL / mail / path / hyperlink-like strings are not linguistic
// translation targets. They are transport tokens, identifiers, or
// locators. If they are sent to CORE1 segmentation or external
// providers, Japanese text inside the token can be translated and
// the destination can be corrupted.
//
// Examples of corruption this layer prevents:
//
// - http://example.com/東京/沖縄
//   -> http://example.com/东京/冲绳
//
// - test+東京@example.com
//   -> test+东京@example.com
//
// - C:\案件\東京\file.xlsx
//   -> C:\案件\东京\file.xlsx
//
// This detection must happen BEFORE translation segmentation,
// batching, provider dispatch, and rejoin.
//
// DO NOT move this logic into:
//
// - batch layer
// - provider layer
// - rejoin layer
//
// Regression history:
//
// Previous versions preserved URL-like strings, but the protection
// was accidentally removed during split/batch standardization.
// Keep this as a named structural protection layer so it does not
// look like disposable, case-by-case IF logic.
// ============================================================

pub fn is_protected_transport_text(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }

    contains_hyperlink_formula(trimmed)
        || contains_url_scheme(trimmed)
        || starts_with_www(trimmed)
        || looks_like_email_address(trimmed)
        || looks_like_windows_path(trimmed)
        || looks_like_unc_path(trimmed)
        || looks_like_excel_internal_link(trimmed)
}

fn contains_hyperlink_formula(text: &str) -> bool {
    text.to_ascii_uppercase().contains("HYPERLINK(")
}

fn contains_url_scheme(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("http://")
        || lower.contains("https://")
        || lower.contains("ftp://")
        || lower.contains("file://")
        || lower.contains("mailto:")
}

fn starts_with_www(text: &str) -> bool {
    text.to_ascii_lowercase().starts_with("www.")
}

fn looks_like_email_address(text: &str) -> bool {
    let s = text.trim();

    if s.chars().any(char::is_whitespace) {
        return false;
    }

    let Some(at_pos) = s.find('@') else {
        return false;
    };

    if at_pos == 0 || at_pos + 1 >= s.len() {
        return false;
    }

    let local = &s[..at_pos];
    let domain = &s[at_pos + 1..];

    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
}

fn looks_like_windows_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 3 {
        return false;
    }

    bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

fn looks_like_unc_path(text: &str) -> bool {
    text.starts_with("\\\\")
}

fn looks_like_excel_internal_link(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('#') && trimmed.contains('!')
}

#[cfg(test)]
mod tests {
    use super::is_protected_transport_text;

    #[test]
    fn detects_transport_tokens() {
        assert!(is_protected_transport_text("http://example.com/東京/沖縄"));
        assert!(is_protected_transport_text("https://example.com"));
        assert!(is_protected_transport_text("ftp://server/path"));
        assert!(is_protected_transport_text("file://server/share"));
        assert!(is_protected_transport_text("mailto:test@example.com"));
        assert!(is_protected_transport_text("www.example.com/東京"));
        assert!(is_protected_transport_text("test+東京@example.com"));
        assert!(is_protected_transport_text("C:\\案件\\東京\\file.xlsx"));
        assert!(is_protected_transport_text("\\\\server\\share\\東京"));
        assert!(is_protected_transport_text("#Sheet1!A1"));
        assert!(is_protected_transport_text("=HYPERLINK(\"http://example.com/東京\",\"東京\")"));
    }

    #[test]
    fn does_not_detect_normal_text() {
        assert!(!is_protected_transport_text("東京設備"));
        assert!(!is_protected_transport_text("ABC東京DEF"));
        assert!(!is_protected_transport_text("=IF(A1>0,\"東京\",\"大阪\")"));
    }
}
