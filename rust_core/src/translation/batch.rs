use anyhow::{anyhow, Result};

use super::{
    translate_many_with_provider,
    BatchLimits,
    IndexedTextUnit,
    TranslationProvider,
    TranslationRequestOwned,
    WorkTableRow,
    WorkTableTranslatedRow,
};

pub async fn execute_worktable_rows_buffered(
    provider: TranslationProvider,
    rows: &[WorkTableRow],
    source_lang: &str,
    target_lang: &str,
    limits: BatchLimits,
) -> Result<Vec<WorkTableTranslatedRow>> {
    let units = flatten_rows(rows, source_lang, target_lang);
    let translated_units = execute_units_buffered(provider, &units, limits).await?;
    rebuild_rows(rows, translated_units)
}

pub async fn execute_whole_rows_buffered(
    provider: TranslationProvider,
    rows: &[WorkTableRow],
    source_lang: &str,
    target_lang: &str,
    limits: BatchLimits,
) -> Result<Vec<WorkTableTranslatedRow>> {
    let mut whole_units = Vec::with_capacity(rows.len());

    for (row_index, row) in rows.iter().enumerate() {
        let whole_text = row.parts.join("");
        whole_units.push(IndexedTextUnit {
            row_index,
            part_index: 0,
            request: TranslationRequestOwned {
                text: whole_text,
                source_lang: source_lang.to_string(),
                target_lang: target_lang.to_string(),
            },
        });
    }

    let translated_units = execute_units_buffered(provider, &whole_units, limits).await?;
    rebuild_rows_whole(rows, translated_units)
}

fn flatten_rows(
    rows: &[WorkTableRow],
    source_lang: &str,
    target_lang: &str,
) -> Vec<IndexedTextUnit> {
    let mut units = Vec::new();

    for (row_index, row) in rows.iter().enumerate() {
        for (part_index, part) in row.parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }

            units.push(IndexedTextUnit {
                row_index,
                part_index,
                request: TranslationRequestOwned {
                    text: part.clone(),
                    source_lang: source_lang.to_string(),
                    target_lang: target_lang.to_string(),
                },
            });
        }
    }

    units
}

async fn execute_units_buffered(
    provider: TranslationProvider,
    units: &[IndexedTextUnit],
    limits: BatchLimits,
) -> Result<Vec<IndexedTextUnit>> {
    let mut output = Vec::with_capacity(units.len());
    let mut start = 0;

    while start < units.len() {
        let end = decide_chunk_end(units, start, limits);
        let chunk = &units[start..end];

        let requests: Vec<TranslationRequestOwned> =
            chunk.iter().map(|u| u.request.clone()).collect();

        let translated_texts = translate_many_with_provider(
            provider,
            &requests,
            limits.amazon_parallelism,
        )
        .await?;

        if translated_texts.len() != chunk.len() {
            return Err(anyhow!(
                "Translated count mismatch: expected {}, got {}",
                chunk.len(),
                translated_texts.len()
            ));
        }

        for (unit, translated) in chunk.iter().zip(translated_texts.into_iter()) {
            output.push(IndexedTextUnit {
                row_index: unit.row_index,
                part_index: unit.part_index,
                request: TranslationRequestOwned {
                    text: translated,
                    source_lang: unit.request.source_lang.clone(),
                    target_lang: unit.request.target_lang.clone(),
                },
            });
        }

        start = end;
    }

    Ok(output)
}

fn decide_chunk_end(
    units: &[IndexedTextUnit],
    start: usize,
    limits: BatchLimits,
) -> usize {
    let mut count = 0usize;
    let mut chars = 0usize;
    let mut idx = start;

    while idx < units.len() {
        let next_len = units[idx].request.text.chars().count();

        if count >= limits.max_items {
            break;
        }

        if count > 0 && chars + next_len > limits.max_chars {
            break;
        }

        count += 1;
        chars += next_len;
        idx += 1;
    }

    idx.max(start + 1)
}

fn rebuild_rows(
    original_rows: &[WorkTableRow],
    translated_units: Vec<IndexedTextUnit>,
) -> Result<Vec<WorkTableTranslatedRow>> {
    let mut translated_parts: Vec<Vec<String>> = original_rows
        .iter()
        .map(|row| vec![String::new(); row.parts.len()])
        .collect();

    for unit in translated_units {
        translated_parts[unit.row_index][unit.part_index] = unit.request.text;
    }

    let mut result = Vec::with_capacity(original_rows.len());

    for (idx, row) in original_rows.iter().enumerate() {
        result.push(WorkTableTranslatedRow {
            origin_cell: row.origin_cell.clone(),
            cell_attr: row.cell_attr.clone(),
            translated_parts: translated_parts[idx].clone(),
        });
    }

    Ok(result)
}

fn rebuild_rows_whole(
    original_rows: &[WorkTableRow],
    translated_units: Vec<IndexedTextUnit>,
) -> Result<Vec<WorkTableTranslatedRow>> {
    let mut translated_texts = vec![String::new(); original_rows.len()];

    for unit in translated_units {
        translated_texts[unit.row_index] = unit.request.text;
    }

    let mut result = Vec::with_capacity(original_rows.len());

    for (idx, row) in original_rows.iter().enumerate() {
        result.push(WorkTableTranslatedRow {
            origin_cell: row.origin_cell.clone(),
            cell_attr: row.cell_attr.clone(),
            translated_parts: vec![translated_texts[idx].clone()],
        });
    }

    Ok(result)
}