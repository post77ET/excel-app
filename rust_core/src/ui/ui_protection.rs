use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use std::collections::HashMap;

use calamine::{open_workbook_auto, Data, Reader};
use umya_spreadsheet::structs::{Color, Fill, PatternFill, PatternValues};
use umya_spreadsheet::{Workbook, Style, Worksheet};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

pub const UI_SHEET_NAME: &str = "TRANSLATION_UI";
pub const SECURITY_REPORT_SHEET_NAME: &str = "SECURITY_REPORT";
pub const INTERNAL_SHEET_NAME: &str = "__ETB_INTERNAL";
pub const WARNINGS_SHEET_NAME: &str = "TRANSLATION_WARNINGS";

fn all_ui_cols() -> [&'static str; 17] {
    [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q",
    ]
}

fn locked_gray() -> Style {
    let mut style = Style::default();
    style.protection_mut().set_locked(true);

    let mut color = Color::default();
    color.set_argb_str("FFE7E6E6");

    let mut pattern = PatternFill::default();
    pattern.set_pattern_type(PatternValues::Solid);
    pattern.set_foreground_color(color);

    let mut fill = Fill::default();
    fill.set_pattern_fill(pattern);
    style.set_fill(fill);

    style
}

fn unlocked_orange() -> Style {
    let mut style = Style::default();
    style.protection_mut().set_locked(false);

    let mut color = Color::default();
    color.set_argb_str("FFFFF2CC");

    let mut pattern = PatternFill::default();
    pattern.set_pattern_type(PatternValues::Solid);
    pattern.set_foreground_color(color);

    let mut fill = Fill::default();
    fill.set_pattern_fill(pattern);
    style.set_fill(fill);

    style
}

fn normalize_cell_string(value: String) -> String {
    value.trim().to_string()
}

fn get_cell_string(sheet: &Worksheet, addr: &str) -> String {
    match sheet.cell(addr) {
        Some(cell) => normalize_cell_string(cell.value().to_string()),
        None => String::new(),
    }
}

fn has_candidate_value(sheet: &Worksheet, row: u32) -> bool {
    for col in ["G", "H", "I"] {
        let addr = format!("{col}{row}");
        let value = get_cell_string(sheet, addr.as_str());
        if !value.is_empty() {
            return true;
        }
    }
    false
}

fn row_should_allow_user_input(sheet: &Worksheet, row: u32) -> bool {
    if row <= 1 {
        return false;
    }

    let writeback_mode = {
        let addr = format!("F{row}");
        get_cell_string(sheet, addr.as_str())
    };

    if writeback_mode == "Preserve" || writeback_mode == "SharedFormulaFollower" {
        return false;
    }

    has_candidate_value(sheet, row)
}

fn set_ui_row_lock_state(sheet: &mut Worksheet, row: u32) {
    let locked_style = locked_gray();
    let editable_style = unlocked_orange();
    let editable = row_should_allow_user_input(sheet, row);

    for col in all_ui_cols() {
        let addr = format!("{col}{row}");
        let cell = sheet.cell_mut(addr.as_str());

        let is_input_col = matches!(col, "K" | "L" | "M");

        if is_input_col && editable {
            cell.set_style(editable_style.clone());
        } else {
            cell.set_style(locked_style.clone());
        }
    }
}

fn apply_password_sheet_protection(sheet: &mut Worksheet, password: &str) {
    let protection = sheet.sheet_protection_mut();

    protection.set_sheet(true);
    protection.set_password(password);

    protection.set_objects(false);
    protection.set_scenarios(false);
    protection.set_format_cells(false);
    protection.set_format_columns(false);
    protection.set_format_rows(false);
    protection.set_insert_columns(false);
    protection.set_insert_rows(false);
    protection.set_insert_hyperlinks(false);
    protection.set_delete_columns(false);
    protection.set_delete_rows(false);
    protection.set_sort(false);
    protection.set_auto_filter(false);
    protection.set_pivot_tables(false);

    // Excel/OOXMLでは selectLockedCells / selectUnlockedCells は
    // 「選択を禁止する」側の属性なので、許可したい場合は false にする。
    protection.set_select_locked_cells(false);
    protection.set_select_unlocked_cells(false);
}

fn apply_passwordless_sheet_protection(sheet: &mut Worksheet) {
    let protection = sheet.sheet_protection_mut();

    protection.set_sheet(true);
    protection.set_password("");

    protection.set_objects(false);
    protection.set_scenarios(false);
    protection.set_format_cells(false);
    protection.set_format_columns(false);
    protection.set_format_rows(false);
    protection.set_insert_columns(false);
    protection.set_insert_rows(false);
    protection.set_insert_hyperlinks(false);
    protection.set_delete_columns(false);
    protection.set_delete_rows(false);
    protection.set_sort(false);
    protection.set_auto_filter(false);
    protection.set_pivot_tables(false);

    protection.set_select_locked_cells(false);
    protection.set_select_unlocked_cells(false);
}

