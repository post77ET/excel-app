use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

use crate::adapters::translator_trait::TranslatorAdapter;
use crate::adapters::types::{
    AdapterError,
    AdapterErrorKind,
    Lang,
    ProviderKind,
    TranslateRequest,
    TranslateResponse,
};

const GOOGLE_TRANSLATE_V2_URL: &str =
    "https://translation.googleapis.com/language/translate/v2";

const GOOGLE_HTTP_TIMEOUT_SEC: u64 = 15;
const GOOGLE_HTTP_CONNECT_TIMEOUT_SEC: u64 = 10;

#[derive(Debug, Serialize)]
struct GoogleV2TranslateRequestBody {
    q: Vec<String>,
    source: String,
    target: String,
    format: String,
}

#[derive(Debug, Deserialize)]
struct GoogleV2TranslateResponseBody {
    data: GoogleV2TranslateData,
}

#[derive(Debug, Deserialize)]
struct GoogleV2TranslateData {
    translations: Vec<GoogleV2TranslatedText>,
}

#[derive(Debug, Deserialize)]
struct GoogleV2TranslatedText {
    #[serde(rename = "translatedText")]
    translated_text: String,

    #[serde(rename = "detectedSourceLanguage")]
    detected_source_language: Option<String>,
}

pub struct GoogleAdapter;

impl GoogleAdapter {
    pub fn new() -> Self {
        Self
    }

    fn load_api_key() -> Result<String, AdapterError> {
        let key = env::var("GOOGLE_TRANSLATE_API_KEY")
            .or_else(|_| env::var("GOOGLE_API_KEY"))
            .map_err(|_| {
                AdapterError::new(
                    ProviderKind::Google,
                    AdapterErrorKind::InvalidConfig,
                    "GOOGLE_TRANSLATE_API_KEY missing",
                )
            })?;

        let trimmed = key.trim().to_string();

        if trimmed.is_empty() {
            return Err(AdapterError::new(
                ProviderKind::Google,
                AdapterErrorKind::InvalidConfig,
                "GOOGLE_TRANSLATE_API_KEY is empty",
            ));
        }

        Ok(trimmed)
    }

    fn map_detected_lang(lang_code: Option<&str>) -> Option<Lang> {
        match lang_code.unwrap_or("").to_uppercase().as_str() {
            "JA" => Some(Lang::Ja),
            "ZH" | "ZH-CN" | "ZH-HANS" => Some(Lang::Zh),
            "EN" => Some(Lang::En),
            "DE" => Some(Lang::De),
            _ => None,
        }
    }

    fn map_http_error(status_code: u16, body: &str) -> AdapterErrorKind {
        match status_code {
            400 | 404 => AdapterErrorKind::InvalidConfig,
            401 | 403 => AdapterErrorKind::Auth,
            408 => AdapterErrorKind::Timeout,
            429 => AdapterErrorKind::RateLimit,
            500..=599 => AdapterErrorKind::Server,

            _ => {
                let body_upper = body.to_uppercase();

                if body_upper.contains("AUTH")
                    || body_upper.contains("API KEY")
                    || body_upper.contains("API_KEY")
                    || body_upper.contains("CREDENTIAL")
                    || body_upper.contains("PERMISSION")
                {
                    AdapterErrorKind::Auth
                } else if body_upper.contains("RATE")
                    || body_upper.contains("TOO MANY")
                {
                    AdapterErrorKind::RateLimit
                } else if body_upper.contains("TIMEOUT")
                    || body_upper.contains("TIMED OUT")
                {
                    AdapterErrorKind::Timeout
                } else {
                    AdapterErrorKind::Unknown
                }
            }
        }
    }

    fn normalize_google_lang(
        lang: Lang,
    ) -> Result<&'static str, AdapterError> {
        match lang {
            Lang::Ja => Ok("ja"),
            Lang::Zh => Ok("zh-CN"),
            Lang::En => Ok("en"),
            Lang::De => Ok("de"),

            Lang::Auto => Err(AdapterError::new(
                ProviderKind::Google,
                AdapterErrorKind::InvalidConfig,
                "Google API-key v2 adapter requires explicit source/target language",
            )),
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

    fn sanitize_message(msg: &str, api_key: &str) -> String {
        if api_key.is_empty() {
            msg.to_string()
        } else {
            msg.replace(api_key, "<GOOGLE_API_KEY_MASKED>")
        }
    }

    fn print_proxy_env() {
        println!("[TRANSLATE][GOOGLE] HTTPS_PROXY={:?}", env::var("HTTPS_PROXY"));
        println!("[TRANSLATE][GOOGLE] HTTP_PROXY={:?}", env::var("HTTP_PROXY"));
        println!("[TRANSLATE][GOOGLE] NO_PROXY={:?}", env::var("NO_PROXY"));
    }

    fn build_client() -> Result<Client, AdapterError> {
        Client::builder()
            .connect_timeout(Duration::from_secs(
                GOOGLE_HTTP_CONNECT_TIMEOUT_SEC,
            ))
            .timeout(Duration::from_secs(
                GOOGLE_HTTP_TIMEOUT_SEC,
            ))
            .build()
            .map_err(|e| {
                AdapterError::new(
                    ProviderKind::Google,
                    AdapterErrorKind::InvalidConfig,
                    format!(
                        "failed to build google reqwest client: {}",
                        e
                    ),
                )
            })
    }
}

impl Default for GoogleAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TranslatorAdapter for GoogleAdapter {
    fn provider_kind(&self) -> ProviderKind {
        ProviderKind::Google
    }

