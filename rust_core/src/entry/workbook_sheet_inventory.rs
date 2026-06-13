use std::path::Path;

use calamine::{open_workbook_auto, Reader};

use crate::entry::entry_state::SheetInventory;
use crate::infra::app_error::AppError;

pub fn load_sheet_inventory(workbook_path: &Path) -> Result<SheetInventory, AppError> {
    let workbook = open_workbook_auto(workbook_path)
        .map_err(|e| AppError::WorkbookReadFailed(format!("open workbook failed: {e}")))?;
    let sheets = workbook.sheet_names().to_vec();
    if sheets.is_empty() {
        return Err(AppError::WorkbookReadFailed("no sheets found".to_string()));
    }
    Ok(SheetInventory { sheets })
}
