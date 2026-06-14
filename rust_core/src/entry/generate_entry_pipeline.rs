use std::path::Path;

use crate::adapters::provider_factory::create_adapter;
use crate::core1::analyzer::{build_candidate_bundles_batch, CandidateGenerationPlan};
use crate::core1::default_select::decide_default_select;
use crate::core1::text_structure_analyzer::analyze_text_structure;
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
use crate::entry::job_plan_settings::{load_job_plan_settings, EXPERIENCE_MAX_COL, EXPERIENCE_MAX_ROW, EXPERIENCE_RANGE_LABEL};
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

        if job_plan.is_experience() {
            let before = logical_cells.len();
            logical_cells.retain(|cell| is_in_experience_range(&cell.anchor_address));
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
            let structure = analyze_text_structure(&logical_cell.source_text);
            let policy = decide_translation_policy(logical_cell.cell_kind, &structure);
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
            let structure = analyze_text_structure(&logical_cell.source_text);
            let policy = &policies[idx];
            let candidate1_text = bundle.candidate1.as_deref();
            default_selects[idx] = decide_default_select(
                logical_cell.cell_kind,
                &structure,
                policy.translate_candidates,
                candidate1_text,
                &logical_cell.source_text,
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

    write_generate_workbook(
        &replica_str,
        &rows,
        &job_paths.output_ui_path.to_string_lossy(),
        &cfg,
        &security_report,
        &job_plan.enabled_candidates,
    ).map_err(|e| EntryError::Internal(format!("generate workbook write failed: {e}")))?;

    println!("=== ENTRY generate-select end ===");
    Ok(GenerateSelectResult {
        job_id: job_paths.job_id,
        output_ui_path: job_paths.output_ui_path,
        selected_sheets,
    })
}
fn is_in_experience_range(address: &str) -> bool {
    match split_cell_address(address) {
        Some((col, row)) => row >= 1 && row <= EXPERIENCE_MAX_ROW && col >= 1 && col <= EXPERIENCE_MAX_COL,
        None => false,
    }
}

fn split_cell_address(address: &str) -> Option<(u32, u32)> {
    let mut col: u32 = 0;
    let mut row_text = String::new();
    let mut seen_digit = false;

    for ch in address.chars() {
        if ch.is_ascii_alphabetic() && !seen_digit {
            col = col * 26 + ((ch.to_ascii_uppercase() as u8 - b'A' + 1) as u32);
        } else if ch.is_ascii_digit() {
            seen_digit = true;
            row_text.push(ch);
        } else {
            return None;
        }
    }

    if col == 0 || row_text.is_empty() {
        return None;
    }

    let row = row_text.parse::<u32>().ok()?;
    Some((col, row))
}
