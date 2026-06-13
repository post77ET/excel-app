use crate::core2::source_workbook_reader::read_source_logical_cells;
use crate::core2::structure_types::LogicalCell;
use crate::infra::app_error::AppError;

pub fn read_range_logical_cells() -> Result<Vec<LogicalCell>, AppError> {
    read_source_logical_cells()
}

pub fn read_first_logical_cell() -> Result<LogicalCell, AppError> {
    read_source_logical_cells()?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::WorkbookReadFailed("no logical cells found".to_string()))
}