fn protect_entire_sheet_with_password(
    book: &mut Workbook,
    sheet_name: &str,
    password: &str,
) -> Result<(), String> {
    let sheet = book
        .sheet_by_name_mut(sheet_name)
        .map_err(|_| format!("sheet not found for protection: {sheet_name}"))?;

    apply_password_sheet_protection(sheet, password);
    Ok(())
}

fn protect_entire_sheet_without_password(
    book: &mut Workbook,
    sheet_name: &str,
) -> Result<(), String> {
    let sheet = book
        .sheet_by_name_mut(sheet_name)
        .map_err(|_| format!("sheet not found for protection: {sheet_name}"))?;

    apply_passwordless_sheet_protection(sheet);
    Ok(())
}

pub fn load_sheet_protection_password() -> String {
    env::var("ETB_SHEET_PROTECTION_PASSWORD")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| env::var("ETB_PROTECT").ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "ETB_PROTECT".to_string())
}

pub fn apply_ui_protection(book: &mut Workbook, max_row: u32) {
    let password = load_sheet_protection_password();

    let Ok(sheet) = book.sheet_by_name_mut(UI_SHEET_NAME) else {
        return;
    };

    apply_password_sheet_protection(sheet, &password);

    for row in 1..=max_row {
        set_ui_row_lock_state(sheet, row);
    }
}

pub fn apply_generate_protection(
    book: &mut Workbook,
    main_sheet_names: &[String],
    ui_max_row: u32,
    password: &str,
) -> Result<(), String> {
    {
        let ui_sheet = book
            .sheet_by_name_mut(UI_SHEET_NAME)
            .map_err(|_| format!("sheet not found for protection: {UI_SHEET_NAME}"))?;

        apply_password_sheet_protection(ui_sheet, password);

        for row in 1..=ui_max_row {
            set_ui_row_lock_state(ui_sheet, row);
        }
    }

    for sheet_name in main_sheet_names {
        protect_entire_sheet_with_password(book, sheet_name.as_str(), password)?;
    }

    protect_entire_sheet_with_password(book, SECURITY_REPORT_SHEET_NAME, password)?;
    protect_entire_sheet_with_password(book, INTERNAL_SHEET_NAME, password)?;
    protect_entire_sheet_with_password(book, WARNINGS_SHEET_NAME, password)?;

    Ok(())
}

pub fn apply_apply_output_protection(
    book: &mut Workbook,
    main_sheet_names: &[String],
) -> Result<(), String> {
    for sheet_name in main_sheet_names {
        if book.sheet_by_name(sheet_name.as_str()).is_ok() {
            protect_entire_sheet_without_password(book, sheet_name.as_str())?;
        }
    }

    // Apply output is based on the server-side original workbook.
    // It normally does not contain TRANSLATION_UI / TRANSLATION_WARNINGS.
    // Therefore UI-only sheets are protected only when they actually exist.
    if book.sheet_by_name(UI_SHEET_NAME).is_ok() {
        protect_entire_sheet_without_password(book, UI_SHEET_NAME)?;
    }

    if book.sheet_by_name(WARNINGS_SHEET_NAME).is_ok() {
        protect_entire_sheet_without_password(book, WARNINGS_SHEET_NAME)?;
    }

    Ok(())
}

