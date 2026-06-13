use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

#[derive(Debug, Clone)]
pub struct SharedFormulaOverride {
    pub sheet_name: String,
    pub anchor_address: String,
    pub formula_body: String,
}

#[derive(Debug, Clone)]
struct SheetInfo {
    name: String,
    path: String,
}

#[derive(Debug, Clone)]
struct SharedFormulaTemplate {
    master_cell: String,
    ref_range: String,
    si: String,
}

#[derive(Debug, Clone)]
struct FormulaPatchAction {
    si: String,
    ref_range: Option<String>,
    formula_body: String,
}

pub fn load_shared_formula_parent_lookup(source_workbook_path: &str) -> Result<HashSet<String>, String> {
    let mut zip = open_zip(source_workbook_path)?;
    let workbook_xml = read_zip_string(&mut zip, "xl/workbook.xml")?;
    let workbook_rels_xml = read_zip_string(&mut zip, "xl/_rels/workbook.xml.rels")?;
    let sheets = resolve_sheet_infos(&workbook_xml, &workbook_rels_xml)?;

    let mut out = HashSet::new();

    for sheet in sheets {
        let xml = read_zip_string(&mut zip, &sheet.path)?;
        for master_cell in extract_shared_formula_master_cells(&xml)? {
            out.insert(format!("{}!{}", sheet.name, master_cell));
        }
    }

    Ok(out)
}

pub fn patch_apply_shared_formula_groups(
    source_workbook_path: &str,
    output_workbook_path: &str,
    overrides: &[SharedFormulaOverride],
) -> Result<(), String> {
    if overrides.is_empty() {
        return Ok(());
    }

    let mut source_zip = open_zip(source_workbook_path)?;
    let source_workbook_xml = read_zip_string(&mut source_zip, "xl/workbook.xml")?;
    let source_workbook_rels_xml = read_zip_string(&mut source_zip, "xl/_rels/workbook.xml.rels")?;
    let source_sheets = resolve_sheet_infos(&source_workbook_xml, &source_workbook_rels_xml)?;

    let mut sheet_path_by_name: HashMap<String, String> = HashMap::new();
    for sheet in source_sheets {
        sheet_path_by_name.insert(sheet.name, sheet.path);
    }

    let mut patches_by_sheet_path: HashMap<String, Vec<SharedFormulaTemplateWithBody>> = HashMap::new();

    for ov in overrides {
        let sheet_path = sheet_path_by_name
            .get(&ov.sheet_name)
            .ok_or_else(|| format!("shared formula source sheet not found: {}", ov.sheet_name))?
            .clone();

        let source_sheet_xml = read_zip_string(&mut source_zip, &sheet_path)?;
        let template = extract_shared_formula_template_for_master(&source_sheet_xml, &ov.anchor_address)?
            .ok_or_else(|| {
                format!(
                    "shared formula template not found: sheet={} cell={}",
                    ov.sheet_name, ov.anchor_address
                )
            })?;

        patches_by_sheet_path
            .entry(sheet_path)
            .or_default()
            .push(SharedFormulaTemplateWithBody {
                master_cell: template.master_cell,
                ref_range: template.ref_range,
                si: template.si,
                formula_body: ov.formula_body.clone(),
            });
    }

    rewrite_output_with_shared_formula_patches(output_workbook_path, &patches_by_sheet_path)
}

#[derive(Debug, Clone)]
struct SharedFormulaTemplateWithBody {
    master_cell: String,
    ref_range: String,
    si: String,
    formula_body: String,
}

fn rewrite_output_with_shared_formula_patches(
    output_workbook_path: &str,
    patches_by_sheet_path: &HashMap<String, Vec<SharedFormulaTemplateWithBody>>,
) -> Result<(), String> {
    let source_path = Path::new(output_workbook_path);
    let source_file = File::open(source_path)
        .map_err(|e| format!("output xlsx open failed: {e}"))?;
    let mut archive =
        ZipArchive::new(source_file).map_err(|e| format!("output zip parse failed: {e}"))?;

    let temp_path = build_temp_xlsx_path(source_path);
    let temp_file =
        File::create(&temp_path).map_err(|e| format!("temp xlsx create failed: {e}"))?;
    let mut writer = ZipWriter::new(temp_file);

    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|e| format!("zip entry open failed: {e}"))?;

        let entry_name = entry.name().to_string();
        let options = SimpleFileOptions::default().compression_method(entry.compression());

        if entry.is_dir() {
            writer
                .add_directory(entry_name, options)
                .map_err(|e| format!("zip add dir failed: {e}"))?;
            continue;
        }

        let mut data = Vec::new();
        entry.read_to_end(&mut data)
            .map_err(|e| format!("zip entry read failed: {e}"))?;

        writer
            .start_file(entry_name.as_str(), options)
            .map_err(|e| format!("zip start file failed: {e}"))?;

        if let Some(sheet_patches) = patches_by_sheet_path.get(&entry_name) {
            let xml = String::from_utf8(data)
                .map_err(|e| format!("sheet xml utf8 decode failed: {e}"))?;
            let patched = patch_sheet_xml_shared_groups(&xml, sheet_patches)?;
            writer
                .write_all(patched.as_bytes())
                .map_err(|e| format!("zip patched sheet write failed: {e}"))?;
        } else {
            writer
                .write_all(&data)
                .map_err(|e| format!("zip passthrough write failed: {e}"))?;
        }
    }

    writer
        .finish()
        .map_err(|e| format!("zip finalize failed: {e}"))?;

    fs::remove_file(source_path)
        .map_err(|e| format!("original xlsx remove failed: {e}"))?;
    fs::rename(&temp_path, source_path)
        .map_err(|e| format!("patched xlsx replace failed: {e}"))?;

    Ok(())
}

