use crate::adapters::amazon_adapter::AmazonAdapter;
use crate::adapters::deepl_adapter::DeepLAdapter;
use crate::adapters::google_adapter::GoogleAdapter;
use crate::adapters::mock_adapter::MockTranslator;
use std::sync::Arc;
use crate::adapters::translator_trait::TranslatorAdapter;
use crate::adapters::types::ProviderKind;
use crate::infra::config_loader::TranslatorConfig;

pub fn create_adapter(
    provider: ProviderKind,
    config: &TranslatorConfig,
) -> Arc<dyn TranslatorAdapter> {
    match provider {
        ProviderKind::DeepL => {
            let api_key = std::env::var("DEEPL_API_KEY")
                .ok()
                .or_else(|| config.deepl_api_key.clone())
                .unwrap_or_default();
            Arc::new(DeepLAdapter::new(api_key))
        }
        ProviderKind::Google => Arc::new(GoogleAdapter::new()),
        ProviderKind::Amazon => Arc::new(AmazonAdapter::new()),
        ProviderKind::Mock => Arc::new(MockTranslator::default()),
    }
}