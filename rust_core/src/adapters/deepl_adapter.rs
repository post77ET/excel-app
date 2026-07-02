use reqwest::blocking::Client;
use std::time::Duration;
use reqwest::header::AUTHORIZATION;
use serde::Deserialize;
use serde_json::json;

use crate::adapters::translator_trait::TranslatorAdapter;
use crate::adapters::types::{
    AdapterError,
    AdapterErrorKind,
    Lang,
    ProviderKind,
    TranslateRequest,
    TranslateResponse,
};

#[derive(Debug, Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Debug, Deserialize)]
struct DeepLTranslation {
    text: String,
    #[serde(default)]
    detected_source_language: Option<String>,
}

const DEEPL_HTTP_TIMEOUT_SEC: u64 = 30;
const DEEPL_HTTP_CONNECT_TIMEOUT_SEC: u64 = 10;

pub struct DeepLAdapter {
    client: Client,
    api_key: String,
}

impl DeepLAdapter {
    pub fn new(api_key: String) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(DEEPL_HTTP_CONNECT_TIMEOUT_SEC))
            .timeout(Duration::from_secs(DEEPL_HTTP_TIMEOUT_SEC))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client, api_key }
    }

    fn map_detected_lang(lang_code: Option<&str>) -> Option<Lang> {
        match lang_code.unwrap_or("").to_uppercase().as_str() {
            "JA" => Some(Lang::Ja),
            "ZH" => Some(Lang::Zh),
            "EN" => Some(Lang::En),
            "DE" => Some(Lang::De),
            "VI" => Some(Lang::Vi),
            _ => None,
        }
    }

    fn map_http_error(status_code: u16, body: &str) -> AdapterErrorKind {
        match status_code {
            400 | 404 => AdapterErrorKind::InvalidConfig,
            401 | 403 => AdapterErrorKind::Auth,
            408 => AdapterErrorKind::Timeout,
            429 => AdapterErrorKind::RateLimit,
            456 => AdapterErrorKind::RateLimit,
            500..=599 => AdapterErrorKind::Server,
            _ => {
                let body_upper = body.to_uppercase();
                if body_upper.contains("AUTH") || body_upper.contains("KEY") {
                    AdapterErrorKind::Auth
                } else if body_upper.contains("RATE") || body_upper.contains("TOO MANY") || body_upper.contains("QUOTA") {
                    AdapterErrorKind::RateLimit
                } else {
                    AdapterErrorKind::Unknown
                }
            }
        }
    }


    fn endpoint_for_key(api_key: &str) -> &'static str {
        if api_key.trim().ends_with(":fx") {
            "https://api-free.deepl.com/v2/translate"
        } else {
            "https://api.deepl.com/v2/translate"
        }
    }

    fn endpoint_label(api_key: &str) -> &'static str {
        if api_key.trim().ends_with(":fx") {
            "api-free"
        } else {
            "api-pro"
        }
    }

    fn classify_reqwest_error(msg: &str) -> AdapterErrorKind {
        let lower = msg.to_lowercase();
        if lower.contains("timed out") || lower.contains("timeout") {
            AdapterErrorKind::Timeout
        } else if lower.contains("handshake") || lower.contains("tls") || lower.contains("certificate") || lower.contains("unexpected eof") {
            AdapterErrorKind::Network
        } else if lower.contains("dns") || lower.contains("resolve") || lower.contains("connect") || lower.contains("connection") {
            AdapterErrorKind::Network
        } else {
            AdapterErrorKind::Network
        }
    }

    fn summarize_reqwest_error(msg: &str) -> &'static str {
        let lower = msg.to_lowercase();
        if lower.contains("unexpected eof") && lower.contains("handshake") {
            "TLS_HANDSHAKE_UNEXPECTED_EOF"
        } else if lower.contains("handshake") {
            "TLS_HANDSHAKE_FAILED"
        } else if lower.contains("timed out") || lower.contains("timeout") {
            "TIMEOUT"
        } else if lower.contains("dns") || lower.contains("resolve") {
            "DNS_OR_RESOLVE_FAILED"
        } else if lower.contains("connect") || lower.contains("connection") {
            "CONNECT_FAILED"
        } else {
            "REQUEST_SEND_FAILED"
        }
    }

}

impl TranslatorAdapter for DeepLAdapter {
    fn provider_kind(&self) -> ProviderKind {
        ProviderKind::DeepL
    }

    fn translate_once(
        &self,
        request: &TranslateRequest,
    ) -> Result<TranslateResponse, AdapterError> {
        let results = self.translate_batch(&[request.clone()])?;
        results.into_iter().next().ok_or_else(|| {
            AdapterError::new(
                ProviderKind::DeepL,
                AdapterErrorKind::Empty,
                "DeepL returned empty result for single request",
            )
        })
    }

