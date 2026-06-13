use crate::adapters::types::ProviderKind;
use serde::Deserialize;
use std::fs;
use std::path::Path;

// ============================================================
// JOB PLAN SETTINGS
//
// IMPORTANT:
//
// This module is the Rust-side execution contract for course / plan
// selection. The Web UI may create the json file, but Rust must still
// validate and execute it.
//
// Candidate1/2/3 are translation METHODS.
// Google/Amazon/DeepL are PROVIDERS.
//
// Do NOT fake candidate columns. If candidate1 fails, candidate2 must
// never be copied into candidate1. Only DefaultSelect may fallback to
// another successful candidate.
//
// DeepL must not be assigned to Candidate1 or Candidate2 because those
// methods use split translation. DeepL is intended for Candidate3
// whole-cell context translation.
//
// Experience course is NOT a separate generate/apply system.
// It is the normal production flow with only the translation support
// target range restricted to A1:D5.
// ============================================================

pub const EXPERIENCE_RANGE_LABEL: &str = "A1:D5";
pub const EXPERIENCE_MAX_ROW: u32 = 5;
pub const EXPERIENCE_MAX_COL: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobCourseMode {
    Experience,
    Paid,
}

impl JobCourseMode {
    pub fn as_label(&self) -> &'static str {
        match self {
            JobCourseMode::Experience => "experience",
            JobCourseMode::Paid => "paid",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawJobPlanSettings {
    pub mode: Option<String>,
    pub plan_name: Option<String>,
    pub enabled_candidates: Option<Vec<u8>>,
    pub candidate1_provider: Option<String>,
    pub candidate2_provider: Option<String>,
    pub candidate3_provider: Option<String>,
    pub default_candidate_priority: Option<Vec<u8>>,
    pub job_accept_threshold: Option<f64>,
    pub experience_range: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JobPlanSettings {
    pub mode: JobCourseMode,
    pub plan_name: String,
    pub enabled_candidates: Vec<u8>,
    pub candidate1_provider: Option<ProviderKind>,
    pub candidate2_provider: Option<ProviderKind>,
    pub candidate3_provider: Option<ProviderKind>,
    pub default_candidate_priority: Vec<u8>,
    pub job_accept_threshold: f64,
    pub experience_range: String,
}

impl Default for JobPlanSettings {
    fn default() -> Self {
        Self {
            mode: JobCourseMode::Paid,
            plan_name: "STANDARD_3_CANDIDATES_DEFAULT".to_string(),
            enabled_candidates: vec![1, 2, 3],
            candidate1_provider: Some(ProviderKind::Google),
            candidate2_provider: Some(ProviderKind::Amazon),
            candidate3_provider: Some(ProviderKind::DeepL),
            default_candidate_priority: vec![1, 2, 3],
            job_accept_threshold: 0.80,
            experience_range: EXPERIENCE_RANGE_LABEL.to_string(),
        }
    }
}

impl JobPlanSettings {
    pub fn is_enabled(&self, candidate_no: u8) -> bool {
        self.enabled_candidates.contains(&candidate_no)
    }

    pub fn is_experience(&self) -> bool {
        self.mode == JobCourseMode::Experience
    }

    pub fn normalized_priority(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for &v in &self.default_candidate_priority {
            if (1..=3).contains(&v) && self.is_enabled(v) && !out.contains(&v) {
                out.push(v);
            }
        }
        for &v in &self.enabled_candidates {
            if (1..=3).contains(&v) && !out.contains(&v) {
                out.push(v);
            }
        }
        out
    }
}

pub fn load_job_plan_settings() -> JobPlanSettings {
    let path = std::env::var("ETB_JOB_PLAN_CONFIG")
        .unwrap_or_else(|_| "config/job_plan_settings.json".to_string());

    if !Path::new(&path).exists() {
        let plan = JobPlanSettings::default();
        print_job_plan(&plan, &path, false);
        return plan;
    }

    let text = match fs::read_to_string(&path) {
        Ok(v) => v,
        Err(e) => {
            println!("[JOB_PLAN][WARN] failed to read {}: {}. Use default plan.", path, e);
            let plan = JobPlanSettings::default();
            print_job_plan(&plan, &path, false);
            return plan;
        }
    };

    let raw: RawJobPlanSettings = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            println!("[JOB_PLAN][WARN] failed to parse {}: {}. Use default plan.", path, e);
            let plan = JobPlanSettings::default();
            print_job_plan(&plan, &path, false);
            return plan;
        }
    };

    let mut plan = normalize_raw_plan(raw);
    validate_and_repair_plan(&mut plan);
    print_job_plan(&plan, &path, true);
    plan
}

fn normalize_raw_plan(raw: RawJobPlanSettings) -> JobPlanSettings {
    let default = JobPlanSettings::default();

    JobPlanSettings {
        mode: raw.mode.as_deref().map(parse_mode).unwrap_or(default.mode),
        plan_name: raw.plan_name.unwrap_or(default.plan_name),
        enabled_candidates: normalize_candidates(raw.enabled_candidates.unwrap_or(default.enabled_candidates)),
        candidate1_provider: raw.candidate1_provider
            .as_deref()
            .and_then(parse_provider)
            .or(default.candidate1_provider),
        candidate2_provider: raw.candidate2_provider
            .as_deref()
            .and_then(parse_provider)
            .or(default.candidate2_provider),
        candidate3_provider: raw.candidate3_provider
            .as_deref()
            .and_then(parse_provider)
            .or(default.candidate3_provider),
        default_candidate_priority: normalize_candidates(raw.default_candidate_priority.unwrap_or(default.default_candidate_priority)),
        job_accept_threshold: raw.job_accept_threshold.unwrap_or(default.job_accept_threshold),
        experience_range: raw.experience_range.unwrap_or(default.experience_range),
    }
}

fn normalize_candidates(values: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    for v in values {
        if (1..=3).contains(&v) && !out.contains(&v) {
            out.push(v);
        }
    }
    if out.is_empty() {
        vec![1, 2, 3]
    } else {
        out
    }
}

fn validate_and_repair_plan(plan: &mut JobPlanSettings) {
    if plan.is_enabled(1) && matches!(plan.candidate1_provider, Some(ProviderKind::DeepL)) {
        println!("[JOB_PLAN][WARN] Candidate1 cannot use DeepL. Fallback to Google.");
        plan.candidate1_provider = Some(ProviderKind::Google);
    }

    if plan.is_enabled(2) && matches!(plan.candidate2_provider, Some(ProviderKind::DeepL)) {
        println!("[JOB_PLAN][WARN] Candidate2 cannot use DeepL. Fallback to Amazon.");
        plan.candidate2_provider = Some(ProviderKind::Amazon);
    }

    if plan.is_enabled(3) && plan.candidate3_provider.is_none() {
        plan.candidate3_provider = Some(ProviderKind::DeepL);
    }

    if !(0.0..=1.0).contains(&plan.job_accept_threshold) {
        println!("[JOB_PLAN][WARN] invalid job_accept_threshold. Fallback to 0.80.");
        plan.job_accept_threshold = 0.80;
    }

    if plan.is_experience() && plan.experience_range.trim() != EXPERIENCE_RANGE_LABEL {
        println!(
            "[JOB_PLAN][WARN] experience range is fixed to {}. Ignore configured value: {}",
            EXPERIENCE_RANGE_LABEL,
            plan.experience_range
        );
        plan.experience_range = EXPERIENCE_RANGE_LABEL.to_string();
    }

    plan.default_candidate_priority = plan.normalized_priority();
}

fn parse_mode(value: &str) -> JobCourseMode {
    match value.trim().to_ascii_lowercase().as_str() {
        "experience" | "trial" | "free" | "体験" => JobCourseMode::Experience,
        "paid" | "full" | "有料" => JobCourseMode::Paid,
        other => {
            println!("[JOB_PLAN][WARN] unknown mode '{}'. Fallback to paid.", other);
            JobCourseMode::Paid
        }
    }
}

fn parse_provider(value: &str) -> Option<ProviderKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "google" => Some(ProviderKind::Google),
        "amazon" => Some(ProviderKind::Amazon),
        "deepl" | "deep_l" => Some(ProviderKind::DeepL),
        "mock" => Some(ProviderKind::Mock),
        _ => None,
    }
}

fn print_job_plan(plan: &JobPlanSettings, path: &str, loaded: bool) {
    println!("[JOB_PLAN] source={} path={}", if loaded { "file" } else { "default" }, path);
    println!("[JOB_PLAN] mode={}", plan.mode.as_label());
    println!("[JOB_PLAN] plan_name={}", plan.plan_name);
    println!("[JOB_PLAN] enabled_candidates={:?}", plan.enabled_candidates);
    println!("[JOB_PLAN] candidate1_provider={:?}", plan.candidate1_provider.map(|v| v.as_label()));
    println!("[JOB_PLAN] candidate2_provider={:?}", plan.candidate2_provider.map(|v| v.as_label()));
    println!("[JOB_PLAN] candidate3_provider={:?}", plan.candidate3_provider.map(|v| v.as_label()));
    println!("[JOB_PLAN] default_candidate_priority={:?}", plan.default_candidate_priority);
    println!("[JOB_PLAN] job_accept_threshold={:.2}", plan.job_accept_threshold);
    if plan.is_experience() {
        println!("[JOB_PLAN] experience_range={}", plan.experience_range);
    }
}
