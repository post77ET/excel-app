use std::error::Error;
use std::io;

use crate::ui::types::UiRow;
use crate::ui::ui_sheet_reader::read_ui_rows;

#[derive(Debug)]
pub struct UiWorkbookState {
    pub ui_rows: Vec<UiRow>,
}

pub fn read_ui_workbook_state(path: &str) -> Result<UiWorkbookState, Box<dyn Error>> {
    let ui_rows =
        read_ui_rows(path).map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?;
    Ok(UiWorkbookState { ui_rows })
}
