use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::env;

use super::types::{TranslationRequest, TranslationRequestOwned};

#[derive(Debug, Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Debug, Deserialize)]
struct DeepLTranslation {
    text: String,
}

pub async fn translate(request: &TranslationRequest<'_>) -> Result<String> {
    let owned = TranslationRequestOwned {
        text: request.text.to_string(),
        source_lang: request.source_lang.to_string(),
        target_lang: request.target_lang.to_string(),
    };

    let mut out = translate_many(&[owned]).await?;
    out.pop().ok_or_else(|| anyhow!("DeepL translation missing"))
}

pub async fn translate_many(requests: &[TranslationRequestOwned]) -> Result<Vec<String>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let api_key = env::var("DEEPL_API_KEY").context("DEEPL_API_KEY missing")?;
    let endpoint = if api_key.ends_with(":fx") {
        "https://api-free.deepl.com/v2/translate"
    } else {
        "https://api.deepl.com/v2/translate"
    };

    let first = &requests[0];
    let mut form_data: Vec<(String, String)> = vec![
        ("source_lang".to_string(), normalize_deepl_lang(&first.source_lang).to_string()),
        ("target_lang".to_string(), normalize_deepl_lang(&first.target_lang).to_string()),
    ];

    for req in requests {
        form_data.push(("text".to_string(), req.text.clone()));
    }

    let client = Client::new();
    let response = client
        .post(endpoint)
        .header("Authorization", format!("DeepL-Auth-Key {}", api_key))
        .form(&form_data)
        .send()
        .await
        .context("DeepL HTTP request failed")?;

    let status = response.status();
    let body = response.text().await.context("Failed to read DeepL body")?;

    if !status.is_success() {
        return Err(anyhow!("DeepL failed: {} / {}", status, body));
    }

    let parsed: DeepLResponse =
        serde_json::from_str(&body).context("Failed to parse DeepL response JSON")?;

    let translated: Vec<String> = parsed
        .translations
        .into_iter()
        .map(|t| t.text)
        .collect();

    if translated.len() != requests.len() {
        return Err(anyhow!(
            "DeepL result count mismatch: expected {}, got {}",
            requests.len(),
            translated.len()
        ));
    }

    Ok(translated)
}

fn normalize_deepl_lang(lang: &str) -> &str {
    match lang {
        "ja" | "JA" => "JA",
        "zh" | "ZH" | "zh-CN" | "ZH-CN" => "ZH",
        _ => lang,
    }
}