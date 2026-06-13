use crate::adapters::types::{AdapterError, ProviderKind, TranslateRequest, TranslateResponse};

pub trait TranslatorAdapter: Send + Sync {
    fn provider_kind(&self) -> ProviderKind;

    fn translate_once(
        &self,
        request: &TranslateRequest,
    ) -> Result<TranslateResponse, AdapterError>;

    fn translate_batch(
        &self,
        requests: &[TranslateRequest],
    ) -> Result<Vec<TranslateResponse>, AdapterError> {
        let mut out = Vec::with_capacity(requests.len());
        for request in requests {
            out.push(self.translate_once(request)?);
        }
        Ok(out)
    }
}
