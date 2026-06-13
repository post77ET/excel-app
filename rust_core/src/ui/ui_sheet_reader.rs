use crate::core1::types::CandidateAlarms;
use crate::ui::types::UiRow;

use calamine::{open_workbook_auto, Data, Range, Reader};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use zip::ZipArchive;

pub fn read_ui_rows(input_path: &str) -> Result<Vec<UiRow>, String> {
    let mut workbook =
        open_workbook_auto(input_path).map_err(|e| format!("ui workbook open failed: {e}"))?;

    let range = workbook
        .worksheet_range("TRANSLATION_UI")
        .map_err(|e| format!("ui sheet read failed: {e}"))?;

    let candidate4_map = load_ui_candidate4_map(input_path)?;

    let mut rows = Vec::new();

    for row_idx in 1..range.height() {
        let excel_row = row_idx + 1;
        let sheet_name_val = cell_string(&range, row_idx, 0);
        let anchor_address = cell_string(&range, row_idx, 1);
        if sheet_name_val.trim().is_empty() || anchor_address.trim().is_empty() {
            continue;
        }

        let user_select_raw = cell_string(&range, row_idx, 10);
        let apply_raw = cell_string(&range, row_idx, 11);
        let user_select = parse_user_select(&user_select_raw, excel_row)?;
        let apply_flag = parse_apply_flag(&apply_raw, excel_row)?;

        // R列 (index 17): WritebackAllowed
        // "0" のみ false、それ以外（"1" または空=旧UIとの互換）は true
        let writeback_allowed_raw = cell_string(&range, row_idx, 17);
        let writeback_allowed = writeback_allowed_raw.trim() != "0";

        let candidate4 = opt_string(candidate4_map.get(&excel_row).cloned().unwrap_or_default());

        rows.push(UiRow {
            writeback_allowed,
            logical_cell_id: format!("{}!{}", sheet_name_val, anchor_address),
            sheet_name: sheet_name_val,
            anchor_address,
            cell_kind: cell_string(&range, row_idx, 2),
            original: cell_string(&range, row_idx, 3),
            original_writeback: cell_string(&range, row_idx, 4),
            writeback_mode: cell_string(&range, row_idx, 5),
            candidate1: opt_string(cell_string(&range, row_idx, 6)),
            candidate2: opt_string(cell_string(&range, row_idx, 7)),
            candidate3: opt_string(cell_string(&range, row_idx, 8)),
            default_select: opt_u8(cell_string(&range, row_idx, 9)).unwrap_or(0),
            user_select,
            apply_flag,
            candidate4,
            alarms: CandidateAlarms {
                candidate1_alarm: opt_string(cell_string(&range, row_idx, 13)),
                candidate2_alarm: opt_string(cell_string(&range, row_idx, 14)),
                candidate3_alarm: opt_string(cell_string(&range, row_idx, 15)),
            },
            note: cell_string(&range, row_idx, 16),
        });
    }

    Ok(rows)
}

fn cell_string(range: &Range<Data>, row: usize, col: usize) -> String {
    match range.get((row, col)) {
        Some(Data::String(s)) => s.clone(),
        Some(Data::Float(v)) => v.to_string(),
        Some(Data::Int(v)) => v.to_string(),
        Some(Data::Bool(v)) => v.to_string(),
        Some(Data::DateTime(v)) => v.to_string(),
        Some(Data::DateTimeIso(s)) => s.clone(),
        Some(Data::DurationIso(s)) => s.clone(),
        Some(Data::Error(e)) => format!("{:?}", e),
        _ => String::new(),
    }
}

fn opt_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn opt_u8(value: String) -> Option<u8> {
    let trimmed = trim_excel_input(&value);
    if trimmed.is_empty() {
        None
    } else {
        trimmed.parse::<u8>().ok()
    }
}

