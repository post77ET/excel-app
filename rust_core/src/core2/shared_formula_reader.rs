use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct FormulaMeta {
    pub formula_text: String,
    pub has_formula_tag: bool,
    pub is_shared_parent: bool,
    pub is_shared_follower: bool,
    pub shared_index: Option<String>,
    pub resolved_formula_text: Option<String>,
}

#[derive(Debug, Clone)]
struct SharedFormulaMaster {
    anchor_address: String,
    formula_text: String,
}

pub fn parse_formula_cells(sheet_xml: &str) -> HashMap<String, FormulaMeta> {
    let mut raw: Vec<(String, FormulaMeta)> = Vec::new();
    let mut masters: HashMap<String, SharedFormulaMaster> = HashMap::new();

    for block in find_cell_blocks(sheet_xml) {
        if let Some(reference) = extract_attr(block, "r") {
            let meta = extract_formula_meta(block);
            if meta.has_formula_tag {
                if meta.is_shared_parent {
                    if let Some(si) = meta.shared_index.clone() {
                        masters.insert(
                            si,
                            SharedFormulaMaster {
                                anchor_address: reference.clone(),
                                formula_text: meta.formula_text.clone(),
                            },
                        );
                    }
                }
                raw.push((reference, meta));
            }
        }
    }

    let mut out = HashMap::new();
    for (reference, mut meta) in raw {
        if meta.is_shared_follower {
            if let Some(si) = meta.shared_index.clone() {
                if let Some(master) = masters.get(&si) {
                    if let Ok(resolved) = resolve_shared_formula_follower(
                        &master.formula_text,
                        &master.anchor_address,
                        &reference,
                    ) {
                        meta.resolved_formula_text = Some(resolved);
                    }
                }
            }
        }
        out.insert(reference, meta);
    }

    out
}

fn find_cell_blocks(xml: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut pos = 0usize;

    while let Some(start_rel) = xml[pos..].find("<c ") {
        let start = pos + start_rel;
        let rest = &xml[start..];
        let Some(open_end_rel) = rest.find('>') else {
            break;
        };
        let open_tag = &rest[..=open_end_rel];

        if open_tag.trim_end().ends_with("/>") {
            out.push(open_tag);
            pos = start + open_end_rel + 1;
            continue;
        }

        let after_open = &rest[open_end_rel + 1..];
        let Some(close_rel) = after_open.find("</c>") else {
            break;
        };
        let end = start + open_end_rel + 1 + close_rel + "</c>".len();
        out.push(&xml[start..end]);
        pos = end;
    }

    out
}

fn extract_formula_meta(block: &str) -> FormulaMeta {
    let Some(f_start) = block.find("<f") else {
        return FormulaMeta::default();
    };

    let rest = &block[f_start..];
    let Some(gt_rel) = rest.find('>') else {
        return FormulaMeta::default();
    };

    let open_tag = &rest[..=gt_rel];
    let inner = &rest[gt_rel + 1..];
    let end_rel = inner.find("</f>");
    let formula_text = end_rel
        .map(|idx| xml_unescape(&inner[..idx]))
        .unwrap_or_default();

    let tag_self_closing = open_tag.trim_end().ends_with("/>");
    let shared_type = extract_attr(open_tag, "t").as_deref() == Some("shared");
    let shared_index = extract_attr(open_tag, "si");
    let is_shared_follower =
        shared_type && formula_text.trim().is_empty() && (tag_self_closing || end_rel.is_some());
    let is_shared_parent = shared_type && !is_shared_follower;

    FormulaMeta {
        formula_text,
        has_formula_tag: true,
        is_shared_parent,
        is_shared_follower,
        shared_index,
        resolved_formula_text: None,
    }
}

pub fn resolve_shared_formula_follower(
    master_formula: &str,
    master_anchor: &str,
    follower_anchor: &str,
) -> Result<String, String> {
    let (master_col, master_row) = split_cell_ref(master_anchor)?;
    let (follower_col, follower_row) = split_cell_ref(follower_anchor)?;

    let row_delta = follower_row as i32 - master_row as i32;
    let col_delta = col_letters_to_number(&follower_col)? as i32 - col_letters_to_number(&master_col)? as i32;

    let formula_body = master_formula.strip_prefix('=').unwrap_or(master_formula);
    let shifted = shift_formula_a1(formula_body, row_delta, col_delta)?;
    Ok(format!("={shifted}"))
}

fn shift_formula_a1(formula: &str, row_delta: i32, col_delta: i32) -> Result<String, String> {
    let mut out = String::with_capacity(formula.len() + 16);
    let mut i = 0usize;
    let mut in_string = false;

    while i < formula.len() {
        let remaining = &formula[i..];
        let mut chars = remaining.chars();
        let Some(ch) = chars.next() else {
            break;
        };
        let ch_len = ch.len_utf8();

        if ch == '"' {
            out.push(ch);
            i += ch_len;

            if in_string {
                if formula[i..].starts_with('"') {
                    out.push('"');
                    i += '"'.len_utf8();
                    continue;
                }
                in_string = false;
                continue;
            }

            in_string = true;
            continue;
        }

        if in_string {
            out.push(ch);
            i += ch_len;
            continue;
        }

        if let Some((consumed, shifted)) = try_shift_reference_token(remaining, row_delta, col_delta)? {
            out.push_str(&shifted);
            i += consumed;
            continue;
        }

        out.push(ch);
        i += ch_len;
    }

    Ok(out)
}