/// 図形・画像(drawing*.xml)を、翻訳前の元ファイルの内容でそのまま復元する。
///
/// 背景（実データで確認済み）：umya-spreadsheet 3.0.0 は、グループ図形が
/// 2階層以上ネストしている場合（例：画像2枚＋図形をグループ化し、それを
/// さらに別の要素とグループ化する等）、read→write の往復でグループ内の
/// 画像・図形のアンカー座標（twoCellAnchor の from/to）を正しく再現できず、
/// 位置ズレ・縦横比の破損を引き起こすことが確認された。ネストが1階層まで
/// （単純なグループ化1回）であれば往復は正常。
///
/// 翻訳処理はセルのテキストだけを書き換えるものであり、画像・図形
/// （drawing*.xml）の内容は本来一切変更する必要がない。そのため、
/// umyaによる書き出しが終わった後、対象シートのdrawing*.xmlを
/// 「翻訳前の元ファイルにあったバイト列」でそのまま置き換えることで、
/// グループのネスト階層数に関わらず、画像・図形が絶対に壊れないようにする。
///
/// - `original_path`: 翻訳前の元ファイル（ユーザーが最初にアップロードしたもの）
/// - `output_path`: umyaが書き出した直後の生成/反映後ファイル（このファイルを書き換える）
/// - `sheet_names`: 復元対象のシート名一覧（通常は処理対象の全シート）
pub fn restore_original_drawings_in_file(
    original_path: &str,
    output_path: &str,
    sheet_names: &[String],
) -> Result<(), String> {
    let orig_file = fs::File::open(original_path)
        .map_err(|e| format!("restore_drawings: original open failed: {e}"))?;
    let mut orig_archive = ZipArchive::new(orig_file)
        .map_err(|e| format!("restore_drawings: original zip open failed: {e}"))?;

    let orig_workbook_xml = read_zip_entry_string(&mut orig_archive, "xl/workbook.xml")?;
    let orig_workbook_rels_xml =
        read_zip_entry_string(&mut orig_archive, "xl/_rels/workbook.xml.rels")?;

    // シート名 -> 元ファイル側のdrawingパス、を先に確定させる。
    let mut sheet_to_drawing_bytes: HashMap<String, Vec<u8>> = HashMap::new();
    for sheet_name in sheet_names {
        let Some(sheet_xml_path) =
            resolve_sheet_xml_path(&orig_workbook_xml, &orig_workbook_rels_xml, sheet_name)?
        else {
            continue; // シートが元ファイル側に無ければスキップ（新規追加シート等）
        };
        let Some(drawing_path) =
            resolve_sheet_drawing_path(&mut orig_archive, &sheet_xml_path)?
        else {
            continue; // このシートに図形・画像が無ければスキップ
        };
        let Ok(mut entry) = orig_archive.by_name(drawing_path.as_str()) else {
            continue;
        };
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|e| format!("restore_drawings: original drawing read failed: {e}"))?;
        sheet_to_drawing_bytes.insert(sheet_name.clone(), data);
    }

    if sheet_to_drawing_bytes.is_empty() {
        return Ok(()); // 復元対象なし
    }

    // 出力ファイル側で、各シート名がどのdrawingパスに対応するか確定させる。
    let out_source_path = Path::new(output_path);
    let out_file = fs::File::open(out_source_path)
        .map_err(|e| format!("restore_drawings: output open failed: {e}"))?;
    let mut out_archive = ZipArchive::new(out_file)
        .map_err(|e| format!("restore_drawings: output zip open failed: {e}"))?;

    let out_workbook_xml = read_zip_entry_string(&mut out_archive, "xl/workbook.xml")?;
    let out_workbook_rels_xml =
        read_zip_entry_string(&mut out_archive, "xl/_rels/workbook.xml.rels")?;

    // drawingパス(出力ファイル内) -> 復元すべきバイト列
    let mut target_entries: HashMap<String, Vec<u8>> = HashMap::new();
    for (sheet_name, bytes) in &sheet_to_drawing_bytes {
        let Some(sheet_xml_path) =
            resolve_sheet_xml_path(&out_workbook_xml, &out_workbook_rels_xml, sheet_name)?
        else {
            continue;
        };
        let Some(drawing_path) = resolve_sheet_drawing_path(&mut out_archive, &sheet_xml_path)?
        else {
            continue;
        };
        target_entries.insert(drawing_path, bytes.clone());
    }

    if target_entries.is_empty() {
        return Ok(());
    }

    let temp_path = build_temp_xlsx_path(out_source_path);
    let temp_file = fs::File::create(&temp_path)
        .map_err(|e| format!("restore_drawings: temp create failed: {e}"))?;
    let mut writer = ZipWriter::new(temp_file);

    for idx in 0..out_archive.len() {
        let mut entry = out_archive
            .by_index(idx)
            .map_err(|e| format!("restore_drawings: zip entry open failed: {e}"))?;
        let entry_name = entry.name().to_string();
        let options = SimpleFileOptions::default().compression_method(entry.compression());

        if entry.is_dir() {
            writer
                .add_directory(entry_name, options)
                .map_err(|e| format!("restore_drawings: add directory failed: {e}"))?;
            continue;
        }

        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|e| format!("restore_drawings: entry read failed: {e}"))?;

        writer
            .start_file(entry_name.as_str(), options)
            .map_err(|e| format!("restore_drawings: start file failed: {e}"))?;

        if let Some(restore_bytes) = target_entries.get(&entry_name) {
            println!("[RESTORE_DRAWINGS] restoring original bytes for {entry_name}");
            writer
                .write_all(restore_bytes)
                .map_err(|e| format!("restore_drawings: write restored drawing failed: {e}"))?;
        } else {
            writer
                .write_all(&data)
                .map_err(|e| format!("restore_drawings: passthrough write failed: {e}"))?;
        }
    }

    writer
        .finish()
        .map_err(|e| format!("restore_drawings: zip finish failed: {e}"))?;

    drop(out_archive);
    fs::rename(&temp_path, out_source_path)
        .map_err(|e| format!("restore_drawings: rename temp failed: {e}"))?;

    Ok(())
}