fn patch_sheet_xml_shared_groups(
    sheet_xml: &str,
    patches: &[SharedFormulaTemplateWithBody],
) -> Result<String, String> {
    let mut actions: HashMap<String, FormulaPatchAction> = HashMap::new();

    for patch in patches {
        actions.insert(
            patch.master_cell.clone(),
            FormulaPatchAction {
                si: patch.si.clone(),
                ref_range: Some(patch.ref_range.clone()),
                formula_body: patch.formula_body.clone(),
            },
        );

        let cells = expand_range(&patch.ref_range)?;
        for cell in cells {
            if cell == patch.master_cell {
                continue;
            }
            actions.insert(
                cell,
                FormulaPatchAction {
                    si: patch.si.clone(),
                    ref_range: None,
                    formula_body: String::new(),
                },
            );
        }
    }

    let mut out = String::with_capacity(sheet_xml.len() + 512);
    let mut rest = sheet_xml;

    while let Some(c_pos) = rest.find("<c ") {
        out.push_str(&rest[..c_pos]);
        let after = &rest[c_pos..];

        let close_rel = after
            .find('>')
            .ok_or_else(|| "cell start tag malformed".to_string())?;
        let start_tag = &after[..close_rel + 1];
        let self_closing = start_tag.trim_end().ends_with("/>");

        let cell_ref = extract_attr(start_tag, "r").unwrap_or_default();

        if self_closing {
            out.push_str(start_tag);
            rest = &after[close_rel + 1..];
            continue;
        }

        let tail = &after[close_rel + 1..];
        let end_rel = tail
            .find("</c>")
            .ok_or_else(|| format!("cell close tag missing: {cell_ref}"))?;
        let end_idx = close_rel + 1 + end_rel + "</c>".len();
        let cell_block = &after[..end_idx];

        if let Some(action) = actions.get(&cell_ref) {
            let patched_block = patch_cell_formula_block(cell_block, action)?;
            out.push_str(&patched_block);
        } else {
            out.push_str(cell_block);
        }

        rest = &after[end_idx..];
    }

    out.push_str(rest);
    Ok(out)
}

fn patch_cell_formula_block(cell_block: &str, action: &FormulaPatchAction) -> Result<String, String> {
    let new_formula_xml = match &action.ref_range {
        Some(ref_range) => format!(
            "<f t=\"shared\" ref=\"{}\" si=\"{}\">{}</f>",
            xml_escape_attr(ref_range),
            xml_escape_attr(&action.si),
            xml_escape_text(&action.formula_body),
        ),
        None => format!(
            "<f t=\"shared\" si=\"{}\"></f>",
            xml_escape_attr(&action.si),
        ),
    };

    if let Some(f_start_rel) = cell_block.find("<f") {
        let after_f = &cell_block[f_start_rel..];
        let open_end_rel = after_f
            .find('>')
            .ok_or_else(|| "formula start tag malformed".to_string())?;
        let open_tag = &after_f[..open_end_rel + 1];

        if open_tag.trim_end().ends_with("/>") {
            let mut out = String::with_capacity(cell_block.len() + new_formula_xml.len());
            out.push_str(&cell_block[..f_start_rel]);
            out.push_str(&new_formula_xml);
            out.push_str(&cell_block[f_start_rel + open_tag.len()..]);
            return Ok(out);
        }

        let after_open = &cell_block[f_start_rel + open_tag.len()..];
        let f_close_rel = after_open
            .find("</f>")
            .ok_or_else(|| "formula close tag missing".to_string())?;
        let old_formula_end = f_start_rel + open_tag.len() + f_close_rel + "</f>".len();

        let mut out = String::with_capacity(cell_block.len() + new_formula_xml.len());
        out.push_str(&cell_block[..f_start_rel]);
        out.push_str(&new_formula_xml);
        out.push_str(&cell_block[old_formula_end..]);
        return Ok(out);
    }

    if let Some(v_pos) = cell_block.find("<v") {
        let mut out = String::with_capacity(cell_block.len() + new_formula_xml.len());
        out.push_str(&cell_block[..v_pos]);
        out.push_str(&new_formula_xml);
        out.push_str(&cell_block[v_pos..]);
        return Ok(out);
    }

    let insert_pos = cell_block
        .rfind("</c>")
        .ok_or_else(|| "cell close tag missing".to_string())?;
    let mut out = String::with_capacity(cell_block.len() + new_formula_xml.len());
    out.push_str(&cell_block[..insert_pos]);
    out.push_str(&new_formula_xml);
    out.push_str(&cell_block[insert_pos..]);
    Ok(out)
}