fn trim_excel_input(value: &str) -> String {
    value
        .replace('\u{3000}', "") // 全角スペース除去
        .trim()
        .chars()
        .map(|c| {
            let cp = c as u32;
            if (0xFF10..=0xFF19).contains(&cp) {
                char::from_u32(cp - 0xFF10 + 0x0030).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

fn parse_user_select(value: &str, excel_row: usize) -> Result<Option<u8>, String> {
    let trimmed = trim_excel_input(value);
    if trimmed.is_empty() {
        return Ok(None);
    }
    match trimmed.as_str() {
        "0" | "1" | "2" | "3" | "4" => trimmed.parse::<u8>().ok().map(Some).ok_or_else(|| {
            format!("TRANSLATION_UI row {} col K(UserSelect) invalid value: '{}'", excel_row, value)
        }),
        _ => {
            // 手入力による不正値はエラーにせずNone（DefaultSelect使用）として扱う
            println!("[WARN] TRANSLATION_UI row {} col K(UserSelect) unknown value: '{}' -> ignored", excel_row, value);
            Ok(None)
        }
    }
}

fn parse_apply_flag(value: &str, excel_row: usize) -> Result<bool, String> {
    let trimmed = trim_excel_input(value);
    if trimmed.is_empty() {
        return Ok(false);
    }
    match trimmed.as_str() {
        v if matches!(v, "Y" | "y" | "Ｙ" | "ｙ") => Ok(true),
        v if matches!(v, "N" | "n" | "Ｎ" | "ｎ") => Ok(false),
        _ => {
            println!("[WARN] TRANSLATION_UI row {} col L(Apply) unknown value: '{}' -> treated as empty", excel_row, value);
            Ok(false)
        }
    }
}

fn load_ui_candidate4_map(input_path: &str) -> Result<HashMap<usize, String>, String> {
    let file = File::open(input_path).map_err(|e| format!("ui workbook zip open failed: {e}"))?;
    let mut zip = ZipArchive::new(file).map_err(|e| format!("ui workbook zip parse failed: {e}"))?;

    let workbook_xml = read_zip_string(&mut zip, "xl/workbook.xml")?;
    let workbook_rels_xml = read_zip_string(&mut zip, "xl/_rels/workbook.xml.rels")?;
    let shared_strings = load_shared_strings(&mut zip)?;

    let sheet_path = resolve_sheet_path(&workbook_xml, &workbook_rels_xml, "TRANSLATION_UI")?
        .ok_or_else(|| "TRANSLATION_UI xml path not found".to_string())?;

    let sheet_xml = read_zip_string(&mut zip, &sheet_path)?;
    extract_column_values(&sheet_xml, 'M', &shared_strings)
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

fn resolve_sheet_path(
    workbook_xml: &str,
    workbook_rels_xml: &str,
    sheet_name: &str,
) -> Result<Option<String>, String> {
    let marker = format!("name=\"{}\"", escape_xml_attr(sheet_name));
    let sheet_pos = match workbook_xml.find(&marker) {
        Some(pos) => pos,
        None => return Ok(None),
    };

    let rid_key = "r:id=\"";
    let rid_start_rel = workbook_xml[sheet_pos..]
        .find(rid_key)
        .ok_or_else(|| format!("r:id not found for sheet {sheet_name}"))?;
    let rid_start = sheet_pos + rid_start_rel + rid_key.len();
    let rid_end_rel = workbook_xml[rid_start..]
        .find('"')
        .ok_or_else(|| format!("r:id close quote not found for sheet {sheet_name}"))?;
    let rid = &workbook_xml[rid_start..rid_start + rid_end_rel];

    let rel_marker = format!("Id=\"{}\"", rid);
    let rel_pos = workbook_rels_xml
        .find(&rel_marker)
        .ok_or_else(|| format!("relationship not found for {rid}"))?;
    let target_key = "Target=\"";
    let target_start_rel = workbook_rels_xml[rel_pos..]
        .find(target_key)
        .ok_or_else(|| format!("Target not found for {rid}"))?;
    let target_start = rel_pos + target_start_rel + target_key.len();
    let target_end_rel = workbook_rels_xml[target_start..]
        .find('"')
        .ok_or_else(|| format!("Target close quote not found for {rid}"))?;
    let target = &workbook_rels_xml[target_start..target_start + target_end_rel];

    Ok(Some(if target.starts_with("xl/") {
        target.to_string()
    } else {
        format!("xl/{}", target)
    }))
}

fn load_shared_strings(zip: &mut ZipArchive<File>) -> Result<Vec<String>, String> {
    let xml = match read_zip_string(zip, "xl/sharedStrings.xml") {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };

    let mut values = Vec::new();
    let mut rest = xml.as_str();
    while let Some(si_start) = rest.find("<si") {
        let after_si = &rest[si_start..];
        let tag_end = after_si
            .find('>')
            .ok_or_else(|| "sharedStrings <si> malformed".to_string())?;
        let content = &after_si[tag_end + 1..];
        let si_end = content
            .find("</si>")
            .ok_or_else(|| "sharedStrings </si> missing".to_string())?;
        let si_xml = &content[..si_end];
        values.push(extract_text_nodes(si_xml));
        rest = &content[si_end + 5..];
    }
    Ok(values)
}

fn extract_column_values(
    sheet_xml: &str,
    target_col: char,
    shared_strings: &[String],
) -> Result<HashMap<usize, String>, String> {
    let mut map = HashMap::new();
    let mut rest = sheet_xml;

    while let Some(c_pos) = rest.find("<c ") {
        let after = &rest[c_pos..];
        let close = after
            .find('>')
            .ok_or_else(|| "cell start tag malformed".to_string())?;
        let start_tag = &after[..close + 1];
        let self_closing = start_tag.ends_with("/>");

        let cell_ref = match extract_attr(start_tag, "r") {
            Some(v) => v,
            None => {
                rest = &after[close + 1..];
                continue;
            }
        };

        let (col_letters, row_num) = split_cell_ref(&cell_ref)?;
        let is_target = col_letters.len() == 1 && col_letters.chars().next() == Some(target_col);

        if self_closing {
            rest = &after[close + 2..];
            if is_target {
                map.insert(row_num, String::new());
            }
            continue;
        }

        let tail = &after[close + 1..];
        let end_rel = tail
            .find("</c>")
            .ok_or_else(|| format!("cell close tag missing for {cell_ref}"))?;
        let inner = &tail[..end_rel];
        rest = &tail[end_rel + 4..];

        if !is_target {
            continue;
        }

        if let Some(formula) = extract_tag_text(inner, "f") {
            let trimmed = formula.trim();
            if !trimmed.is_empty() {
                map.insert(row_num, format!("={}", decode_xml_text(trimmed)));
                continue;
            }
        }

        let cell_type = extract_attr(start_tag, "t").unwrap_or_default();
        if cell_type == "inlineStr" {
            if let Some(text) = extract_inline_string(inner) {
                map.insert(row_num, decode_xml_text(text.trim()));
                continue;
            }
        }

        if let Some(raw_v) = extract_tag_text(inner, "v") {
            let raw = raw_v.trim();
            let resolved = if cell_type == "s" {
                raw.parse::<usize>()
                    .ok()
                    .and_then(|idx| shared_strings.get(idx).cloned())
                    .unwrap_or_default()
            } else {
                decode_xml_text(raw)
            };
            map.insert(row_num, resolved);
        } else {
            map.insert(row_num, String::new());
        }
    }

    Ok(map)
}

fn extract_attr(tag: &str, key: &str) -> Option<String> {
    let needle = format!("{}=\"", key);
    let start = tag.find(&needle)? + needle.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

fn extract_tag_text(inner: &str, tag: &str) -> Option<String> {
    let start_needle = format!("<{}", tag);
    let start_pos = inner.find(&start_needle)?;
    let after_start = &inner[start_pos..];
    let start_close = after_start.find('>')?;
    let content = &after_start[start_close + 1..];
    let end_needle = format!("</{}>", tag);
    let end_pos = content.find(&end_needle)?;
    Some(content[..end_pos].to_string())
}

fn extract_inline_string(inner: &str) -> Option<String> {
    if let Some(is_content) = extract_tag_text(inner, "is") {
        return Some(extract_text_nodes(&is_content));
    }
    None
}

fn extract_text_nodes(xml_fragment: &str) -> String {
    let xml_without_rph = remove_rph_blocks(xml_fragment);

    let mut out = String::new();
    let mut rest = xml_without_rph.as_str();

    while let Some(t_pos) = rest.find("<t") {
        let after = &rest[t_pos..];
        let close = match after.find('>') {
            Some(v) => v,
            None => break,
        };
        let content = &after[close + 1..];
        let end = match content.find("</t>") {
            Some(v) => v,
            None => break,
        };
        out.push_str(&decode_xml_text(&content[..end]));
        rest = &content[end + 4..];
    }

    out
}

fn remove_rph_blocks(xml_fragment: &str) -> String {
    let mut out = String::new();
    let mut rest = xml_fragment;

    loop {
        match rest.find("<rPh") {
            Some(start) => {
                out.push_str(&rest[..start]);
                let after = &rest[start..];
                match after.find("</rPh>") {
                    Some(end_rel) => {
                        rest = &after[end_rel + "</rPh>".len()..];
                    }
                    None => {
                        break;
                    }
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }

    out
}

fn split_cell_ref(cell_ref: &str) -> Result<(String, usize), String> {
    let mut letters = String::new();
    let mut digits = String::new();
    for ch in cell_ref.chars() {
        if ch.is_ascii_alphabetic() {
            letters.push(ch);
        } else if ch.is_ascii_digit() {
            digits.push(ch);
        }
    }
    if letters.is_empty() || digits.is_empty() {
        return Err(format!("invalid cell ref: {cell_ref}"));
    }
    let row = digits
        .parse::<usize>()
        .map_err(|e| format!("invalid cell ref row {cell_ref}: {e}"))?;
    Ok((letters, row))
}

fn decode_xml_text(input: &str) -> String {
    input
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn escape_xml_attr(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
