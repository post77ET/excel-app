use crate::adapters::translator_trait::TranslatorAdapter;
use crate::adapters::types::{AdapterError, ProviderKind, TranslateRequest, TranslateResponse};

#[derive(Default)]
pub struct Mock2Translator;

impl TranslatorAdapter for Mock2Translator {
    fn provider_kind(&self) -> ProviderKind { ProviderKind::Mock }

    fn translate_once(&self, request: &TranslateRequest) -> Result<TranslateResponse, AdapterError> {
        let translated_text = match request.text.as_str() {
            "確認" => "确认_G".to_string(),
            "高速" => "高速_G".to_string(),
            other => format!("G<{}>", other),
        };

        Ok(TranslateResponse {
            request_id: request.request_id.clone(),
            provider: ProviderKind::Mock,
            translated_text,
            detected_source_lang: None,
            raw_meta: None,
        })
    }
}
