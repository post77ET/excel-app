use crate::core2::apply_workbook_writer::write_apply_workbook;
use crate::ui::ui_apply_payload::ApplyPayloadRow;

pub fn write_applied_workbook(
    source_path: &str,
    rows: &[ApplyPayloadRow],
    output_path: &str,
) -> Result<(), String> {
    write_apply_workbook(source_path, source_path, rows, output_path)
}
