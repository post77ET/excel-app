use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ja,
    Zh,
    En,
    De,
    Vi,
    Auto,
}

impl Lang {
    pub fn to_deepl_code(self) -> Option<&'static str> {
        match self {
            Lang::Ja => Some("JA"),
            Lang::Zh => Some("ZH"),
            Lang::En => Some("EN"),
            Lang::De => Some("DE"),
            // DeepL は 2025-06 の言語拡張でベトナム語（VI）に対応済み（確認済み）。
            Lang::Vi => Some("VI"),
            Lang::Auto => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Google,
    Amazon,
    DeepL,
    Mock,
}

impl ProviderKind {
    pub fn as_label(&self) -> &'static str {
        match self {
            ProviderKind::DeepL => "DEEPL",
            ProviderKind::Google => "GOOGLE",
            ProviderKind::Amazon => "AMAZON",
            ProviderKind::Mock => "MOCK",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranslateRequest {
    pub request_id: String,
    pub provider: ProviderKind,
    pub text: String,
    pub from_lang: Lang,
    pub to_lang: Lang,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub struct TranslateResponse {
    pub request_id: String,
    pub provider: ProviderKind,
    pub translated_text: String,
    pub detected_source_lang: Option<Lang>,
    pub raw_meta: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterErrorKind {
    Timeout,
    Empty,
    Auth,
    RateLimit,
    Server,
    Network,
    InvalidConfig,
    Parse,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct AdapterError {
    pub provider: ProviderKind,
    pub error_kind: AdapterErrorKind,
    pub message: String,
}

impl AdapterError {
    pub fn new(provider: ProviderKind, error_kind: AdapterErrorKind, message: impl Into<String>) -> Self {
        Self {
            provider,
            error_kind,
            message: message.into(),
        }
    }
}