fn open_zip(path: &str) -> Result<ZipArchive<File>, String> {
    let file = File::open(path).map_err(|e| format!("xlsx open failed: {e}"))?;
    ZipArchive::new(file).map_err(|e| format!("xlsx zip parse failed: {e}"))
}

fn read_zip_string(zip: &mut ZipArchive<File>, name: &str) -> Result<String, String> {
    let mut entry = zip
        .by_name(name)
        .map_err(|e| format!("zip entry open failed {name}: {e}"))?;
    let mut buf = String::new();
    entry
        .read_to_string(&mut buf)
        .map_err(|e| format!("zip entry read failed {name}: {e}"))?;
    Ok(buf)
}

fn resolve_sheet_infos(workbook_xml: &str, workbook_rels_xml: &str) -> Result<Vec<SheetInfo>, String> {
    let mut out = Vec::new();
    let mut rest = workbook_xml;

    while let Some(pos) = rest.find("<sheet ") {
        let after = &rest[pos..];
        let end_rel = after
            .find("/>")
            .ok_or_else(|| "sheet tag malformed".to_string())?;
        let tag = &after[..end_rel + 2];

        let name = match extract_attr(tag, "name") {
            Some(v) => v,
            None => {
                rest = &after[end_rel + 2..];
                continue;
            }
        };
        let rid = extract_attr(tag, "r:id")
            .ok_or_else(|| format!("sheet rid missing: {name}"))?;
        let target = extract_relationship_target(workbook_rels_xml, &rid)?
            .ok_or_else(|| format!("sheet target missing: {name}"))?;

        out.push(SheetInfo {
            name,
            path: normalize_xl_path(&target),
        });

        rest = &after[end_rel + 2..];
    }

    Ok(out)
}

fn extract_relationship_target(workbook_rels_xml: &str, rid: &str) -> Result<Option<String>, String> {
    let marker = format!("Id=\"{}\"", rid);
    let start = match workbook_rels_xml.find(&marker) {
        Some(v) => v,
        None => return Ok(None),
    };

    let tail = &workbook_rels_xml[start..];
    let target_key = "Target=\"";
    let target_start_rel = tail
        .find(target_key)
        .ok_or_else(|| format!("Target not found for {rid}"))?;
    let target_value_start = start + target_start_rel + target_key.len();
    let target_tail = &workbook_rels_xml[target_value_start..];
    let target_end_rel = target_tail
        .find('"')
        .ok_or_else(|| format!("Target close quote missing for {rid}"))?;
    Ok(Some(
        workbook_rels_xml[target_value_start..target_value_start + target_end_rel].to_string(),
    ))
}

fn normalize_xl_path(target: &str) -> String {
    let cleaned = target.trim_start_matches('/');
    if cleaned.starts_with("xl/") {
        cleaned.to_string()
    } else {
        format!("xl/{}", cleaned)
    }
}

fn extract_shared_formula_master_cells(sheet_xml: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut rest = sheet_xml;

    while let Some(c_pos) = rest.find("<c ") {
        let after = &rest[c_pos..];
        let close_rel = after
            .find('>')
            .ok_or_else(|| "cell tag malformed".to_string())?;
        let start_tag = &after[..close_rel + 1];
        let self_closing = start_tag.trim_end().ends_with("/>");
        let cell_ref = extract_attr(start_tag, "r").unwrap_or_default();

        if self_closing {
            rest = &after[close_rel + 1..];
            continue;
        }

        let tail = &after[close_rel + 1..];
        let end_rel = tail
            .find("</c>")
            .ok_or_else(|| format!("cell close missing: {cell_ref}"))?;
        let cell_block = &after[..close_rel + 1 + end_rel + "</c>".len()];

        if let Some(f_start) = cell_block.find("<f") {
            let after_f = &cell_block[f_start..];
            let f_tag_end = after_f
                .find('>')
                .ok_or_else(|| "formula tag malformed".to_string())?;
            let f_open_tag = &after_f[..f_tag_end + 1];

            if f_open_tag.contains("t=\"shared\"") && f_open_tag.contains("ref=\"") {
                out.push(cell_ref);
            }
        }

        rest = &after[close_rel + 1 + end_rel + "</c>".len()..];
    }

    Ok(out)
}

