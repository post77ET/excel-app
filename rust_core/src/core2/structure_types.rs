#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalCellKind {
    Text,
    FormulaRaw,
    SharedFormulaParent,
    SharedFormulaFollower,
    Date,
    Number,
    HyperlinkText,
    Empty,
    NonTranslatable,
}

#[derive(Debug, Clone)]
pub struct LogicalCell {
    pub is_merged: bool,
    pub is_merge_anchor: bool,
    pub merge_anchor_address: Option<String>,
    pub writeback_allowed: bool,
    pub logical_cell_id: String,
    pub sheet_name: String,
    pub anchor_address: String,
    pub cell_kind: LogicalCellKind,
    pub source_text: String,
}