fn try_shift_reference_token(s: &str, row_delta: i32, col_delta: i32) -> Result<Option<(usize, String)>, String> {
    if let Some((sheet_prefix_len, sheet_prefix)) = parse_sheet_prefix(s) {
        let remain = &s[sheet_prefix_len..];
        if let Some((consumed_ref_len, shifted_ref)) = parse_and_shift_ref_or_range(remain, row_delta, col_delta)? {
            let total_len = sheet_prefix_len + consumed_ref_len;
            return Ok(Some((total_len, format!("{}{}", sheet_prefix, shifted_ref))));
        }
    }

    if let Some((consumed_len, shifted_ref)) = parse_and_shift_ref_or_range(s, row_delta, col_delta)? {
        return Ok(Some((consumed_len, shifted_ref)));
    }

    Ok(None)
}

fn parse_sheet_prefix(s: &str) -> Option<(usize, String)> {
    if s.starts_with('\'') {
        let mut i = 1usize;
        let bytes = s.as_bytes();
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c == '\'' && i + 1 < bytes.len() && bytes[i + 1] as char == '!' {
                let prefix = &s[..=i + 1];
                return Some((i + 2, prefix.to_string()));
            }
            i += 1;
        }
        None
    } else {
        let mut i = 0usize;
        let bytes = s.as_bytes();
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c == '!' {
                if i == 0 {
                    return None;
                }
                let prefix = &s[..=i];
                if prefix[..prefix.len() - 1]
                    .chars()
                    .all(|x| x.is_ascii_alphanumeric() || x == '_' || x == '.')
                {
                    return Some((i + 1, prefix.to_string()));
                }
                return None;
            }
            if !(c.is_ascii_alphanumeric() || c == '_' || c == '.') {
                break;
            }
            i += 1;
        }
        None
    }
}

fn parse_and_shift_ref_or_range(s: &str, row_delta: i32, col_delta: i32) -> Result<Option<(usize, String)>, String> {
    if let Some((len1, r1)) = parse_single_ref(s)? {
        if s[len1..].starts_with(':') {
            let s2 = &s[len1 + 1..];
            if let Some((len2, r2)) = parse_single_ref(s2)? {
                let shifted1 = shift_single_ref(&r1, row_delta, col_delta)?;
                let shifted2 = shift_single_ref(&r2, row_delta, col_delta)?;
                return Ok(Some((len1 + 1 + len2, format!("{}:{}", shifted1, shifted2))));
            }
        }

        let shifted = shift_single_ref(&r1, row_delta, col_delta)?;
        return Ok(Some((len1, shifted)));
    }
    Ok(None)
}

#[derive(Debug, Clone)]
struct ParsedRef {
    col_abs: bool,
    col_letters: String,
    row_abs: bool,
    row_number: u32,
}

fn parse_single_ref(s: &str) -> Result<Option<(usize, ParsedRef)>, String> {
    let bytes = s.as_bytes();
    let mut i = 0usize;

    let mut col_abs = false;
    if i < bytes.len() && bytes[i] as char == '$' {
        col_abs = true;
        i += 1;
    }

    let col_start = i;
    while i < bytes.len() && (bytes[i] as char).is_ascii_alphabetic() {
        i += 1;
    }
    if i == col_start {
        return Ok(None);
    }
    let col_letters = s[col_start..i].to_ascii_uppercase();

    let mut row_abs = false;
    if i < bytes.len() && bytes[i] as char == '$' {
        row_abs = true;
        i += 1;
    }

    let row_start = i;
    while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
        i += 1;
    }
    if i == row_start {
        return Ok(None);
    }

    let row_number: u32 = s[row_start..i]
        .parse()
        .map_err(|_| format!("invalid cell reference: {}", &s[..i]))?;

    if i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphanumeric() || c == '_' {
            return Ok(None);
        }
    }

    Ok(Some((
        i,
        ParsedRef {
            col_abs,
            col_letters,
            row_abs,
            row_number,
        },
    )))
}

fn shift_single_ref(r: &ParsedRef, row_delta: i32, col_delta: i32) -> Result<String, String> {
    let col_num = col_letters_to_number(&r.col_letters)? as i32;
    let shifted_col = if r.col_abs { col_num } else { col_num + col_delta };
    let shifted_row = if r.row_abs { r.row_number as i32 } else { r.row_number as i32 + row_delta };

    if shifted_col < 1 || shifted_row < 1 {
        return Err(format!("reference underflow: {}{}", r.col_letters, r.row_number));
    }

    Ok(format!(
        "{}{}{}{}",
        if r.col_abs { "$" } else { "" },
        number_to_col_letters(shifted_col as u32),
        if r.row_abs { "$" } else { "" },
        shifted_row
    ))
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
    Ok((col, row.parse::<u32>().map_err(|_| format!("invalid row: {cell_ref}"))?))
}

fn col_letters_to_number(s: &str) -> Result<u32, String> {
    let mut n = 0u32;
    for ch in s.chars() {
        if !ch.is_ascii_alphabetic() {
            return Err(format!("invalid column: {s}"));
        }
        n = n * 26 + ((ch.to_ascii_uppercase() as u8 - b'A' + 1) as u32);
    }
    Ok(n)
}

fn number_to_col_letters(mut n: u32) -> String {
    let mut buf = Vec::new();
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        buf.push((b'A' + rem) as char);
        n = (n - 1) / 26;
    }
    buf.iter().rev().collect()
}

fn extract_attr(tag: &str, attr_name: &str) -> Option<String> {
    let needle = format!("{attr_name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn xml_unescape(input: &str) -> String {
    input
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
