use std::path::Path;

use crate::adapters::provider_factory::create_adapter;
use crate::core1::analyzer::{build_candidate_bundles_batch, CandidateGenerationPlan};
use crate::core1::default_select::decide_default_select;
use crate::core1::translation_policy::decide_translation_policy;
use crate::core1::types::DefaultSelect;
use crate::core2::generate_workbook_writer::write_generate_workbook;
use crate::core2::source_workbook_reader::read_source_logical_cells;
use crate::entry::entry_state::{EntryError, GenerateSelectResult};
use crate::entry::replica_preparer::store_original_and_create_replica;
use crate::entry::runtime_paths::{build_job_paths, ensure_runtime_dirs, project_root};
use crate::entry::sheet_select_cli::{confirm, select_sheets};
use crate::entry::workbook_sheet_inventory::load_sheet_inventory;
use crate::infra::config_loader::load_translator_config;
use crate::planning::ExecutionPlan;
use crate::plan::CellScope;
use crate::entry::job_plan_settings::{load_job_plan_settings, EXPERIENCE_RANGE_LABEL};
use crate::security::pipeline::{inspect_xlsx, print_report};
use crate::security::types::SecurityResult;
use crate::ui::types::UiRow;
use crate::ui::ui_sheet_builder::build_ui_row;

