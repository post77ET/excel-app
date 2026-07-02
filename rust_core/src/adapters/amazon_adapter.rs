use aws_config::{timeout::TimeoutConfig, BehaviorVersion, Region};
use aws_sdk_translate::types::{Formality, TranslationSettings};
use aws_sdk_translate::Client as AwsTranslateClient;
use std::time::Duration;
use tokio::runtime::Runtime;

use crate::adapters::translator_trait::TranslatorAdapter;
use crate::adapters::types::{
    AdapterError,
    AdapterErrorKind,
    Lang,
    ProviderKind,
    TranslateRequest,
    TranslateResponse,
};

pub struct AmazonAdapter;

impl AmazonAdapter {
    pub fn new() -> Self {
        Self
    }

    fn normalize_amazon_lang(lang: Lang) -> Result<&'static str, AdapterError> {
        match lang {
            Lang::Ja => Ok("ja"),
            Lang::Zh => Ok("zh"),
            Lang::En => Ok("en"),
            Lang::De => Ok("de"),
            Lang::Vi => Ok("vi"),
            Lang::Auto => Err(AdapterError::new(
                ProviderKind::Amazon,
                AdapterErrorKind::InvalidConfig,
                "Amazon source/target language cannot be Auto in this adapter",
            )),
        }
    }

    fn map_aws_error_message(msg: &str) -> AdapterErrorKind {
        let upper = msg.to_uppercase();

        if upper.contains("TIMEOUT") {
            AdapterErrorKind::Timeout
        } else if upper.contains("SUBSCRIPTIONREQUIREDEXCEPTION")
            || upper.contains("INVALIDSIGNATUREEXCEPTION")
            || upper.contains("UNRECOGNIZEDCLIENTEXCEPTION")
            || upper.contains("AUTH")
            || upper.contains("ACCESS KEY")
        {
            AdapterErrorKind::Auth
        } else if upper.contains("THROTTL") || upper.contains("RATE") {
            AdapterErrorKind::RateLimit
        } else if upper.contains("NETWORK") || upper.contains("CONNECT") {
            AdapterErrorKind::Network
        } else if upper.contains("5XX")
            || upper.contains("INTERNAL")
            || upper.contains("SERVER")
        {
            AdapterErrorKind::Server
        } else {
            AdapterErrorKind::Unknown
        }
    }

    fn translate_one(request: &TranslateRequest) -> Result<TranslateResponse, AdapterError> {
        let source_lang = Self::normalize_amazon_lang(request.from_lang)?;
        let target_lang = Self::normalize_amazon_lang(request.to_lang)?;

        let rt = Runtime::new().map_err(|e| {
            AdapterError::new(
                ProviderKind::Amazon,
                AdapterErrorKind::InvalidConfig,
                format!("Failed to create tokio runtime: {}", e),
            )
        })?;

        rt.block_on(async move {
            println!("[TRANSLATE][AMAZON] ===== START =====");

            println!(
                "[TRANSLATE][AMAZON] AWS_REGION env={:?}",
                std::env::var("AWS_REGION")
            );

            println!(
                "[TRANSLATE][AMAZON] AWS_DEFAULT_REGION env={:?}",
                std::env::var("AWS_DEFAULT_REGION")
            );

            println!(
                "[TRANSLATE][AMAZON] AWS_ACCESS_KEY_ID={}",
                std::env::var("AWS_ACCESS_KEY_ID")
                    .map(|v| format!("present len={}", v.len()))
                    .unwrap_or("missing".to_string())
            );

            println!(
                "[TRANSLATE][AMAZON] AWS_SECRET_ACCESS_KEY={}",
                std::env::var("AWS_SECRET_ACCESS_KEY")
                    .map(|v| format!("present len={}", v.len()))
                    .unwrap_or("missing".to_string())
            );

            println!(
                "[TRANSLATE][AMAZON] request_id={} chars={}",
                request.request_id,
                request.text.chars().count()
            );

            let timeout_config = TimeoutConfig::builder()
                .connect_timeout(Duration::from_secs(20))
                .operation_attempt_timeout(Duration::from_secs(60))
                .operation_timeout(Duration::from_secs(90))
                .build();

            println!("[TRANSLATE][AMAZON] timeout config created");

            let config = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new("ap-northeast-1"))
                .timeout_config(timeout_config)
                .load()
                .await;

            println!(
                "[TRANSLATE][AMAZON] config loaded region={:?}",
                config.region()
            );

            let client = AwsTranslateClient::new(&config);

            println!("[TRANSLATE][AMAZON] client created");

            println!(
                "[TRANSLATE][AMAZON] request start request_id={} chars={}",
                request.request_id,
                request.text.chars().count()
            );

            let result = client
                .translate_text()
                .text(request.text.clone())
                .source_language_code(source_lang)
                .target_language_code(target_lang)
                .settings(
                    TranslationSettings::builder()
                        .formality(Formality::Formal)
                        .build(),
                )
                .send()
                .await;

            match result {
                Ok(response) => {
                    println!("[TRANSLATE][AMAZON] SUCCESS");

                    Ok(TranslateResponse {
                        request_id: request.request_id.clone(),
                        provider: ProviderKind::Amazon,
                        translated_text: response.translated_text().to_string(),
                        detected_source_lang: Some(request.from_lang),
                        raw_meta: None,
                    })
                }

                Err(e) => {
                    println!(
                        "[TRANSLATE][AMAZON][ERROR][DEBUG]={:#?}",
                        e
                    );

                    println!(
                        "[TRANSLATE][AMAZON][ERROR][DISPLAY]={}",
                        e
                    );

                    let msg = format!("{:?}", e);

                    Err(AdapterError::new(
                        ProviderKind::Amazon,
                        Self::map_aws_error_message(&msg),
                        format!("amazon translate failed: {}", msg),
                    ))
                }
            }
        })
    }
}

impl Default for AmazonAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TranslatorAdapter for AmazonAdapter {
    fn provider_kind(&self) -> ProviderKind {
        ProviderKind::Amazon
    }

    fn translate_once(
        &self,
        request: &TranslateRequest,
    ) -> Result<TranslateResponse, AdapterError> {
        Self::translate_one(request)
    }

    fn translate_batch(
        &self,
        requests: &[TranslateRequest],
    ) -> Result<Vec<TranslateResponse>, AdapterError> {
        let mut out = Vec::with_capacity(requests.len());

        for request in requests {
            out.push(Self::translate_one(request)?);
        }

        Ok(out)
    }
}