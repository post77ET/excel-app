use crate::adapters::types::ProviderKind;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct TranslatorConfig {
    pub candidate1_provider: ProviderKind,
    pub candidate2_provider: ProviderKind,
    pub candidate3_provider: ProviderKind,
    pub batch_max_items: usize,
    pub batch_max_chars: usize,
    pub deepl_api_key: Option<String>,
    pub deepl_use_free_endpoint: Option<bool>,
}

impl Default for TranslatorConfig {
    fn default() -> Self {
        Self {
            candidate1_provider: ProviderKind::Google,
            candidate2_provider: ProviderKind::Amazon,
            candidate3_provider: ProviderKind::DeepL,
            batch_max_items: 50,
            batch_max_chars: 3000,
            deepl_api_key: None,
            deepl_use_free_endpoint: Some(true),
        }
    }
}

pub fn load_translator_config() -> TranslatorConfig {
    let path = std::env::var("ETB_TRANSLATOR_CONFIG")
        .unwrap_or_else(|_| "config/translator_config.json".to_string());

    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|_| TranslatorConfig::default()),
        Err(_) => TranslatorConfig::default(),
    }
}
