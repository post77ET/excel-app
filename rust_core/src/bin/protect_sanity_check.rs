use std::path::Path;

use umya_spreadsheet::structs::{Color, Fill, PatternFill, PatternValues};
use umya_spreadsheet::{new_file, Style};

fn locked_gray() -> Style {
    let mut style = Style::default();
    style.get_protection_mut().set_locked(true);

    let mut color = Color::default();
    color.set_argb("FFE7E6E6");

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
    style.get_protection_mut().set_locked(false);

    let mut color = Color::default();
    color.set_argb("FFFFF2CC");

    let mut pattern = PatternFill::default();
    pattern.set_pattern_type(PatternValues::Solid);
    pattern.set_foreground_color(color);

    let mut fill = Fill::default();
    fill.set_pattern_fill(pattern);
    style.set_fill(fill);

    style
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_path = "protect_sanity_check.xlsx";
    let password = std::env::var("ETB_SHEET_PROTECTION_PASSWORD")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| std::env::var("ETB_PROTECT").ok().filter(|v| !v.trim().is_empty()))
        .unwrap_or_else(|| "ETB_PROTECT".to_string());

    let mut book = new_file();

    let sheet_name = "PROTECT_TEST";
    let _ = book.new_sheet(sheet_name);
    let sheet = book
        .get_sheet_by_name_mut(sheet_name)
        .ok_or("PROTECT_TEST sheet not found")?;

    let locked = locked_gray();
    let unlocked = unlocked_orange();

    sheet.get_cell_mut("A1").set_value("判定");
    sheet.get_cell_mut("B1").set_value("説明");
    sheet.get_cell_mut("C1").set_value("期待動作");

    sheet.get_cell_mut("A2").set_value("LOCKED");
    sheet.get_cell_mut("B2").set_value("グレーセル");
    sheet.get_cell_mut("C2").set_value("編集不可");

    sheet.get_cell_mut("A3").set_value("UNLOCKED");
    sheet.get_cell_mut("B3").set_value("オレンジセル");
    sheet.get_cell_mut("C3").set_value("編集可");

    sheet.get_cell_mut("A4").set_value("UNLOCKED");
    sheet.get_cell_mut("B4").set_value("オレンジセル");
    sheet.get_cell_mut("C4").set_value("編集可");

    sheet.get_cell_mut("A5").set_value("LOCKED");
    sheet.get_cell_mut("B5").set_value("グレーセル");
    sheet.get_cell_mut("C5").set_value("編集不可");

    for row in 1..=5u32 {
        for col in ["A", "B", "C"] {
            let addr = format!("{col}{row}");
            let cell = sheet.get_cell_mut(addr.as_str());

            match row {
                3 | 4 => cell.set_style(unlocked.clone()),
                _ => cell.set_style(locked.clone()),
            };
        }
    }

    {
        let protection = sheet.get_sheet_protection_mut();
        protection.set_sheet(true);
        protection.set_password(password.as_str());

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

        protection.set_select_locked_cells(true);
        protection.set_select_unlocked_cells(true);
    }

    sheet.get_column_dimension_mut("A").set_width(18.0);
    sheet.get_column_dimension_mut("B").set_width(24.0);
    sheet.get_column_dimension_mut("C").set_width(18.0);

    umya_spreadsheet::writer::xlsx::write(&book, Path::new(output_path))?;

    println!("created: {}", output_path);
    println!("password: {}", password);
    println!("check points:");
    println!(" - A2:C2, A5:C5 should be locked");
    println!(" - A3:C3, A4:C4 should be editable");
    println!(" - orange cells must be editable under sheet protection");

    Ok(())
}