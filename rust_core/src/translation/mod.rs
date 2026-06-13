pub mod amazon;
pub mod batch;
pub mod deepl;
pub mod env_tools;
pub mod google;
pub mod types;

use anyhow::Result;

pub use batch::{
    execute_whole_rows_buffered,
    execute_worktable_rows_buffered,
};
pub use env_tools::{load_dotenv_if_exists, print_env_check};
pub use types::{
    BatchLimits,
    CandidateBundle,
    CandidateOutput,
    CandidatePlan,
    CandidateSlot,
    IndexedTextUnit,
    TranslationProvider,
    TranslationProviderDisplay,
    TranslationRequest,
    TranslationRequestOwned,
    WorkTableRow,
    WorkTableTranslatedRow,
};

pub async fn translate_with_provider(
    provider: TranslationProvider,
    request: &TranslationRequest<'_>,
) -> Result<String> {
    match provider {
        TranslationProvider::Amazon => amazon::translate(request).await,
        TranslationProvider::Google => google::translate(request).await,
        TranslationProvider::DeepL => deepl::translate(request).await,
    }
}

pub async fn translate_many_with_provider(
    provider: TranslationProvider,
    requests: &[TranslationRequestOwned],
    amazon_parallelism: usize,
) -> Result<Vec<String>> {
    match provider {
        TranslationProvider::Amazon => {
            amazon::translate_many_parallel(requests, amazon_parallelism).await
        }
        TranslationProvider::Google => google::translate_many(requests).await,
        TranslationProvider::DeepL => deepl::translate_many(requests).await,
    }
}

pub async fn run_candidate_plan(
    plan: &CandidatePlan,
    request: &TranslationRequest<'_>,
) -> Vec<CandidateOutput> {
    let mut outputs = Vec::with_capacity(3);

    for (slot, provider) in [
        (CandidateSlot::Candidate1, plan.candidate1),
        (CandidateSlot::Candidate2, plan.candidate2),
        (CandidateSlot::Candidate3, plan.candidate3),
    ] {
        let result = translate_with_provider(provider, request).await;
        outputs.push(CandidateOutput {
            slot,
            provider,
            result,
        });
    }

    outputs
}