/// sheetN.xml内の <drawing r:id="..."/> から、対応するdrawingM.xmlのパスを取得する。
fn resolve_sheet_drawing_path(
    archive: &mut ZipArchive<fs::File>,
    sheet_xml_path: &str,
) -> Result<Option<String>, String> {
    let sheet_xml = read_zip_entry_string(archive, sheet_xml_path)?;
    let Some(rid_start) = sheet_xml.find("<drawing ") else {
        return Ok(None); // このシートに図形・画像が無い
    };
    let tail = &sheet_xml[rid_start..];
    let key = "r:id=\"";
    let Some(key_pos) = tail.find(key) else {
        return Ok(None);
    };
    let value_start = key_pos + key.len();
    let Some(end) = tail[value_start..].find('"') else {
        return Ok(None);
    };
    let rid = &tail[value_start..value_start + end];

    // sheetN.xml.rels のパスを組み立てる（例: xl/worksheets/sheet5.xml -> xl/worksheets/_rels/sheet5.xml.rels）
    let Some(slash_idx) = sheet_xml_path.rfind('/') else {
        return Ok(None);
    };
    let (dir, file) = sheet_xml_path.split_at(slash_idx);
    let file = &file[1..]; // 先頭の '/' を除く
    let rels_path = format!("{dir}/_rels/{file}.rels");

    let rels_xml = match read_zip_entry_string(archive, rels_path.as_str()) {
        Ok(xml) => xml,
        Err(_) => return Ok(None), // rels自体が無ければ図形は無い扱い
    };
    let target = extract_relationship_target(&rels_xml, rid)?;
    let Some(target) = target else {
        return Ok(None);
    };

    // targetは "../drawings/drawing3.xml" のような相対パス（sheetN.xmlのある場所からの相対）
    let resolved = normalize_relative_path(dir, target.as_str());
    Ok(Some(resolved))
}

/// "xl/worksheets" + "../drawings/drawing3.xml" のような相対パスを正規化して
/// "xl/drawings/drawing3.xml" のような絶対パス（zip内パス）にする。
fn normalize_relative_path(base_dir: &str, relative: &str) -> String {
    let mut parts: Vec<&str> = base_dir.split('/').collect();
    for seg in relative.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}


pub fn patch_named_sheet_protection_in_file(
    path: &str,
    sheet_passwords: &[(&str, Option<&str>)],
) -> Result<(), String> {
    let source_path = Path::new(path);
    if !source_path.exists() {
        return Err(format!("xlsx not found for sheet protection patch: {path}"));
    }

    let source_file =
        fs::File::open(source_path).map_err(|e| format!("patch open failed: {e}"))?;
    let mut archive =
        ZipArchive::new(source_file).map_err(|e| format!("patch zip open failed: {e}"))?;

    let workbook_xml = read_zip_entry_string(&mut archive, "xl/workbook.xml")?;
    let workbook_rels_xml = read_zip_entry_string(&mut archive, "xl/_rels/workbook.xml.rels")?;

    let mut target_entries: Vec<(String, Option<String>)> = Vec::new();
    for (sheet_name, password) in sheet_passwords {
        let sheet_path = resolve_sheet_xml_path(&workbook_xml, &workbook_rels_xml, sheet_name)?
            .ok_or_else(|| format!("sheet xml path not found: {sheet_name}"))?;
        target_entries.push((sheet_path, password.map(|v| v.to_string())));
    }

    let temp_path = build_temp_xlsx_path(source_path);

    let temp_file =
        fs::File::create(&temp_path).map_err(|e| format!("patch temp create failed: {e}"))?;
    let mut writer = ZipWriter::new(temp_file);

    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|e| format!("patch zip entry open failed: {e}"))?;

        let entry_name = entry.name().to_string();
        let options = SimpleFileOptions::default().compression_method(entry.compression());

        if entry.is_dir() {
            writer
                .add_directory(entry_name, options)
                .map_err(|e| format!("patch add directory failed: {e}"))?;
            continue;
        }

        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|e| format!("patch read failed: {e}"))?;

        writer
            .start_file(entry_name.as_str(), options)
            .map_err(|e| format!("patch start file failed: {e}"))?;

        if let Some((_, password)) = target_entries
            .iter()
            .find(|(target_name, _)| target_name == &entry_name)
        {
            let xml = String::from_utf8(data)
                .map_err(|e| format!("sheet xml utf8 decode failed: {e}"))?;
            let patched_xml = patch_sheet_xml_protection(&xml, password.as_deref());
            writer
                .write_all(patched_xml.as_bytes())
                .map_err(|e| format!("patch write protected sheet failed: {e}"))?;
        } else {
            writer
                .write_all(&data)
                .map_err(|e| format!("patch write passthrough failed: {e}"))?;
        }
    }

    writer
        .finish()
        .map_err(|e| format!("patch finalize failed: {e}"))?;

    fs::remove_file(source_path)
        .map_err(|e| format!("patch original remove failed: {e}"))?;
    fs::rename(&temp_path, source_path)
        .map_err(|e| format!("patch replace failed: {e}"))?;

    Ok(())
}

