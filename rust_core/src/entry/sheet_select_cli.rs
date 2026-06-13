use crate::entry::entry_state::EntryError;
use std::io::{self, Write};

pub fn select_sheets(sheet_names: &[String], experience_mode: bool) -> Result<Vec<String>, EntryError> {
    if let Ok(selection) = std::env::var("ETB_SELECTED_SHEETS") {
        let selected = parse_sheet_selection(&selection, sheet_names, experience_mode)?;
        println!("=== SHEET SELECTION ===");
        println!("[ENV] ETB_SELECTED_SHEETS = {}", selection);
        println!("[ENV] selected sheets = {:?}", selected);
        if experience_mode {
            println!("[EXPERIENCE] sheet selection rule = single sheet only / A1:D5 fixed");
        }
        return Ok(selected);
    }

    loop {
        println!("=== SHEET SELECTION ===");
        for (i, name) in sheet_names.iter().enumerate() {
            println!("  {}: {}", i + 1, name);
        }

        if experience_mode {
            println!("INPUT: single sheet number only, exit");
            println!("[EXPERIENCE] A1:D5 only. Multiple sheets and ALL are not allowed.");
        } else {
            println!("INPUT: number(comma), all, exit");
        }
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("q") {
            return Err(EntryError::UserExit);
        }

        match parse_sheet_selection(input, sheet_names, experience_mode) {
            Ok(selected) => return Ok(selected),
            Err(e) => {
                if experience_mode {
                    println!("Invalid input. Experience course accepts one sheet number only. retry. ({:?})", e);
                } else {
                    println!("Invalid input. retry. ({:?})", e);
                }
            }
        }
    }
}

pub fn confirm(selected: &[String]) -> Result<(), EntryError> {
    if std::env::var("ETB_SELECTED_SHEETS").is_ok() {
        println!("=== CONFIRM ===");
        println!("[ENV] Selected sheets = {:?}", selected);
        println!("[ENV] auto-confirmed by ETB_SELECTED_SHEETS");
        return Ok(());
    }

    loop {
        println!("=== CONFIRM ===");
        println!("Selected sheets = {:?}", selected);
        println!("INPUT: ok / back / exit");
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim().to_lowercase();

        match input.as_str() {
            "ok" => return Ok(()),
            "back" => return Err(EntryError::UserBack),
            "exit" | "q" => return Err(EntryError::UserExit),
            _ => println!("Invalid input. retry."),
        }
    }
}

fn parse_sheet_selection(input: &str, sheet_names: &[String], experience_mode: bool) -> Result<Vec<String>, EntryError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(EntryError::Internal("empty sheet selection".to_string()));
    }

    if experience_mode {
        if trimmed.eq_ignore_ascii_case("all") || trimmed.contains(',') {
            return Err(EntryError::Internal(
                "experience course allows single sheet number only".to_string(),
            ));
        }

        let idx = trimmed.parse::<usize>().map_err(|_| {
            EntryError::Internal("experience course requires a sheet number".to_string())
        })?;

        if idx >= 1 && idx <= sheet_names.len() {
            return Ok(vec![sheet_names[idx - 1].clone()]);
        }

        return Err(EntryError::Internal(format!("sheet index out of range: {idx}")));
    }

    if trimmed.eq_ignore_ascii_case("all") {
        return Ok(sheet_names.to_vec());
    }

    let mut selected = Vec::new();
    for part in trimmed.split(',') {
        let token = part.trim();
        if token.is_empty() {
            return Err(EntryError::Internal("empty sheet selection token".to_string()));
        }

        if let Ok(idx) = token.parse::<usize>() {
            if idx >= 1 && idx <= sheet_names.len() {
                let name = sheet_names[idx - 1].clone();
                if !selected.contains(&name) {
                    selected.push(name);
                }
                continue;
            }
            return Err(EntryError::Internal(format!("sheet index out of range: {idx}")));
        }

        if let Some(name) = sheet_names.iter().find(|name| name.as_str() == token) {
            if !selected.contains(name) {
                selected.push(name.clone());
            }
            continue;
        }

        return Err(EntryError::Internal(format!("unknown sheet selection token: {token}")));
    }

    if selected.is_empty() {
        Err(EntryError::Internal("no sheet selected".to_string()))
    } else {
        Ok(selected)
    }
}
