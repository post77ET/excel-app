use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct JobPaths {
    pub job_id: String,
    pub original_path: PathBuf,
    pub replica_path: PathBuf,
    pub output_ui_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SheetInventory {
    pub sheets: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GenerateSelectResult {
    pub job_id: String,
    pub output_ui_path: PathBuf,
    pub selected_sheets: Vec<String>,
}
#[derive(Debug)]
pub enum EntryError {
    UserExit,
    UserBack,
    Internal(String),
}