pub fn patch_shared_formula_masters_in_file(path: &str) -> Result<(), String> {
    // UIシートの SharedFormulaParent 行から、(sheet_name, cell) -> formula_body を収集
    let target_map = collect_shared_formula_master_formulas_from_ui(path)?;
    if target_map.is_empty() {
        return Ok(());
    }

    let source_path = Path::new(path);
    if !source_path.exists() {
        return Err(format!("xlsx not found for shared-formula patch: {path}"));
    }

    let source_file =
        fs::File::open(source_path).map_err(|e| format!("shared-formula patch open failed: {e}"))?;
    let mut archive =
        ZipArchive::new(source_file).map_err(|e| format!("shared-formula patch zip open failed: {e}"))?;

    let workbook_xml = read_zip_entry_string(&mut archive, "xl/workbook.xml")?;
    let workbook_rels_xml = read_zip_entry_string(&mut archive, "xl/_rels/workbook.xml.rels")?;

    // sheet_name -> sheet_xml_path を解決
    let mut target_entries: Vec<(String, HashMap<String, String>)> = Vec::new();
    for (sheet_name, cell_map) in target_map {
        let sheet_path = resolve_sheet_xml_path(&workbook_xml, &workbook_rels_xml, sheet_name.as_str())?
            .ok_or_else(|| format!("sheet xml path not found: {sheet_name}"))?;
        target_entries.push((sheet_path, cell_map));
    }

    let temp_path = build_temp_xlsx_path(source_path);
    let temp_file =
        fs::File::create(&temp_path).map_err(|e| format!("shared-formula patch temp create failed: {e}"))?;
    let mut writer = ZipWriter::new(temp_file);

    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|e| format!("shared-formula patch zip entry open failed: {e}"))?;

        let entry_name = entry.name().to_string();
        let options = SimpleFileOptions::default().compression_method(entry.compression());

        if entry.is_dir() {
            writer
                .add_directory(entry_name, options)
                .map_err(|e| format!("shared-formula patch add directory failed: {e}"))?;
            continue;
        }

        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|e| format!("shared-formula patch read failed: {e}"))?;

        writer
            .start_file(entry_name.as_str(), options)
            .map_err(|e| format!("shared-formula patch start file failed: {e}"))?;

        if let Some((_, cell_map)) = target_entries
            .iter()
            .find(|(target_name, _)| target_name == &entry_name)
        {
            let xml = String::from_utf8(data)
                .map_err(|e| format!("shared-formula sheet xml utf8 decode failed: {e}"))?;
            let patched_xml = patch_sheet_xml_shared_formula_masters(&xml, cell_map);
            writer
                .write_all(patched_xml.as_bytes())
                .map_err(|e| format!("shared-formula patch write failed: {e}"))?;
        } else {
            writer
                .write_all(&data)
                .map_err(|e| format!("shared-formula patch write passthrough failed: {e}"))?;
        }
    }

    writer
        .finish()
        .map_err(|e| format!("shared-formula patch finalize failed: {e}"))?;

    fs::remove_file(source_path)
        .map_err(|e| format!("shared-formula patch original remove failed: {e}"))?;
    fs::rename(&temp_path, source_path)
        .map_err(|e| format!("shared-formula patch replace failed: {e}"))?;

    Ok(())
}

fn collect_shared_formula_master_formulas_from_ui(
    path: &str,
) -> Result<HashMap<String, HashMap<String, String>>, String> {
    let mut workbook =
        open_workbook_auto(path).map_err(|e| format!("shared-formula ui open failed: {e}"))?;

    let range = workbook
        .worksheet_range(UI_SHEET_NAME)
        .map_err(|e| format!("shared-formula ui sheet read failed: {e}"))?;

    let mut out: HashMap<String, HashMap<String, String>> = HashMap::new();

    // ヘッダ行(1行目)を除外。range の row/col は 0-index。
    for row_idx in 1..range.height() {
        let sheet_name = cell_string(&range, row_idx, 0);
        let cell_addr = cell_string(&range, row_idx, 1);
        let original_writeback = cell_string(&range, row_idx, 4);
        let writeback_mode = cell_string(&range, row_idx, 5);

        if writeback_mode.trim() != "Formula" {
            continue;
        }
        if sheet_name.trim().is_empty() || cell_addr.trim().is_empty() {
            continue;
        }

        let formula_body = normalize_formula_body_plain(&original_writeback);
        if formula_body.is_empty() {
            continue;
        }

        out.entry(sheet_name)
            .or_insert_with(HashMap::new)
            .insert(cell_addr, formula_body);
    }

    Ok(out)
}