    fn translate_once(
        &self,
        request: &TranslateRequest,
    ) -> Result<TranslateResponse, AdapterError> {
        let results =
            self.translate_batch(&[request.clone()])?;

        results.into_iter().next().ok_or_else(|| {
            AdapterError::new(
                ProviderKind::Google,
                AdapterErrorKind::Empty,
                "Google returned empty result for single request",
            )
        })
    }

    fn translate_batch(
        &self,
        requests: &[TranslateRequest],
    ) -> Result<Vec<TranslateResponse>, AdapterError> {
        if requests.is_empty() {
            return Ok(vec![]);
        }

        println!(
            "[TRANSLATE][GOOGLE] v2 api-key batch start count={}",
            requests.len()
        );

        let first_target = requests[0].to_lang;
        let first_source = requests[0].from_lang;

        for req in requests.iter().skip(1) {
            if req.to_lang != first_target
                || req.from_lang != first_source
            {
                return Err(AdapterError::new(
                    ProviderKind::Google,
                    AdapterErrorKind::InvalidConfig,
                    "Google batch requires same source/target lang within one batch",
                ));
            }
        }

        let api_key = Self::load_api_key()?;

        println!(
            "[TRANSLATE][GOOGLE] api-key loaded len={}",
            api_key.len()
        );
        Self::print_proxy_env();

        let source_lang =
            Self::normalize_google_lang(first_source)?;

        let target_lang =
            Self::normalize_google_lang(first_target)?;

        let request_body = GoogleV2TranslateRequestBody {
            q: requests
                .iter()
                .map(|r| r.text.clone())
                .collect(),

            source: source_lang.to_string(),
            target: target_lang.to_string(),
            format: "text".to_string(),
        };

        let client = Self::build_client()?;

        println!(
            "[TRANSLATE][GOOGLE] api post start endpoint=v2 source={} target={} timeout_sec={}",
            source_lang,
            target_lang,
            GOOGLE_HTTP_TIMEOUT_SEC
        );

        println!(
            "[TRANSLATE][GOOGLE] response wait start"
        );

        let response = client
            .post(GOOGLE_TRANSLATE_V2_URL)
            .query(&[("key", api_key.as_str())])
            .json(&request_body)
            .timeout(Duration::from_secs(
                GOOGLE_HTTP_TIMEOUT_SEC,
            ))
            .send();

        match &response {
            Ok(resp) => {
                println!(
                    "[TRANSLATE][GOOGLE] api response status={}",
                    resp.status()
                );
            }

            Err(e) => {
                let raw = format!("{:?}", e);
                let sanitized = Self::sanitize_message(&raw, &api_key);
                println!(
                    "[TRANSLATE][GOOGLE][ERROR] response error={}",
                    sanitized
                );
            }
        }

        let response = response.map_err(|e| {
            let msg = e.to_string();
            let sanitized = Self::sanitize_message(&msg, &api_key);
            let summary = Self::summarize_reqwest_error(&sanitized);
            let kind = Self::classify_reqwest_error(&sanitized);

            println!(
                "[TRANSLATE][GOOGLE][ERROR_CLASS] {} kind={:?}",
                summary,
                kind
            );

            AdapterError::new(
                ProviderKind::Google,
                kind,
                format!(
                    "google v2 api-key request failed: class={} message={}",
                    summary,
                    sanitized
                ),
            )
        })?;

        let status = response.status();

        let body_text = response.text().map_err(|e| {
            AdapterError::new(
                ProviderKind::Google,
                AdapterErrorKind::Parse,
                format!(
                    "google body read failed: {}",
                    e
                ),
            )
        })?;

        println!(
            "[TRANSLATE][GOOGLE] response body len={}",
            body_text.len()
        );

        if !status.is_success() {
            println!(
                "[TRANSLATE][GOOGLE][ERROR] http status={} body={}",
                status.as_u16(),
                body_text
            );

            return Err(AdapterError::new(
                ProviderKind::Google,
                Self::map_http_error(
                    status.as_u16(),
                    &body_text,
                ),
                format!(
                    "google v2 http error: status={} body={}",
                    status.as_u16(),
                    body_text
                ),
            ));
        }

        let parsed: GoogleV2TranslateResponseBody =
            serde_json::from_str(&body_text).map_err(|e| {
                AdapterError::new(
                    ProviderKind::Google,
                    AdapterErrorKind::Parse,
                    format!(
                        "google v2 json parse failed: {} body={}",
                        e,
                        body_text
                    ),
                )
            })?;

        if parsed.data.translations.is_empty() {
            return Err(AdapterError::new(
                ProviderKind::Google,
                AdapterErrorKind::Empty,
                "Google returned no translations",
            ));
        }

        if parsed.data.translations.len() != requests.len() {
            return Err(AdapterError::new(
                ProviderKind::Google,
                AdapterErrorKind::Parse,
                format!(
                    "Google response count mismatch: requests={} responses={}",
                    requests.len(),
                    parsed.data.translations.len()
                ),
            ));
        }

        if let Some(first_req) = requests.first() {
            if let Some(first_trans) =
                parsed.data.translations.first()
            {
                println!(
                    "[TRANSLATE][GOOGLE] sample {} -> {}",
                    first_req.text,
                    first_trans.translated_text
                );
            }
        }

        let mut results = Vec::with_capacity(
            parsed.data.translations.len(),
        );

        for (req, trans) in requests
            .iter()
            .zip(parsed.data.translations.into_iter())
        {
            results.push(TranslateResponse {
                request_id: req.request_id.clone(),
                provider: ProviderKind::Google,
                translated_text: trans.translated_text,

                detected_source_lang:
                    Self::map_detected_lang(
                        trans.detected_source_language
                            .as_deref(),
                    ),

                raw_meta: None,
            });
        }

        println!(
            "[TRANSLATE][GOOGLE] v2 api-key batch ok count={}",
            results.len()
        );

        Ok(results)
    }
}
