use crate::core1::types::CandidateBundle;
use crate::core2::structure_types::LogicalCell;
use crate::infra::config_loader::TranslatorConfig;
use crate::ui::types::UiRow;
use crate::ui::ui_sheet_builder::{build_ui_row as build_ui_row_new, write_ui_sheet_into_book};
use crate::ui::ui_sheet_reader::read_ui_rows;

pub fn build_ui_row(logical_cell: &LogicalCell, bundle: &CandidateBundle) -> UiRow {
    build_ui_row_new(logical_cell, bundle)
}

pub fn write_ui_workbook(
    rows: &[UiRow],
    output_path: &str,
    config: &TranslatorConfig,
) -> Result<(), String> {
    let mut book = umya_spreadsheet::new_file();
    if book.sheet_by_name("Sheet1").is_ok() {
        let _ = book.remove_sheet_by_name("Sheet1");
    }
    write_ui_sheet_into_book(&mut book, rows, config, &[1, 2, 3])?;
    umya_spreadsheet::writer::xlsx::write(&book, output_path).map_err(|e| e.to_string())
}

pub fn read_ui_workbook(input_path: &str) -> Result<Vec<UiRow>, String> {
    read_ui_rows(input_path)
}
