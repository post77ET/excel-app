use std::fs;
use std::path::Path;

use crate::core1::analyzer::{estimate_candidate_usage, CandidateGenerationPlan};
use crate::core1::translation_policy::decide_translation_policy;
use crate::core2::source_workbook_reader::read_source_logical_cells;
use crate::entry::entry_state::EntryError;
use crate::entry::job_plan_settings::{load_job_plan_settings, EXPERIENCE_RANGE_LABEL};
use crate::entry::sheet_select_cli::{confirm, select_sheets};
use crate::entry::workbook_sheet_inventory::load_sheet_inventory;
use crate::infra::config_loader::load_translator_config;
use crate::planning::ExecutionPlan;
use crate::plan::CellScope;
use crate::pricing::estimate::{calculate_billing_estimate, BillingEstimate, EstimateInput};
use crate::security::pipeline::{inspect_xlsx, print_report};
use crate::security::types::SecurityResult;

pub fn run_estimate_select_pipeline(input_path: &str) -> Result<BillingEstimate, EntryError> {
    println!("=== ENTRY estimate-select start ===");
    let input = Path::new(input_path);
    if !input.exists() {
        return Err(EntryError::Internal(format!("input file not found: {input_path}")));
    }

    let input_str = input.to_string_lossy().to_string();

    let security_report = inspect_xlsx(&input_str);
    print_report(&security_report);
    if security_report.final_result == SecurityResult::Reject {
        return Err(EntryError::Internal(format!("security rejected input workbook: {}", input_str)));
    }

    let inventory = load_sheet_inventory(input).map_err(|e| EntryError::Internal(format!("{:?}", e)))?;
    let job_plan = load_job_plan_settings();
    let cfg = load_translator_config();

    // === Phase 1: ExecutionPlan を確定し direction / plan を resolve（恒等マッピング）===
    let execution_plan = ExecutionPlan::from_runtime(&job_plan);
    let direction_profile = execution_plan
        .resolve_direction()
        .map_err(EntryError::Internal)?;
    let plan_policy = execution_plan
        .resolve_plan()
        .map_err(EntryError::Internal)?;
    let cell_scope = plan_policy.cell_scope();
    println!(
        "[EXECUTION_PLAN][RESOLVED][ESTIMATE] plan={} cell_scope={:?}",
        plan_policy.id(),
        cell_scope
    );
    println!("[ETB_PHASE1_MARKER] estimate path reached: plan={}", plan_policy.id());

    let selected_sheets = loop {
        let selected = match select_sheets(&inventory.sheets, job_plan.is_experience()) {
            Ok(v) => v,
            Err(EntryError::UserExit) => return Err(EntryError::UserExit),
            Err(e) => return Err(EntryError::Internal(format!("{:?}", e))),
        };

        match confirm(&selected) {
            Ok(()) => break selected,
            Err(EntryError::UserBack) => continue,
            Err(EntryError::UserExit) => return Err(EntryError::UserExit),
            Err(e) => return Err(EntryError::Internal(format!("{:?}", e))),
        }
    };

    let candidate_generation_plan = CandidateGenerationPlan {
        enabled_candidates: job_plan.enabled_candidates.clone(),
        default_candidate_priority: job_plan.default_candidate_priority.clone(),
        job_accept_threshold: job_plan.job_accept_threshold,
    };

    let old_input = std::env::var("ETB_INPUT_PATH").ok();
    let old_target = std::env::var("ETB_TARGET_SHEET").ok();
    std::env::set_var("ETB_INPUT_PATH", &input_str);

    let mut total_logical_cells = 0usize;
    let mut total_translatable_cells = 0usize;
    let mut total_candidate_units = 0usize;
    let mut total_c1_requests = 0usize;
    let mut total_c1_chars = 0usize;
    let mut total_c2_requests = 0usize;
    let mut total_c2_chars = 0usize;
    let mut total_c3_requests = 0usize;
    let mut total_c3_chars = 0usize;

    for sheet in &selected_sheets {
        println!("[ESTIMATE] processing sheet = {}", sheet);
        std::env::set_var("ETB_TARGET_SHEET", sheet);
        let mut logical_cells = read_source_logical_cells().map_err(|e| EntryError::Internal(format!("{:?}", e)))?;

        // Phase 1: 範囲制限を plan_policy.cell_scope() の値で実際に判定する（現行と同値）。
        if let CellScope::Range { .. } = cell_scope {
            let before = logical_cells.len();
            logical_cells.retain(|cell| cell_scope.contains_address(&cell.anchor_address));
            println!(
                "[EXPERIENCE][ESTIMATE] sheet={} range={} target_cells={} filtered_out={}",
                sheet,
                EXPERIENCE_RANGE_LABEL,
                logical_cells.len(),
                before.saturating_sub(logical_cells.len())
            );
        }

        total_logical_cells += logical_cells.len();

        let mut policies = Vec::with_capacity(logical_cells.len());
        for logical_cell in &logical_cells {
            let policy = decide_translation_policy(logical_cell.cell_kind, &logical_cell.source_text, direction_profile.as_ref());
            policies.push(policy);
        }

        let usage = estimate_candidate_usage(&logical_cells, &policies, &candidate_generation_plan, direction_profile.as_ref())
            .map_err(|e| EntryError::Internal(format!("{:?}", e)))?;

        println!(
            "[ESTIMATE] sheet={} logical_cells={} translatable_cells={} candidate_units={} requests={} chars={}",
            sheet,
            logical_cells.len(),
            usage.translatable_cells,
            usage.candidate_units,
            usage.total_requests(),
            usage.total_chars()
        );

        total_translatable_cells += usage.translatable_cells;
        total_candidate_units += usage.candidate_units;
        total_c1_requests += usage.candidate1_requests;
        total_c1_chars += usage.candidate1_chars;
        total_c2_requests += usage.candidate2_requests;
        total_c2_chars += usage.candidate2_chars;
        total_c3_requests += usage.candidate3_requests;
        total_c3_chars += usage.candidate3_chars;
    }

    match old_target {
        Some(v) => std::env::set_var("ETB_TARGET_SHEET", v),
        None => std::env::remove_var("ETB_TARGET_SHEET"),
    }
    match old_input {
        Some(v) => std::env::set_var("ETB_INPUT_PATH", v),
        None => std::env::remove_var("ETB_INPUT_PATH"),
    }

    let estimate = calculate_billing_estimate(EstimateInput {
        mode: job_plan.mode.as_label().to_string(),
        selected_sheets,
        logical_cells: total_logical_cells,
        translatable_cells: total_translatable_cells,
        candidate_units: total_candidate_units,
        candidate1_provider: if job_plan.is_enabled(1) { Some(job_plan.candidate1_provider.unwrap_or(cfg.candidate1_provider)) } else { None },
        candidate1_requests: total_c1_requests,
        candidate1_chars: total_c1_chars,
        candidate2_provider: if job_plan.is_enabled(2) { Some(job_plan.candidate2_provider.unwrap_or(cfg.candidate2_provider)) } else { None },
        candidate2_requests: total_c2_requests,
        candidate2_chars: total_c2_chars,
        candidate3_provider: if job_plan.is_enabled(3) { Some(job_plan.candidate3_provider.unwrap_or(cfg.candidate3_provider)) } else { None },
        candidate3_requests: total_c3_requests,
        candidate3_chars: total_c3_chars,
    });

    estimate.print_for_powershell();

    if let Ok(path) = std::env::var("ETB_ESTIMATE_OUTPUT") {
        let json = serde_json::to_string_pretty(&estimate)
            .map_err(|e| EntryError::Internal(format!("estimate json serialize failed: {e}")))?;
        fs::write(&path, json)
            .map_err(|e| EntryError::Internal(format!("estimate json write failed: {path}: {e}")))?;
        println!("[ESTIMATE] json_output = {}", path);
    }

    println!("=== ENTRY estimate-select end ===");
    Ok(estimate)
}
