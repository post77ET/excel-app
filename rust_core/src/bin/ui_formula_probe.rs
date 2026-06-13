
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

use zip::ZipArchive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 || args.len() > 3 {
        eprintln!("usage: cargo run --bin ui_formula_probe -- <ui_xlsx_path> [sheet_name]");
        eprintln!("example: cargo run --bin ui_formula_probe -- TEST_work_ui.xlsx TRANSLATION_UI");
        std::process::exit(1);
    }

    let xlsx_path = &args[1];
    let sheet_name = if args.len() >= 3 {
        args[2].as_str()
    } else {
        "TRANSLATION_UI"
    };

    println!("[UI_FORMULA_PROBE] file={}", xlsx_path);
    println!("[UI_FORMULA_PROBE] target_sheet={}", sheet_name);

    let file = File::open(xlsx_path)?;
    let mut zip = ZipArchive::new(file)?;

    let workbook_xml = read_zip_text(&mut zip, "xl/workbook.xml")?;
    let workbook_rels_xml = read_zip_text(&mut zip, "xl/_rels/workbook.xml.rels")?;

    let rid = extract_sheet_rid(&workbook_xml, sheet_name)
        .ok_or_else(|| format!("sheet not found in workbook.xml: {}", sheet_name))?;
    let target = extract_relationship_target(&workbook_rels_xml, &rid)
        .ok_or_else(|| format!("relationship target not found for rid={}", rid))?;

    let sheet_path = if target.starts_with("xl/") {
        target
    } else {
        format!("xl/{}", target)
    };

    println!("[UI_FORMULA_PROBE] rid={}", rid);
    println!("[UI_FORMULA_PROBE] sheet_xml={}", sheet_path);

    let shared_strings = read_shared_strings_map(&mut zip).unwrap_or_default();
    println!("[UI_FORMULA_PROBE] shared_strings_count={}", shared_strings.len());

    let sheet_xml = read_zip_text(&mut zip, &sheet_path)?;

    // Probe only M column by default
    let mut found = 0usize;
    for row in 1..=5000u32 {
        let cell_ref = format!("M{}", row);
        if let Some(cell_xml) = extract_cell_xml(&sheet_xml, &cell_ref) {
            found += 1;
            let t_attr = extract_attr(&cell_xml, "t").unwrap_or_default();
            let s_attr = extract_attr(&cell_xml, "s").unwrap_or_default();
            let f_text = extract_tag_text(&cell_xml, "f").unwrap_or_default();
            let v_text = extract_tag_text(&cell_xml, "v").unwrap_or_default();
            let inline_text = extract_inline_string(&cell_xml).unwrap_or_default();

            let resolved_value = if t_attr == "s" {
                match v_text.parse::<usize>() {
                    Ok(idx) => shared_strings.get(&idx).cloned().unwrap_or_default(),
                    Err(_) => String::new(),
                }
            } else if t_attr == "inlineStr" {
                inline_text.clone()
            } else {
                v_text.clone()
            };

            println!(
                "[UI_FORMULA_PROBE] cell={} t={:?} s={:?} f={:?} v={:?} resolved={:?}",
                cell_ref, t_attr, s_attr, f_text, v_text, resolved_value
            );
        }
    }

    println!("[UI_FORMULA_PROBE] scanned_column=M found_cells={}", found);
    Ok(())
}

fn read_zip_text(zip: &mut ZipArchive<File>, name: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut f = zip.by_name(name)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(String::from_utf8(buf)?)
}

fn extract_sheet_rid(workbook_xml: &str, sheet_name: &str) -> Option<String> {
    let marker = format!("name=\"{}\"", escape_xml_attr(sheet_name));
    let pos = workbook_xml.find(&marker)?;
    let tail = &workbook_xml[pos..];
    let rid_key = "r:id=\"";
    let rid_pos = tail.find(rid_key)?;
    let start = pos + rid_pos + rid_key.len();
    let end_rel = workbook_xml[start..].find('"')?;
    Some(workbook_xml[start..start + end_rel].to_string())
}

fn extract_relationship_target(rels_xml: &str, rid: &str) -> Option<String> {
    let marker = format!("Id=\"{}\"", escape_xml_attr(rid));
    let pos = rels_xml.find(&marker)?;
    let tail = &rels_xml[pos..];
    let key = "Target=\"";
    let key_pos = tail.find(key)?;
    let start = pos + key_pos + key.len();
    let end_rel = rels_xml[start..].find('"')?;
    Some(rels_xml[start..start + end_rel].to_string())
}

fn extract_cell_xml(sheet_xml: &str, cell_ref: &str) -> Option<String> {
    // Prefer exact r="Mx"
    let marker = format!("<c r=\"{}\"", cell_ref);
    let start = sheet_xml.find(&marker)?;
    let tail = &sheet_xml[start..];

    if let Some(end_rel) = tail.find("</c>") {
        return Some(tail[..end_rel + 4].to_string());
    }

    if let Some(end_rel) = tail.find("/>") {
        return Some(tail[..end_rel + 2].to_string());
    }

    None
}

fn extract_attr(xml: &str, attr: &str) -> Option<String> {
    let marker = format!("{}=\"", attr);
    let start = xml.find(&marker)? + marker.len();
    let end_rel = xml[start..].find('"')?;
    Some(xml[start..start + end_rel].to_string())
}

fn extract_tag_text(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);

    let start = xml.find(&open)? + open.len();
    let end_rel = xml[start..].find(&close)?;
    Some(unescape_xml_text(&xml[start..start + end_rel]))
}

fn extract_inline_string(xml: &str) -> Option<String> {
    // inline string is typically <is><t>...</t></is>
    extract_tag_text(xml, "t")
}

fn read_shared_strings_map(zip: &mut ZipArchive<File>) -> Result<HashMap<usize, String>, Box<dyn std::error::Error>> {
    let xml = match read_zip_text(zip, "xl/sharedStrings.xml") {
        Ok(v) => v,
        Err(_) => return Ok(HashMap::new()),
    };

    let mut map = HashMap::new();
    let mut pos = 0usize;
    let mut idx = 0usize;

    while let Some(si_start_rel) = xml[pos..].find("<si") {
        let si_start = pos + si_start_rel;
        let si_open_end_rel = xml[si_start..].find('>').ok_or("sharedStrings malformed: <si> open")?;
        let content_start = si_start + si_open_end_rel + 1;
        let si_end_rel = xml[content_start..].find("</si>").ok_or("sharedStrings malformed: </si>")?;
        let si_content = &xml[content_start..content_start + si_end_rel];

        let text = collect_all_t_text(si_content);
        map.insert(idx, text);
        idx += 1;
        pos = content_start + si_end_rel + 5;
    }

    Ok(map)
}

fn collect_all_t_text(si_content: &str) -> String {
    let mut out = String::new();
    let mut pos = 0usize;

    while let Some(t_start_rel) = si_content[pos..].find("<t") {
        let t_start = pos + t_start_rel;
        let t_open_end_rel = match si_content[t_start..].find('>') {
            Some(v) => v,
            None => break,
        };
        let text_start = t_start + t_open_end_rel + 1;
        let t_end_rel = match si_content[text_start..].find("</t>") {
            Some(v) => v,
            None => break,
        };
        out.push_str(&unescape_xml_text(&si_content[text_start..text_start + t_end_rel]));
        pos = text_start + t_end_rel + 4;
    }

    out
}

fn escape_xml_attr(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unescape_xml_text(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
