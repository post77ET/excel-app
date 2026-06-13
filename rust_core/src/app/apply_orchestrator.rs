use std::path::{Path, PathBuf};

use crate::core2::apply_workbook_writer::write_apply_workbook;
use crate::core2::ui_workbook_reader::read_ui_workbook_state;
use crate::infra::app_error::AppError;
use crate::security::apply_guard::validate_apply_input_workbook;
use crate::security::pipeline::{inspect_xlsx, print_report};
use crate::security::types::SecurityResult;
use crate::ui::ui_apply_payload::build_apply_payload;

pub fn run_apply_pipeline() -> Result<(), AppError> {
    println!("=== ETb apply pipeline start ===");

    let ui_input_path =
        std::env::var("ETB_UI_INPUT").unwrap_or_else(|_| "TEST_work_ui.xlsx".to_string());

    let base_input_path = std::env::var("ETB_INPUT_PATH")
        .map_err(|_| AppError::Internal("ETB_INPUT_PATH base workbook path is required for apply".to_string()))?;

    let apply_output_path = std::env::var("ETB_APPLY_OUTPUT")
        .unwrap_or_else(|_| build_default_apply_output_path(&base_input_path));

    let security_report = inspect_xlsx(&ui_input_path);
    print_report(&security_report);
    if security_report.final_result == SecurityResult::Reject {
        return Err(AppError::Internal(format!(
            "security rejected ui workbook: {}",
            ui_input_path
        )));
    }

    validate_apply_input_workbook(&ui_input_path)
        .map_err(|e| AppError::Internal(format!("apply security validation failed: {e}")))?;

    let state = read_ui_workbook_state(&ui_input_path)
        .map_err(|e| AppError::Internal(format!("ui workbook reread failed: {e}")) )?;
    let apply_rows = build_apply_payload(&state.ui_rows);

    write_apply_workbook(&base_input_path, &ui_input_path, &apply_rows, &apply_output_path)
        .map_err(|e| AppError::Internal(format!("apply workbook write failed: {e}")))?;

    println!("apply: base input = {}", base_input_path);
    println!("apply: ui input = {}", ui_input_path);
    println!("apply: workbook written = {}", apply_output_path);
    println!("=== ETb apply pipeline end ===");
    Ok(())
}

fn build_default_apply_output_path(base_input_path: &str) -> String {
    // フォールバック: ETB_APPLY_OUTPUT が未設定の場合のみ使用される。
    // base_input_path (原本) のstemを使ってApply出力名を決定する。
    let path = Path::new(base_input_path);
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("output");
    let output_name = format!("{}_apply.xlsx", stem);
    let output_path: PathBuf = parent.join(output_name);
    output_path.to_string_lossy().to_string()
}