pub fn run_generate_select_pipeline(input_path: &str) -> Result<GenerateSelectResult, EntryError> {
    println!("=== ENTRY generate-select start ===");
    let input = Path::new(input_path);
    if !input.exists() {
        return Err(EntryError::Internal(format!("input file not found: {input_path}")));
    }

    let root = project_root().map_err(|e| EntryError::Internal(format!("{:?}", e)))?;
    ensure_runtime_dirs(&root).map_err(|e| EntryError::Internal(format!("{:?}", e)))?;
    let job_paths = build_job_paths(&root, input).map_err(|e| EntryError::Internal(format!("{:?}", e)))?;

    println!("[ENTRY] original = {}", job_paths.original_path.display());
    println!("[ENTRY] replica  = {}", job_paths.replica_path.display());
    println!("[ENTRY] output   = {}", job_paths.output_ui_path.display());

    store_original_and_create_replica(input, &job_paths).map_err(|e| EntryError::Internal(format!("{:?}", e)))?;

    let replica_str = job_paths.replica_path.to_string_lossy().to_string();
    let security_report = inspect_xlsx(&replica_str);
    print_report(&security_report);
    if security_report.final_result == SecurityResult::Reject {
        return Err(EntryError::Internal(format!("security rejected input workbook: {}", replica_str)));
    }
    let inventory = load_sheet_inventory(&job_paths.replica_path).map_err(|e| EntryError::Internal(format!("{:?}", e)))?;
    let job_plan = load_job_plan_settings();

    // === Phase 1: ExecutionPlan を確定し、direction / plan を resolve する ===
    // direction_id / billing_mode を正式な実行条件としてシステムへ流し込む。
    // Phase 1 では resolve の中身は現行挙動と同一（恒等マッピング）。
    // 解決した結果（言語ペア・範囲制約）は、以降の既存処理で実際に参照される。
    let execution_plan = ExecutionPlan::from_runtime(&job_plan);
    let direction_profile = execution_plan
        .resolve_direction()
        .map_err(EntryError::Internal)?;
    let plan_policy = execution_plan
        .resolve_plan()
        .map_err(EntryError::Internal)?;
    let (src_lang, dst_lang) = direction_profile.lang_pair();
    let cell_scope = plan_policy.cell_scope();
    println!(
        "[EXECUTION_PLAN][RESOLVED] direction={} lang_pair={:?}->{:?} plan={} cell_scope={:?}",
        direction_profile.id(),
        src_lang,
        dst_lang,
        plan_policy.id(),
        cell_scope
    );
    println!("[ETB_PHASE1_MARKER] generate path reached: direction={} plan={}", direction_profile.id(), plan_policy.id());

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

    let mut cfg = load_translator_config();

    if job_plan.is_enabled(1) {
        if let Some(provider) = job_plan.candidate1_provider {
            cfg.candidate1_provider = provider;
        }
    }
    if job_plan.is_enabled(2) {
        if let Some(provider) = job_plan.candidate2_provider {
            cfg.candidate2_provider = provider;
        }
    }
    if job_plan.is_enabled(3) {
        if let Some(provider) = job_plan.candidate3_provider {
            cfg.candidate3_provider = provider;
        }
    }

    let p1 = cfg.candidate1_provider;
    let p2 = cfg.candidate2_provider;
    let p3 = cfg.candidate3_provider;
    let adapter1 = create_adapter(p1, &cfg);
    let adapter2 = create_adapter(p2, &cfg);
    let adapter3 = create_adapter(p3, &cfg);

    let candidate_generation_plan = CandidateGenerationPlan {
        enabled_candidates: job_plan.enabled_candidates.clone(),
        default_candidate_priority: job_plan.default_candidate_priority.clone(),
        job_accept_threshold: job_plan.job_accept_threshold,
    };

    let old_input = std::env::var("ETB_INPUT_PATH").ok();
    let old_target = std::env::var("ETB_TARGET_SHEET").ok();
    std::env::set_var("ETB_INPUT_PATH", &replica_str);

    let mut rows: Vec<UiRow> = Vec::new();
    for sheet in &selected_sheets {
        println!("[ENTRY] processing sheet = {}", sheet);
        std::env::set_var("ETB_TARGET_SHEET", sheet);
        let mut logical_cells = read_source_logical_cells().map_err(|e| EntryError::Internal(format!("{:?}", e)))?;

        // Phase 1: 範囲制限を plan_policy.cell_scope() の値で実際に判定する。
        // free -> Range(A1:D5) のとき cell_scope.contains_address() で範囲内のみ残す（Phase 3 で plan へ集約）。
        // paid_standard -> Full のときフィルタ自体を行わない（現行 paid と同一）。
        if let CellScope::Range { .. } = cell_scope {
            let before = logical_cells.len();
            logical_cells.retain(|cell| cell_scope.contains_address(&cell.anchor_address));
            println!(
                "[EXPERIENCE] sheet={} range={} target_cells={} filtered_out={}",
                sheet,
                EXPERIENCE_RANGE_LABEL,
                logical_cells.len(),
                before.saturating_sub(logical_cells.len())
            );
        }

        let mut policies = Vec::with_capacity(logical_cells.len());
        // 第1パス: candidate1 未確定なのですべて Original で初期化し、
        // バンドル生成後に候補テキストを見て上書きする
        let mut default_selects = vec![DefaultSelect::Original; logical_cells.len()];

        for logical_cell in &logical_cells {
            let policy = decide_translation_policy(logical_cell.cell_kind, &logical_cell.source_text, direction_profile.as_ref());
            policies.push(policy);
        }

        let bundles = build_candidate_bundles_batch(
            &logical_cells,
            &policies,
            &default_selects,
            adapter1.as_ref(),
            adapter2.as_ref(),
            adapter3.as_ref(),
            cfg.batch_max_items,
            cfg.batch_max_chars,
            &candidate_generation_plan,
            src_lang,
            dst_lang,
            direction_profile.as_ref(),
        ).map_err(|e| EntryError::Internal(format!("{:?}", e)))?;

        if bundles.len() != logical_cells.len() {
            return Err(EntryError::Internal(format!(
                "candidate bundle count mismatch: cells={} bundles={}",
                logical_cells.len(),
                bundles.len()
            )));
        }

        // 第2パス: candidate1 テキストが確定したので DefaultSelect を上書きする
        for (idx, (logical_cell, bundle)) in logical_cells.iter().zip(bundles.iter()).enumerate() {
            let policy = &policies[idx];
            let candidate1_text = bundle.candidate1.as_deref();
            default_selects[idx] = decide_default_select(
                logical_cell.cell_kind,
                policy.translate_candidates,
                candidate1_text,
                &logical_cell.source_text,
                direction_profile.as_ref(),
            );
        }

        for (idx, (logical_cell, bundle)) in logical_cells.iter().zip(bundles.iter()).enumerate() {
            // bundle の default_select を上書きした値で UiRow を作る
            let mut ui_row = build_ui_row(logical_cell, bundle);
            ui_row.default_select = default_selects[idx] as u8;
            rows.push(ui_row);
        }
    }

    match old_target { Some(v) => std::env::set_var("ETB_TARGET_SHEET", v), None => std::env::remove_var("ETB_TARGET_SHEET") }
    match old_input { Some(v) => std::env::set_var("ETB_INPUT_PATH", v), None => std::env::remove_var("ETB_INPUT_PATH") }

    let candidate_configs = [
        job_plan.candidate_config(1),
        job_plan.candidate_config(2),
        job_plan.candidate_config(3),
    ];
    write_generate_workbook(
        &replica_str,
        &rows,
        &job_paths.output_ui_path.to_string_lossy(),
        &cfg,
        &security_report,
        &job_plan.enabled_candidates,
        &candidate_configs,
    ).map_err(|e| EntryError::Internal(format!("generate workbook write failed: {e}")))?;

    println!("=== ENTRY generate-select end ===");
    Ok(GenerateSelectResult {
        job_id: job_paths.job_id,
        output_ui_path: job_paths.output_ui_path,
        selected_sheets,
    })
}