fn cell_string(range: &calamine::Range<Data>, row: usize, col: usize) -> String {
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

///
/// 例:
///   "=IF(A1>0,1,0)" -> "IF(A1>0,1,0)"
///   "==IF(...)"     -> "IF(...)"   (保険として = を複数剥がす)
///   "'=IF(...)"     -> "IF(...)"   (ユーザ入力都合の先頭 ' を剥がす)
///
fn normalize_formula_body_plain(input: &str) -> String {
    let mut text = input.trim();

    if let Some(s) = text.strip_prefix('\'') {
        text = s.trim_start();
    }

    while let Some(s) = text.strip_prefix('=') {
        text = s.trim_start();
    }

    text.to_string()
}

fn patch_sheet_xml_shared_formula_masters(xml: &str, cell_map: &HashMap<String, String>) -> String {
    if cell_map.is_empty() {
        return xml.to_string();
    }

    let mut out = String::with_capacity(xml.len() + cell_map.len() * 32);
    let mut rest = xml;

    while let Some(c_pos) = rest.find("<c ") {
        // 直前までを吐く
        out.push_str(&rest[..c_pos]);
        let after = &rest[c_pos..];

        let Some(close_rel) = after.find('>') else {
            out.push_str(after);
            return out;
        };
        let start_tag = &after[..close_rel + 1];
        let self_closing = start_tag.trim_end().ends_with("/>");

        // r="A1" を抜く
        let cell_ref = extract_attr(start_tag, "r").unwrap_or_default();

        if self_closing {
            // 自己完結セルはそのまま
            out.push_str(start_tag);
            rest = &after[close_rel + 1..];
            continue;
        }

        let tail = &after[close_rel + 1..];
        let Some(end_rel) = tail.find("</c>") else {
            out.push_str(after);
            return out;
        };
        let end_idx = close_rel + 1 + end_rel + "</c>".len();
        let cell_block = &after[..end_idx];

        if let Some(formula_body) = cell_map.get(&cell_ref) {
            out.push_str(&patch_cell_block_shared_master(cell_block, formula_body));
        } else {
            out.push_str(cell_block);
        }

        rest = &after[end_idx..];
    }

    out.push_str(rest);
    out
}

fn patch_cell_block_shared_master(cell_block: &str, formula_body: &str) -> String {
    let Some(f_start) = cell_block.find("<f") else {
        return cell_block.to_string();
    };

    let after_f = &cell_block[f_start..];
    let Some(gt_rel) = after_f.find('>') else {
        return cell_block.to_string();
    };

    let open_tag = &after_f[..gt_rel + 1];
    // shared で master(=ref属性あり) のみ対象
    if !(open_tag.contains("t=\"shared\"") && open_tag.contains("ref=\"")) {
        return cell_block.to_string();
    }

    let escaped = xml_escape_text(formula_body);

    // <f .../> のケース
    if open_tag.trim_end().ends_with("/>") {
        let open_non_self = open_tag.trim_end_matches("/>").to_string() + ">";

        let mut out = String::with_capacity(cell_block.len() + escaped.len() + 8);
        out.push_str(&cell_block[..f_start]);
        out.push_str(&open_non_self);
        out.push_str(&escaped);
        out.push_str("</f>");
        out.push_str(&cell_block[f_start + open_tag.len()..]);
        return out;
    }

    // <f ...> .... </f> のケース
    let after_open = &cell_block[f_start + open_tag.len()..];
    let Some(end_rel) = after_open.find("</f>") else {
        return cell_block.to_string();
    };

    let inner = &after_open[..end_rel];
    if !inner.trim().is_empty() {
        // 既に式がある → 触らない
        return cell_block.to_string();
    }

    let mut out = String::with_capacity(cell_block.len() + escaped.len());
    out.push_str(&cell_block[..f_start]);
    out.push_str(open_tag);
    out.push_str(&escaped);
    out.push_str("</f>");
    out.push_str(&after_open[end_rel + "</f>".len()..]);
    out
}

fn extract_attr(tag: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn xml_escape_text(input: &str) -> String {
    // 注意: & のエスケープを最初にする
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn read_zip_entry_string(
    archive: &mut ZipArchive<fs::File>,
    entry_name: &str,
) -> Result<String, String> {
    let mut entry = archive
        .by_name(entry_name)
        .map_err(|e| format!("zip entry not found {entry_name}: {e}"))?;

    let mut data = Vec::new();
    entry
        .read_to_end(&mut data)
        .map_err(|e| format!("zip entry read failed {entry_name}: {e}"))?;

    String::from_utf8(data).map_err(|e| format!("zip entry utf8 decode failed {entry_name}: {e}"))
}

fn resolve_sheet_xml_path(
    workbook_xml: &str,
    workbook_rels_xml: &str,
    sheet_name: &str,
) -> Result<Option<String>, String> {
    let rid = extract_sheet_rid(workbook_xml, sheet_name)?;
    let Some(rid) = rid else {
        return Ok(None);
    };

    let target = extract_relationship_target(workbook_rels_xml, rid.as_str())?;
    let Some(target) = target else {
        return Ok(None);
    };

    if target.starts_with("xl/") {
        Ok(Some(target))
    } else {
        Ok(Some(format!("xl/{target}")))
    }
}

fn extract_sheet_rid(workbook_xml: &str, sheet_name: &str) -> Result<Option<String>, String> {
    // workbook.xml内では & 等がXMLエスケープされて保存されているため、
    // シート名側もエスケープしてから検索する（例: "A&B" は name="A&amp;B" と一致させる）。
    let escaped_sheet_name = xml_escape_text(sheet_name);
    let marker = format!("name=\"{escaped_sheet_name}\"");
    let start = match workbook_xml.find(marker.as_str()) {
        Some(pos) => pos,
        None => return Ok(None),
    };

    let tail = &workbook_xml[start..];
    let rid_key = "r:id=\"";
    let rid_start_rel = tail
        .find(rid_key)
        .ok_or_else(|| format!("r:id not found for sheet {sheet_name}"))?;
    let rid_value_start = start + rid_start_rel + rid_key.len();

    let rid_tail = &workbook_xml[rid_value_start..];
    let rid_end_rel = rid_tail
        .find('"')
        .ok_or_else(|| format!("r:id closing quote not found for sheet {sheet_name}"))?;

    Ok(Some(workbook_xml[rid_value_start..rid_value_start + rid_end_rel].to_string()))
}

fn extract_relationship_target(
    workbook_rels_xml: &str,
    rid: &str,
) -> Result<Option<String>, String> {
    let marker = format!("Id=\"{rid}\"");
    let start = match workbook_rels_xml.find(marker.as_str()) {
        Some(pos) => pos,
        None => return Ok(None),
    };

    let tail = &workbook_rels_xml[start..];
    let target_key = "Target=\"";
    let target_start_rel = tail
        .find(target_key)
        .ok_or_else(|| format!("Target not found for relationship {rid}"))?;
    let target_value_start = start + target_start_rel + target_key.len();

    let target_tail = &workbook_rels_xml[target_value_start..];
    let target_end_rel = target_tail
        .find('"')
        .ok_or_else(|| format!("Target closing quote not found for relationship {rid}"))?;

    Ok(Some(
        workbook_rels_xml[target_value_start..target_value_start + target_end_rel].to_string(),
    ))
}

fn patch_sheet_xml_protection(xml: &str, password: Option<&str>) -> String {
    let password_attr = password
        .filter(|v| !v.is_empty())
        .map(|v| format!(" password=\"{}\"", hash_worksheet_password(v)))
        .unwrap_or_default();

    // 重要: selectLockedCells / selectUnlockedCells は 1 だと禁止側になる。
    // 入力可能にするため 0 を明示する。
    let protection_tag = format!(
        "<sheetProtection sheet=\"1\" objects=\"0\" scenarios=\"0\" formatCells=\"0\" formatColumns=\"0\" formatRows=\"0\" insertColumns=\"0\" insertRows=\"0\" insertHyperlinks=\"0\" deleteColumns=\"0\" deleteRows=\"0\" sort=\"0\" autoFilter=\"0\" pivotTables=\"0\" selectLockedCells=\"0\" selectUnlockedCells=\"0\"{password_attr}/>"
    );

    if let Some(start) = xml.find("<sheetProtection") {
        if let Some(end_rel) = xml[start..].find("/>") {
            let end = start + end_rel + 2;
            let mut out = String::with_capacity(xml.len() + protection_tag.len());
            out.push_str(&xml[..start]);
            out.push_str(&protection_tag);
            out.push_str(&xml[end..]);
            return out;
        }
    }

    if let Some(insert_pos) = xml.find("</sheetData>") {
        let insert_at = insert_pos + "</sheetData>".len();
        let mut out = String::with_capacity(xml.len() + protection_tag.len());
        out.push_str(&xml[..insert_at]);
        out.push_str(&protection_tag);
        out.push_str(&xml[insert_at..]);
        return out;
    }

    if let Some(insert_at) = xml.find("</worksheet>") {
        let mut out = String::with_capacity(xml.len() + protection_tag.len());
        out.push_str(&xml[..insert_at]);
        out.push_str(&protection_tag);
        out.push_str(&xml[insert_at..]);
        return out;
    }

    xml.to_string()
}


/// Generate出力ファイルのTRANSLATION_UIシートのDataValidation（K列）に
/// showErrorMessage="1" を付与する（手入力不正値をExcelレベルで拒否させる）
pub fn patch_datavalidation_show_error_in_file(path: &str) -> Result<(), String> {
    let source_path = Path::new(path);
    let temp_path = build_temp_xlsx_path(source_path);

    let file = fs::File::open(source_path)
        .map_err(|e| format!("patch_dv open failed: {e}"))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| format!("patch_dv zip open failed: {e}"))?;

    // TRANSLATION_UIシートのパスを特定
    let workbook_xml = read_zip_string_dv(&mut archive, "xl/workbook.xml")?;
    let rels_xml = read_zip_string_dv(&mut archive, "xl/_rels/workbook.xml.rels")?;
    let ui_sheet_path = resolve_sheet_xml_path(&workbook_xml, &rels_xml, UI_SHEET_NAME)?
        .ok_or_else(|| "patch_dv: TRANSLATION_UI not found".to_string())?;

    let out_file = fs::File::create(&temp_path)
        .map_err(|e| format!("patch_dv temp create failed: {e}"))?;
    let mut writer = ZipWriter::new(out_file);

    for idx in 0..archive.len() {
        let mut entry = archive.by_index(idx)
            .map_err(|e| format!("patch_dv entry open failed: {e}"))?;
        let entry_name = entry.name().to_string();
        let options = SimpleFileOptions::default().compression_method(entry.compression());

        if entry.is_dir() {
            writer.add_directory(&entry_name, options)
                .map_err(|e| format!("patch_dv add dir failed: {e}"))?;
            continue;
        }

        let mut data = Vec::new();
        entry.read_to_end(&mut data)
            .map_err(|e| format!("patch_dv read failed: {e}"))?;

        writer.start_file(&entry_name, options)
            .map_err(|e| format!("patch_dv start file failed: {e}"))?;

        if entry_name == ui_sheet_path {
            let xml = String::from_utf8(data)
                .map_err(|e| format!("patch_dv utf8 failed: {e}"))?;
            let patched = inject_show_error_message(&xml);
            writer.write_all(patched.as_bytes())
                .map_err(|e| format!("patch_dv write failed: {e}"))?;
        } else {
            writer.write_all(&data)
                .map_err(|e| format!("patch_dv passthrough failed: {e}"))?;
        }
    }

    writer.finish()
        .map_err(|e| format!("patch_dv finalize failed: {e}"))?;

    fs::remove_file(source_path)
        .map_err(|e| format!("patch_dv remove failed: {e}"))?;
    fs::rename(&temp_path, source_path)
        .map_err(|e| format!("patch_dv rename failed: {e}"))?;

    Ok(())
}

/// DataValidationタグに showErrorMessage="1" と errorStyle="stop" を付与する。
/// umya が既に showErrorMessage="1" を出力するため、両属性をそれぞれ独立に
/// 「無ければ付ける」方式にする（errorStyle が欠落するとリスト外の手入力を弾けない）。
fn inject_show_error_message(xml: &str) -> String {
    let mut result = String::with_capacity(xml.len());
    let mut search = xml;

    while let Some(pos) = search.find("<dataValidation ") {
        result.push_str(&search[..pos]);
        let tag_end = search[pos..].find('>').unwrap_or(search.len() - pos);
        let tag = &search[pos..pos + tag_end + 1];

        let mut patched_tag = tag.to_string();

        // errorStyle="stop"（リスト外の入力を拒否）が無ければ付与する
        if !patched_tag.contains("errorStyle") {
            patched_tag = patched_tag.replacen(
                "<dataValidation ",
                "<dataValidation errorStyle=\"stop\" ",
                1,
            );
        }
        // showErrorMessage="1"（エラーダイアログを表示）が無ければ付与する
        if !patched_tag.contains("showErrorMessage") {
            patched_tag = patched_tag.replacen(
                "<dataValidation ",
                "<dataValidation showErrorMessage=\"1\" ",
                1,
            );
        }

        result.push_str(&patched_tag);
        search = &search[pos + tag_end + 1..];
    }
    result.push_str(search);
    result
}

fn read_zip_string_dv(archive: &mut ZipArchive<fs::File>, name: &str) -> Result<String, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|e| format!("dv zip entry '{name}' not found: {e}"))?;
    let mut data = Vec::new();
    entry.read_to_end(&mut data)
        .map_err(|e| format!("dv read '{name}' failed: {e}"))?;
    String::from_utf8(data).map_err(|e| format!("dv utf8 '{name}' failed: {e}"))
}

fn hash_worksheet_password(password: &str) -> String {
    let mut hash: u16 = 0;
    for byte in password.as_bytes().iter().rev() {
        hash = ((hash >> 14) & 0x0001) | ((hash << 1) & 0x7fff);
        hash ^= *byte as u16;
    }
    hash = ((hash >> 14) & 0x0001) | ((hash << 1) & 0x7fff);
    hash ^= password.len() as u16;
    hash ^= 0xCE4B;
    format!("{:04X}", hash)
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

    parent.join(format!(".__etb_sheet_patch_{nanos}.xlsx"))
}
