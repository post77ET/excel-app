use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::infra::config_loader::TranslatorConfig;
use crate::ui::types::UiRow;
use umya_spreadsheet::{Spreadsheet, Worksheet};

pub const INTERNAL_SHEET_NAME: &str = "__ETB_INTERNAL";
pub const INTERNAL_APP_ID: &str = "CORE1_ETB_UI";
pub const INTERNAL_VERSION: &str = "v1";

#[derive(Debug, Clone)]
pub struct InternalMetadata {
    pub app_id: String,
    pub version: String,
    pub ui_sheet_name: String,
    pub candidate1_header: String,
    pub candidate2_header: String,
    pub candidate3_header: String,
    pub row_count: usize,
    pub immutable_hash: String,
}

impl InternalMetadata {
    pub fn from_rows(rows: &[UiRow], config: &TranslatorConfig) -> Self {
        let candidate1_header = format!("candidate1 = {}", config.candidate1_provider.as_label());
        let candidate2_header = if rows.iter().any(|r| r.candidate2.is_some()) {
            format!("candidate2 = {}", config.candidate2_provider.as_label())
        } else {
            "candidate2 = None".to_string()
        };
        let candidate3_header = if rows.iter().any(|r| r.candidate3.is_some()) {
            format!("candidate3 = {}", config.candidate3_provider.as_label())
        } else {
            "candidate3 = None".to_string()
        };
        let immutable_hash =
            compute_immutable_hash(rows, &candidate1_header, &candidate2_header, &candidate3_header);

        Self {
            app_id: INTERNAL_APP_ID.to_string(),
            version: INTERNAL_VERSION.to_string(),
            ui_sheet_name: "TRANSLATION_UI".to_string(),
            candidate1_header,
            candidate2_header,
            candidate3_header,
            row_count: rows.len(),
            immutable_hash,
        }
    }
}

pub fn write_internal_metadata_sheet_into_book(
    book: &mut Spreadsheet,
    metadata: &InternalMetadata,
) -> Result<(), String> {
    if book.get_sheet_by_name(INTERNAL_SHEET_NAME).is_some() {
        let _ = book.remove_sheet_by_name(INTERNAL_SHEET_NAME);
    }

    let _ = book.new_sheet(INTERNAL_SHEET_NAME);
    let sheet = book
        .get_sheet_by_name_mut(INTERNAL_SHEET_NAME)
        .ok_or_else(|| "internal metadata sheet create error".to_string())?;

    let pairs = [
        ("app_id", metadata.app_id.as_str()),
        ("version", metadata.version.as_str()),
        ("ui_sheet_name", metadata.ui_sheet_name.as_str()),
        ("candidate1_header", metadata.candidate1_header.as_str()),
        ("candidate2_header", metadata.candidate2_header.as_str()),
        ("candidate3_header", metadata.candidate3_header.as_str()),
        ("row_count", &metadata.row_count.to_string()),
        ("immutable_hash", metadata.immutable_hash.as_str()),
    ];

    for (idx, (key, value)) in pairs.iter().enumerate() {
        let row = idx + 1;
        sheet.get_cell_mut(format!("A{}", row)).set_value(*key);
        sheet.get_cell_mut(format!("B{}", row)).set_value(*value);
    }

    hide_sheet(sheet);
    Ok(())
}

fn hide_sheet(sheet: &mut Worksheet) {
    // シートを非表示にする（UIには表示しない内部メタデータシート）
    sheet.set_sheet_state("hidden".to_string());
}

fn normalize_hash_text(v: &str) -> String {
    v.replace("\r\n", "\n")
}

fn hash_text(value: &str, hasher: &mut DefaultHasher) {
    normalize_hash_text(value).hash(hasher);
}

fn hash_opt_text(value: &Option<String>, hasher: &mut DefaultHasher) {
    value
        .as_ref()
        .map(|v| normalize_hash_text(v))
        .hash(hasher);
}

pub fn compute_immutable_hash(
    rows: &[UiRow],
    candidate1_header: &str,
    candidate2_header: &str,
    candidate3_header: &str,
) -> String {
    let mut hasher = DefaultHasher::new();

    "TRANSLATION_UI".hash(&mut hasher);
    candidate1_header.hash(&mut hasher);
    candidate2_header.hash(&mut hasher);
    candidate3_header.hash(&mut hasher);
    rows.len().hash(&mut hasher);

    for row in rows {
        row.logical_cell_id.hash(&mut hasher);
        row.sheet_name.hash(&mut hasher);
        row.anchor_address.hash(&mut hasher);
        row.cell_kind.hash(&mut hasher);

        hash_text(&row.original, &mut hasher);
        hash_text(&row.original_writeback, &mut hasher);

        row.writeback_mode.hash(&mut hasher);

        hash_opt_text(&row.candidate1, &mut hasher);
        hash_opt_text(&row.candidate2, &mut hasher);
        hash_opt_text(&row.candidate3, &mut hasher);

        row.default_select.hash(&mut hasher);

        hash_opt_text(&row.alarms.candidate1_alarm, &mut hasher);
        hash_opt_text(&row.alarms.candidate2_alarm, &mut hasher);
        hash_opt_text(&row.alarms.candidate3_alarm, &mut hasher);

        hash_text(&row.note, &mut hasher);
    }

    format!("{:016x}", hasher.finish())
}