    fn translate_batch(
        &self,
        requests: &[TranslateRequest],
    ) -> Result<Vec<TranslateResponse>, AdapterError> {
        if self.api_key.trim().is_empty() {
            return Err(AdapterError::new(
                ProviderKind::DeepL,
                AdapterErrorKind::InvalidConfig,
                "DEEPL api key is empty",
            ));
        }

        if requests.is_empty() {
            return Ok(vec![]);
        }

        let first_target = requests[0].to_lang;
        let first_source = requests[0].from_lang;

        for req in requests.iter().skip(1) {
            if req.to_lang != first_target || req.from_lang != first_source {
                return Err(AdapterError::new(
                    ProviderKind::DeepL,
                    AdapterErrorKind::InvalidConfig,
                    "DeepL batch requires same source/target lang within one batch",
                ));
            }
        }

        let target_lang = first_target.to_deepl_code().ok_or_else(|| {
            AdapterError::new(
                ProviderKind::DeepL,
                AdapterErrorKind::InvalidConfig,
                "DeepL target language is invalid or Auto",
            )
        })?;

        let source_lang = first_source.to_deepl_code();
        let texts: Vec<String> = requests.iter().map(|r| r.text.clone()).collect();
        let endpoint = Self::endpoint_for_key(&self.api_key);
        let endpoint_label = Self::endpoint_label(&self.api_key);
        let auth_header_value = format!("DeepL-Auth-Key {}", self.api_key.trim());

        println!(
            "[TRANSLATE][DEEPL] batch start count={} endpoint={} key_present={} key_len={} timeout_sec={}",
            requests.len(),
            endpoint_label,
            !self.api_key.trim().is_empty(),
            self.api_key.trim().len(),
            DEEPL_HTTP_TIMEOUT_SEC
        );
        println!("[TRANSLATE][DEEPL] source={:?} target={}", source_lang, target_lang);

        let mut body = json!({
            "text": texts,
            "target_lang": target_lang
        });

        if let Some(src) = source_lang {
            body["source_lang"] = json!(src);
        }

        let response = self
            .client
            .post(endpoint)
            .header(AUTHORIZATION, auth_header_value)
            .json(&body)
            .send()
            .map_err(|e| {
                let msg = e.to_string();
                let summary = Self::summarize_reqwest_error(&msg);
                let kind = Self::classify_reqwest_error(&msg);
                println!(
                    "[TRANSLATE][DEEPL][ERROR_CLASS] {} kind={:?} message={}",
                    summary,
                    kind,
                    msg
                );
                AdapterError::new(
                    ProviderKind::DeepL,
                    kind,
                    format!("deepl request failed: class={} message={}", summary, msg),
                )
            })?;

        let status = response.status();
        let body_text = response.text().map_err(|e| {
            AdapterError::new(
                ProviderKind::DeepL,
                AdapterErrorKind::Parse,
                format!("deepl body read failed: {}", e),
            )
        })?;

        println!("[TRANSLATE][DEEPL] response status={} body_len={}", status.as_u16(), body_text.len());

        if !status.is_success() {
            println!("[TRANSLATE][DEEPL][ERROR] http status={} body={}", status.as_u16(), body_text);
            return Err(AdapterError::new(
                ProviderKind::DeepL,
                Self::map_http_error(status.as_u16(), &body_text),
                format!("deepl http error: status={} body={}", status.as_u16(), body_text),
            ));
        }

        let parsed: DeepLResponse = serde_json::from_str(&body_text).map_err(|e| {
            AdapterError::new(
                ProviderKind::DeepL,
                AdapterErrorKind::Parse,
                format!("deepl json parse failed: {}", e),
            )
        })?;

        if parsed.translations.is_empty() {
            return Err(AdapterError::new(
                ProviderKind::DeepL,
                AdapterErrorKind::Empty,
                "DeepL returned no translations",
            ));
        }

        if parsed.translations.len() != requests.len() {
            return Err(AdapterError::new(
                ProviderKind::DeepL,
                AdapterErrorKind::Parse,
                format!(
                    "DeepL response count mismatch: requests={} responses={}",
                    requests.len(),
                    parsed.translations.len()
                ),
            ));
        }

        let mut results = Vec::with_capacity(parsed.translations.len());
        for (req, trans) in requests.iter().zip(parsed.translations.into_iter()) {
            results.push(TranslateResponse {
                request_id: req.request_id.clone(),
                provider: ProviderKind::DeepL,
                translated_text: trans.text,
                detected_source_lang: Self::map_detected_lang(
                    trans.detected_source_language.as_deref(),
                ),
                raw_meta: None,
            });
        }

        println!("[TRANSLATE][DEEPL] batch ok count={}", results.len());
        Ok(results)
    }
}
