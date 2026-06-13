use crate::entry::entry_state::EntryError;
use crate::entry::generate_entry_pipeline::run_generate_select_pipeline;
use crate::infra::app_error::AppError;

pub fn run_generate_pipeline() -> Result<String, AppError> {
    println!("=== ETb generate pipeline start ===");

    let source_workbook_path = std::env::var("ETB_INPUT_PATH")
        .unwrap_or_else(|_| "TEST_work.xlsx".to_string());

    // 正式generateは、入口だけPowerShell/Webで異なっても、CORE本体は必ず同じ経路を通す。
    // 順序は SECURITY → sheet inventory → sheet selection → SOURCE_READER → CORE1 → UI生成。
    let result = run_generate_select_pipeline(&source_workbook_path).map_err(convert_entry_error)?;

    println!("generate: workbook written = {}", result.output_ui_path.display());
    println!("=== ETb generate pipeline end ===");
    Ok(result.output_ui_path.to_string_lossy().to_string())
}

fn convert_entry_error(error: EntryError) -> AppError {
    match error {
        EntryError::UserExit => AppError::Internal("user exited generate pipeline".to_string()),
        EntryError::UserBack => AppError::Internal("unexpected user back outside sheet selection loop".to_string()),
        EntryError::Internal(message) => AppError::Internal(message),
    }
}
