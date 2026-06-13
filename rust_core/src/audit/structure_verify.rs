use std::path::Path;

use crate::ui::ui_apply_payload::ApplyPayloadRow;

pub fn verify_apply_output(
    _source_path: &str,
    output_path: &str,
    rows: &[ApplyPayloadRow],
) -> Result<(), String> {
    let output = umya_spreadsheet::reader::xlsx::read(Path::new(output_path))
        .map_err(|e| format!("audit output read failed: {e}"))?;

    for row in rows {
        let output_sheet = output
            .get_sheet_by_name(&row.sheet_name)
            .ok_or_else(|| format!("audit output missing sheet: {}", row.sheet_name))?;

        let output_value = output_sheet.get_value(row.anchor_address.as_str());
        if output_value != row.selected_text {
            return Err(format!(
                "audit value mismatch at {}!{} expected={} actual={}",
                row.sheet_name, row.anchor_address, row.selected_text, output_value
            ));
        }
    }

    Ok(())
}
