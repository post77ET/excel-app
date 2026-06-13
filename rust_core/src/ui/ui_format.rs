use umya_spreadsheet::Worksheet;

pub fn apply_ui_format(sheet: &mut Worksheet, max_row: u32, max_col: u32) {
    if max_col == 0 || max_row == 0 { return; }
    sheet.set_auto_filter(format!("A1:{}1", col_to_letters(max_col)));
    for row in 1..=max_row {
        for col in 1..=max_col {
            let addr = format!("{}{}", col_to_letters(col), row);
            let style = sheet.get_style_mut(addr.as_str());
            style.get_alignment_mut().set_wrap_text(true);
            if row == 1 { style.get_font_mut().set_bold(true); }
        }
    }
    for col in 1..=max_col {
        let width = calculate_column_width(sheet, col, max_row).clamp(8.0, 50.0);
        sheet.get_column_dimension_by_number_mut(&col).set_width(width);
    }
    for row in 1..=max_row {
        let height = calculate_row_height(sheet, row, max_col);
        sheet.get_row_dimension_mut(&row).set_height(height);
    }
}

fn calculate_column_width(sheet: &Worksheet, col: u32, max_row: u32) -> f64 {
    let mut max_width = 8.0;
    for row in 1..=max_row {
        let text = sheet.get_value((col, row));
        let width = estimate_max_line_width(&text);
        if width > max_width { max_width = width; }
    }
    max_width + 2.0
}

fn calculate_row_height(sheet: &Worksheet, row: u32, max_col: u32) -> f64 {
    let mut max_lines = 1u32;
    for col in 1..=max_col {
        let text = sheet.get_value((col, row));
        let explicit_lines = text.lines().count().max(1) as u32;
        let longest_line = estimate_max_line_width(&text).clamp(8.0, 50.0);
        let wrapped_lines = ((longest_line / 50.0).ceil() as u32).max(1);
        let total_lines = explicit_lines.max(wrapped_lines);
        if total_lines > max_lines { max_lines = total_lines; }
    }
    18.0 * max_lines as f64
}

fn estimate_max_line_width(text: &str) -> f64 {
    text.lines().map(estimate_text_width).fold(0.0, |acc, v| if v > acc { v } else { acc })
}

fn estimate_text_width(text: &str) -> f64 {
    text.chars().map(|c| if c == '\n' || c == '\r' { 0.0 } else if c.is_ascii() { 1.0 } else { 2.0 }).sum()
}

fn col_to_letters(mut col: u32) -> String {
    let mut s = String::new();
    while col > 0 {
        let r = ((col - 1) % 26) as u8;
        s.insert(0, (b'A' + r) as char);
        col = (col - 1) / 26;
    }
    s
}
