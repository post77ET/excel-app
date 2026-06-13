use crate::core1::types::CandidateAlarms;

#[derive(Debug, Clone)]
pub struct UiRow {
    pub writeback_allowed: bool,
    pub logical_cell_id: String,
    pub sheet_name: String,
    pub anchor_address: String,
    pub cell_kind: String,
    pub original: String,
    pub original_writeback: String,
    pub writeback_mode: String,
    pub candidate1: Option<String>,
    pub candidate2: Option<String>,
    pub candidate3: Option<String>,
    pub default_select: u8,
    pub user_select: Option<u8>,
    pub apply_flag: bool,
    pub candidate4: Option<String>,
    pub alarms: CandidateAlarms,
    pub note: String,
}
