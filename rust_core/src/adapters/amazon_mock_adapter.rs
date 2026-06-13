use crate::adapters::translator_trait::TranslatorAdapter;
use crate::adapters::types::{AdapterError, ProviderKind, TranslateRequest, TranslateResponse};

#[derive(Default)]
pub struct AmazonMockTranslator;

impl TranslatorAdapter for AmazonMockTranslator {
    fn provider_kind(&self) -> ProviderKind { ProviderKind::Amazon }

    fn translate_once(&self, request: &TranslateRequest) -> Result<TranslateResponse, AdapterError> {
        let translated_text = match request.text.as_str() {
            "確認" => "确认_A".to_string(),
            "高速" => "高速_A".to_string(),
            other => format!("A<{}>", other),
        };

        Ok(TranslateResponse {
            request_id: request.request_id.clone(),
            provider: ProviderKind::Amazon,
            translated_text,
            detected_source_lang: None,
            raw_meta: None,
        })
    }
}
