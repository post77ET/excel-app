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

// ============================================================
// CandidateConfig（C-1 で導入）
//
// 候補ごとの「provider × method」を表す単一モデル。
// 今後 analyzer 実行・estimate・ラベル・Apply は、候補設定を
// 直接 candidateN_provider から読むのではなく、本 CandidateConfig
// （JobPlanSettings::candidate_config 経由）を参照することで一本化する。
//
// 注意（二重管理の回避）:
// CandidateConfig は JobPlanSettings の既存フィールドから「導出するビュー」であり、
// データの second copy を持たない。method のみが新規データ。
// ============================================================

/// 翻訳方式。split=分割翻訳 / whole=文脈翻訳（セル全体）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Split,
    Whole,
}

impl Method {
    pub fn as_label(&self) -> &'static str {
        match self {
            Method::Split => "split",
            Method::Whole => "whole",
        }
    }

    /// 候補番号ごとの既定方式（後方互換）: 1=split, 2=split, 3=whole。
    pub fn default_for_index(index: u8) -> Method {
        match index {
            3 => Method::Whole,
            _ => Method::Split,
        }
    }
}

/// 文字列 -> Method。未知/None は呼び出し側で default_for_index に委ねる。
pub fn parse_method(s: &str) -> Option<Method> {
    match s.trim().to_ascii_lowercase().as_str() {
        "split" => Some(Method::Split),
        "whole" | "context" | "whole_cell" | "wholecell" => Some(Method::Whole),
        _ => None,
    }
}

/// 候補1件分の確定設定（provider × method × enabled）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateConfig {
    pub index: u8,
    pub provider: Option<ProviderKind>,
    pub method: Method,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawJobPlanSettings {
    pub mode: Option<String>,
    pub plan_name: Option<String>,
    pub enabled_candidates: Option<Vec<u8>>,
    pub candidate1_provider: Option<String>,
    pub candidate2_provider: Option<String>,
    pub candidate3_provider: Option<String>,
    // C-1: 候補ごとの翻訳方式（省略可・後方互換）。省略時は default_for_index で補完。
    pub candidate1_method: Option<String>,
    pub candidate2_method: Option<String>,
    pub candidate3_method: Option<String>,
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
    // C-1: 候補ごとの翻訳方式。既定は 1=split,2=split,3=whole。
    pub candidate1_method: Method,
    pub candidate2_method: Method,
    pub candidate3_method: Method,
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
            candidate1_method: Method::default_for_index(1),
            candidate2_method: Method::default_for_index(2),
            candidate3_method: Method::default_for_index(3),
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

    /// C-1: 候補番号 -> CandidateConfig（単一の参照点）。
    /// provider/method/enabled を既存フィールドから導出（second copy を持たない）。
    pub fn candidate_config(&self, index: u8) -> CandidateConfig {
        let (provider, method) = match index {
            1 => (self.candidate1_provider, self.candidate1_method),
            2 => (self.candidate2_provider, self.candidate2_method),
            3 => (self.candidate3_provider, self.candidate3_method),
            _ => (None, Method::default_for_index(index)),
        };
        CandidateConfig {
            index,
            provider,
            method,
            enabled: self.is_enabled(index),
        }
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
        candidate1_method: raw.candidate1_method
            .as_deref()
            .and_then(parse_method)
            .unwrap_or(Method::default_for_index(1)),
        candidate2_method: raw.candidate2_method
            .as_deref()
            .and_then(parse_method)
            .unwrap_or(Method::default_for_index(2)),
        candidate3_method: raw.candidate3_method
            .as_deref()
            .and_then(parse_method)
            .unwrap_or(Method::default_for_index(3)),
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
