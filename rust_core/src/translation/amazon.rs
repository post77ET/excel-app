use anyhow::{anyhow, Result};
use aws_config::{timeout::TimeoutConfig, BehaviorVersion};
use aws_sdk_translate::types::{Formality, TranslationSettings};
use aws_sdk_translate::Client as AwsTranslateClient;
use std::time::Duration;
use tokio::task::JoinSet;

use super::types::{TranslationRequest, TranslationRequestOwned};

pub async fn translate(request: &TranslationRequest<'_>) -> Result<String> {
    let client = build_client().await;
    translate_with_client(&client, request).await
}

pub async fn translate_many_parallel(
    requests: &[TranslationRequestOwned],
    parallelism: usize,
) -> Result<Vec<String>> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let client = build_client().await;
    let mut results: Vec<Option<String>> = vec![None; requests.len()];
    let mut next_index = 0usize;
    let limit = parallelism.max(1);

    while next_index < requests.len() {
        let mut join_set = JoinSet::new();
        let end = (next_index + limit).min(requests.len());

        for idx in next_index..end {
            let req = requests[idx].clone();
            let client_cloned = client.clone();

            join_set.spawn(async move {
                let borrowed = req.as_borrowed();
                let result = translate_with_client(&client_cloned, &borrowed).await;
                (idx, result)
            });
        }

        while let Some(joined) = join_set.join_next().await {
            let (idx, result) = joined.map_err(|e| anyhow!("Amazon join error: {}", e))?;
            results[idx] = Some(result?);
        }

        next_index = end;
    }

    let collected: Result<Vec<String>> = results
        .into_iter()
        .map(|opt| opt.ok_or_else(|| anyhow!("Amazon result missing")))
        .collect();

    collected
}

async fn build_client() -> AwsTranslateClient {
    let timeout_config = TimeoutConfig::builder()
        .connect_timeout(Duration::from_secs(20))
        .operation_attempt_timeout(Duration::from_secs(60))
        .operation_timeout(Duration::from_secs(90))
        .build();

    let config = aws_config::defaults(BehaviorVersion::latest())
        .timeout_config(timeout_config)
        .load()
        .await;

    AwsTranslateClient::new(&config)
}

async fn translate_with_client(
    client: &AwsTranslateClient,
    request: &TranslationRequest<'_>,
) -> Result<String> {
    let result = client
        .translate_text()
        .text(request.text)
        .source_language_code(normalize_amazon_lang(request.source_lang))
        .target_language_code(normalize_amazon_lang(request.target_lang))
        .settings(
            TranslationSettings::builder()
                .formality(Formality::Formal)
                .build(),
        )
        .send()
        .await;

    match result {
        Ok(response) => Ok(response.translated_text().to_string()),
        Err(e) => Err(anyhow!("Amazon Translate ERROR FULL: {:?}", e)),
    }
}

fn normalize_amazon_lang(lang: &str) -> &str {
    match lang {
        "ja" | "JA" => "ja",
        "zh" | "ZH" | "zh-CN" | "ZH-CN" => "zh",
        _ => lang,
    }
}