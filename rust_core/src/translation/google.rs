use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use yup_oauth2::{read_service_account_key, ServiceAccountAuthenticator};

use super::types::{TranslationRequest, TranslationRequestOwned};

#[derive(Debug, Deserialize)]
struct GoogleServiceAccountJson {
    project_id: String,
}

#[derive(Debug, Serialize)]
struct GoogleTranslateRequest {
    contents: Vec<String>,
    mime_type: String,
    source_language_code: String,
    target_language_code: String,
}

#[derive(Debug, Deserialize)]
struct GoogleTranslateResponse {
    translations: Vec<GoogleTranslatedText>,
}

#[derive(Debug, Deserialize)]
struct GoogleTranslatedText {
    #[serde(rename = "translatedText")]
    translated_text: String,
}

pub async fn translate(request: &TranslationRequest<'_>) -> Result<String> {
    let owned = TranslationRequestOwned {
        text: request.text.to_string(),
        source_lang: request.source_lang.to_string(),
        target_lang: request.target_lang.to_string(),
    };

    let mut out = translate_many(&[owned]).await?;
    out.pop().ok_or_else(|| anyhow!("Google translation missing"))
}

pub async fn translate_many(requests: &[TranslationRequestOwned]) -> Result<Vec<String>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let key_path = env::var("GOOGLE_APPLICATION_CREDENTIALS")
        .context("GOOGLE_APPLICATION_CREDENTIALS missing")?;

    println!("Google key path={}", key_path);

    let json_text = std::fs::read_to_string(&key_path)
        .with_context(|| format!("Failed to read Google key file: {}", key_path))?;

    let sa_json: GoogleServiceAccountJson =
        serde_json::from_str(&json_text).context("Failed to parse Google key JSON")?;

    println!("Google project_id={}", sa_json.project_id);

    let sa_key = read_service_account_key(&key_path)
        .await
        .with_context(|| format!("Failed to load service account key: {}", key_path))?;

    let auth = ServiceAccountAuthenticator::builder(sa_key)
        .build()
        .await
        .context("Failed to build Google authenticator")?;

    let token = auth
        .token(&["https://www.googleapis.com/auth/cloud-platform"])
        .await
        .map_err(|e| anyhow!("Failed to get Google OAuth token: {:?}", e))?;

    let access_token = token
        .token()
        .ok_or_else(|| anyhow!("Google access token missing"))?;

    let first = &requests[0];
    let url = format!(
        "https://translation.googleapis.com/v3/projects/{}/locations/global:translateText",
        sa_json.project_id
    );

    let request_body = GoogleTranslateRequest {
        contents: requests.iter().map(|r| r.text.clone()).collect(),
        mime_type: "text/plain".to_string(),
        source_language_code: normalize_google_lang(&first.source_lang).to_string(),
        target_language_code: normalize_google_lang(&first.target_lang).to_string(),
    };

    let client = Client::new();
    let response = client
        .post(url)
        .bearer_auth(access_token)
        .json(&request_body)
        .send()
        .await
        .context("Google Translate HTTP request failed")?;

    let status = response.status();
    let body = response.text().await.context("Failed to read Google body")?;

    if !status.is_success() {
        return Err(anyhow!("Google Translate failed: {} / {}", status, body));
    }

    let parsed: GoogleTranslateResponse =
        serde_json::from_str(&body).context("Failed to parse Google response JSON")?;

    let translated: Vec<String> = parsed
        .translations
        .into_iter()
        .map(|t| t.translated_text)
        .collect();

    if translated.len() != requests.len() {
        return Err(anyhow!(
            "Google result count mismatch: expected {}, got {}",
            requests.len(),
            translated.len()
        ));
    }

    Ok(translated)
}

fn normalize_google_lang(lang: &str) -> &str {
    match lang {
        "ja" | "JA" => "ja",
        "zh" | "ZH" => "zh-CN",
        "zh-CN" | "ZH-CN" => "zh-CN",
        _ => lang,
    }
}