fn extract_shared_formula_template_for_master(
    sheet_xml: &str,
    master_cell: &str,
) -> Result<Option<SharedFormulaTemplate>, String> {
    let mut rest = sheet_xml;

    while let Some(c_pos) = rest.find("<c ") {
        let after = &rest[c_pos..];
        let close_rel = after
            .find('>')
            .ok_or_else(|| "cell tag malformed".to_string())?;
        let start_tag = &after[..close_rel + 1];
        let self_closing = start_tag.trim_end().ends_with("/>");
        let cell_ref = extract_attr(start_tag, "r").unwrap_or_default();

        if self_closing {
            rest = &after[close_rel + 1..];
            continue;
        }

        let tail = &after[close_rel + 1..];
        let end_rel = tail
            .find("</c>")
            .ok_or_else(|| format!("cell close missing: {cell_ref}"))?;
        let cell_block = &after[..close_rel + 1 + end_rel + "</c>".len()];

        if cell_ref == master_cell {
            if let Some(f_start) = cell_block.find("<f") {
                let after_f = &cell_block[f_start..];
                let f_tag_end = after_f
                    .find('>')
                    .ok_or_else(|| "formula tag malformed".to_string())?;
                let f_open_tag = &after_f[..f_tag_end + 1];

                if f_open_tag.contains("t=\"shared\"") && f_open_tag.contains("ref=\"") {
                    let ref_range = extract_attr(f_open_tag, "ref")
                        .ok_or_else(|| format!("shared ref missing: {master_cell}"))?;
                    let si = extract_attr(f_open_tag, "si")
                        .ok_or_else(|| format!("shared si missing: {master_cell}"))?;

                    return Ok(Some(SharedFormulaTemplate {
                        master_cell: master_cell.to_string(),
                        ref_range,
                        si,
                    }));
                }
            }
            return Ok(None);
        }

        rest = &after[close_rel + 1 + end_rel + "</c>".len()..];
    }

    Ok(None)
}

fn extract_attr(tag: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn expand_range(ref_range: &str) -> Result<Vec<String>, String> {
    let parts: Vec<&str> = ref_range.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("invalid shared ref range: {ref_range}"));
    }

    let (start_col, start_row) = split_cell_ref(parts[0])?;
    let (end_col, end_row) = split_cell_ref(parts[1])?;

    let start_col_num = col_letters_to_number(&start_col)?;
    let end_col_num = col_letters_to_number(&end_col)?;

    let mut out = Vec::new();
    for row in start_row..=end_row {
        for col in start_col_num..=end_col_num {
            out.push(format!("{}{}", number_to_col_letters(col), row));
        }
    }
    Ok(out)
}

fn split_cell_ref(cell_ref: &str) -> Result<(String, u32), String> {
    let mut col = String::new();
    let mut row = String::new();

    for ch in cell_ref.chars() {
        if ch.is_ascii_alphabetic() {
            if !row.is_empty() {
                return Err(format!("invalid cell ref: {cell_ref}"));
            }
            col.push(ch.to_ascii_uppercase());
        } else if ch.is_ascii_digit() {
            row.push(ch);
        } else {
            return Err(format!("invalid cell ref: {cell_ref}"));
        }
    }

    if col.is_empty() || row.is_empty() {
        return Err(format!("invalid cell ref: {cell_ref}"));
    }

    let row_num = row
        .parse::<u32>()
        .map_err(|_| format!("invalid row in cell ref: {cell_ref}"))?;

    Ok((col, row_num))
}

fn col_letters_to_number(col: &str) -> Result<u32, String> {
    let mut n = 0u32;
    for ch in col.chars() {
        if !ch.is_ascii_alphabetic() {
            return Err(format!("invalid column letters: {col}"));
        }
        n = n * 26 + ((ch as u8 - b'A' + 1) as u32);
    }
    Ok(n)
}

fn number_to_col_letters(mut n: u32) -> String {
    let mut out = String::new();
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        out.insert(0, (b'A' + rem) as char);
        n = (n - 1) / 26;
    }
    out
}

fn xml_escape_text(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_escape_attr(input: &str) -> String {
    xml_escape_text(input).replace('"', "&quot;")
}

fn build_temp_xlsx_path(source_path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);

    let parent = source_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    parent.join(format!(".__etb_shared_patch_{nanos}.xlsx"))
}