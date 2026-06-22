use crate::adapters::translator_trait::TranslatorAdapter;
use crate::adapters::types::{Lang, TranslateRequest};
use crate::core1::text_structure_analyzer::analyze_text_structure;
use crate::core1::translation_policy::TranslationPolicyDecision;
use crate::core1::types::{CandidateAlarms, CandidateBundle, DefaultSelect, Segment};
use crate::core2::formula_text::{
    split_preserving_structure,
    extract_quoted_literals,
    reassemble_formula,
};
use crate::core2::structure_types::LogicalCell;
use crate::core2::structure_types::LogicalCellKind;
use crate::direction::DirectionProfile;
use crate::infra::app_error::AppError;
use std::fs::OpenOptions;
use std::io::Write;

#[derive(Debug, Clone)]
struct SegmentRequestPlan {
    segment_idx: usize,
    text: String,
}

#[derive(Debug, Clone)]
struct SegmentCellPlan {
    original_text: String,
    segments: Vec<Segment>,
    request_positions: Vec<usize>,
}

#[derive(Debug, Clone)]
struct WholeRequestPlan {
    cell_idx: usize,
    text: String,
}


#[derive(Debug, Clone)]
pub struct CandidateGenerationPlan {
    pub enabled_candidates: Vec<u8>,
    pub default_candidate_priority: Vec<u8>,
    pub job_accept_threshold: f64,
}

impl Default for CandidateGenerationPlan {
    fn default() -> Self {
        Self {
            enabled_candidates: vec![1, 2, 3],
            default_candidate_priority: vec![1, 2, 3],
            job_accept_threshold: 0.80,
        }
    }
}

impl CandidateGenerationPlan {
    fn is_enabled(&self, candidate_no: u8) -> bool {
        self.enabled_candidates.contains(&candidate_no)
    }

    fn priority(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for &candidate_no in &self.default_candidate_priority {
            if (1..=3).contains(&candidate_no)
                && self.is_enabled(candidate_no)
                && !out.contains(&candidate_no)
            {
                out.push(candidate_no);
            }
        }
        for &candidate_no in &self.enabled_candidates {
            if (1..=3).contains(&candidate_no) && !out.contains(&candidate_no) {
                out.push(candidate_no);
            }
        }
        out
    }
}


#[derive(Debug, Clone, Default)]
pub struct CandidateUsageEstimate {
    pub translatable_cells: usize,
    pub candidate_units: usize,
    pub candidate1_requests: usize,
    pub candidate1_chars: usize,
    pub candidate2_requests: usize,
    pub candidate2_chars: usize,
    pub candidate3_requests: usize,
    pub candidate3_chars: usize,
}

impl CandidateUsageEstimate {
    pub fn total_requests(&self) -> usize {
        self.candidate1_requests + self.candidate2_requests + self.candidate3_requests
    }

    pub fn total_chars(&self) -> usize {
        self.candidate1_chars + self.candidate2_chars + self.candidate3_chars
    }
}

pub fn estimate_candidate_usage(
    logical_cells: &[LogicalCell],
    policies: &[TranslationPolicyDecision],
    candidate_plan: &CandidateGenerationPlan,
    direction: &dyn DirectionProfile,
) -> Result<CandidateUsageEstimate, AppError> {
    if logical_cells.len() != policies.len() {
        return Err(AppError::Internal(format!(
            "candidate estimate input length mismatch: cells={} policies={}",
            logical_cells.len(),
            policies.len()
        )));
    }

    let mut out = CandidateUsageEstimate::default();

    for (idx, logical_cell) in logical_cells.iter().enumerate() {
        if !policies[idx].translate_candidates {
            continue;
        }

        out.translatable_cells += 1;

        if candidate_plan.is_enabled(1) {
            out.candidate_units += 1;
            let (requests, chars) = estimate_segment_requests(&logical_cell.source_text, direction);
            out.candidate1_requests += requests;
            out.candidate1_chars += chars;
        }

        if candidate_plan.is_enabled(2) {
            out.candidate_units += 1;
            let (requests, chars) = estimate_segment_requests(&logical_cell.source_text, direction);
            out.candidate2_requests += requests;
            out.candidate2_chars += chars;
        }

        if candidate_plan.is_enabled(3) {
            out.candidate_units += 1;
            // F-2 整合: candidate3 は実処理と一致させる。
            // - 数式セル: 引用符内リテラルのみ翻訳 → リテラル数=request, リテラル文字数合計=chars。
            //             リテラル無し数式は翻訳機を呼ばない → request=0, chars=0。
            // - 非数式セル: 従来どおり全文1リクエスト=全文字数。
            if is_formula_cell(logical_cell) {
                let (_template, literals) = extract_quoted_literals(&logical_cell.source_text);
                // リテラル無し数式は翻訳機を呼ばない → request=0 / chars=0。
                if !literals.is_empty() {
                    out.candidate3_requests += literals.len();
                    out.candidate3_chars +=
                        literals.iter().map(|l| l.chars().count()).sum::<usize>();
                }
            } else {
                out.candidate3_requests += 1;
                out.candidate3_chars += logical_cell.source_text.chars().count();
            }
        }
    }

    Ok(out)
}

fn estimate_segment_requests(text: &str, direction: &dyn DirectionProfile) -> (usize, usize) {
    let mut requests = 0usize;
    let mut chars = 0usize;

    for seg in split_preserving_structure(text) {
        if should_translate_segment(seg.target, &seg.text, direction) {
            requests += 1;
            chars += seg.text.chars().count();
        }
    }

    (requests, chars)
}

pub fn build_candidate_bundle(
    logical_cell: &LogicalCell,
    policy: &TranslationPolicyDecision,
    default_select: DefaultSelect,
    adapter1: &dyn TranslatorAdapter,
    adapter2: &dyn TranslatorAdapter,
    adapter3: &dyn TranslatorAdapter,
    direction: &dyn DirectionProfile,
) -> Result<CandidateBundle, AppError> {
    let (from_lang, to_lang) = direction.lang_pair();
    let bundles = build_candidate_bundles_batch(
        std::slice::from_ref(logical_cell),
        std::slice::from_ref(policy),
        std::slice::from_ref(&default_select),
        adapter1,
        adapter2,
        adapter3,
        50,
        3000,
        &CandidateGenerationPlan::default(),
        from_lang,
        to_lang,
        direction,
    )?;

    bundles
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Internal("candidate bundle missing".to_string()))
}

pub fn build_candidate_bundles_batch(
    logical_cells: &[LogicalCell],
    policies: &[TranslationPolicyDecision],
    default_selects: &[DefaultSelect],
    adapter1: &dyn TranslatorAdapter,
    adapter2: &dyn TranslatorAdapter,
    adapter3: &dyn TranslatorAdapter,
    batch_max_items: usize,
    batch_max_chars: usize,
    candidate_plan: &CandidateGenerationPlan,
    from_lang: Lang,
    to_lang: Lang,
    direction: &dyn DirectionProfile,
) -> Result<Vec<CandidateBundle>, AppError> {
    if logical_cells.len() != policies.len() || logical_cells.len() != default_selects.len() {
        return Err(AppError::Internal(format!(
            "candidate batch input length mismatch: cells={} policies={} defaults={}",
            logical_cells.len(),
            policies.len(),
            default_selects.len()
        )));
    }

    let safe_max_items = if batch_max_items == 0 { 50 } else { batch_max_items };
    let safe_max_chars = if batch_max_chars == 0 { 3000 } else { batch_max_chars };

    println!(
        "[BATCH][CORE1] start cells={} batch_max_items={} batch_max_chars={}",
        logical_cells.len(),
        safe_max_items,
        safe_max_chars
    );

    // candidate1/2/3 を thread::scope で並列実行
    // thread::scope はスコープ内スレッドの終了を保証するため 'static 不要
    let (candidate1_results, candidate2_results, candidate3_results) =
        std::thread::scope(|s| {
            let h1 = if candidate_plan.is_enabled(1) {
                Some(s.spawn(|| {
                    Some(translate_segments_for_cells(
                        "candidate1",
                        logical_cells,
                        policies,
                        adapter1,
                        safe_max_items,
                        safe_max_chars,
                        from_lang,
                        to_lang,
                        direction,
                    ))
                }))
            } else {
                println!("[JOB_PLAN][candidate1] disabled");
                None
            };

            let h2 = if candidate_plan.is_enabled(2) {
                Some(s.spawn(|| {
                    Some(translate_segments_for_cells(
                        "candidate2",
                        logical_cells,
                        policies,
                        adapter2,
                        safe_max_items,
                        safe_max_chars,
                        from_lang,
                        to_lang,
                        direction,
                    ))
                }))
            } else {
                println!("[JOB_PLAN][candidate2] disabled");
                None
            };

            let h3 = if candidate_plan.is_enabled(3) {
                Some(s.spawn(|| {
                    Some(translate_whole_for_cells(
                        "candidate3",
                        logical_cells,
                        policies,
                        adapter3,
                        safe_max_items,
                        safe_max_chars,
                        from_lang,
                        to_lang,
                    ))
                }))
            } else {
                println!("[JOB_PLAN][candidate3] disabled");
                None
            };

            let r1 = h1.map(|h| h.join().unwrap_or(None)).unwrap_or(None);
            let r2 = h2.map(|h| h.join().unwrap_or(None)).unwrap_or(None);
            let r3 = h3.map(|h| h.join().unwrap_or(None)).unwrap_or(None);
            (r1, r2, r3)
        });

    let mut success_candidates = 0usize;
    let mut total_candidates = 0usize;

    let mut bundles = Vec::with_capacity(logical_cells.len());

    for idx in 0..logical_cells.len() {
        let logical_cell = &logical_cells[idx];
        let policy = &policies[idx];

        if !policy.translate_candidates {
            bundles.push(CandidateBundle {
                logical_cell_id: logical_cell.logical_cell_id.clone(),
                original: logical_cell.source_text.clone(),
                candidate1: None,
                candidate2: None,
                candidate3: None,
                default_select: default_selects[idx],
                alarms: CandidateAlarms::default(),
                note: policy.note.clone(),
            });
            continue;
        }

        let cand1 = candidate1_results.as_ref().map(|values| values[idx].clone());
        let cand2 = candidate2_results.as_ref().map(|values| values[idx].clone());
        let cand3 = candidate3_results.as_ref().map(|values| values[idx].clone());

        for cand in [&cand1, &cand2, &cand3] {
            if let Some(result) = cand {
                total_candidates += 1;
                if result.is_ok() {
                    success_candidates += 1;
                }
            }
        }

        diag_log(
            logical_cell,
            cand1.as_ref().unwrap_or(&Ok(String::new())),
            cand2.as_ref().unwrap_or(&Ok(String::new())),
            cand3.as_ref().unwrap_or(&Ok(String::new())),
            direction,
        );

        let default_select = decide_fallback_default_select(
            default_selects[idx],
            candidate_plan,
            cand1.as_ref(),
            cand2.as_ref(),
            cand3.as_ref(),
        );

        bundles.push(CandidateBundle {
            logical_cell_id: logical_cell.logical_cell_id.clone(),
            original: logical_cell.source_text.clone(),
            candidate1: cand1.as_ref().and_then(|v| v.as_ref().ok().cloned()),
            candidate2: cand2.as_ref().and_then(|v| v.as_ref().ok().cloned()),
            candidate3: cand3.as_ref().and_then(|v| v.as_ref().ok().cloned()),
            default_select,
            alarms: CandidateAlarms {
                candidate1_alarm: cand1.and_then(|v| v.err()),
                candidate2_alarm: cand2.and_then(|v| v.err()),
                candidate3_alarm: cand3.and_then(|v| v.err()),
            },
            note: policy.note.clone(),
        });
    }

    let success_rate = if total_candidates == 0 {
        1.0
    } else {
        success_candidates as f64 / total_candidates as f64
    };
    let job_status = if success_rate >= candidate_plan.job_accept_threshold {
        "JOB_ACCEPTED"
    } else {
        "JOB_UNSTABLE"
    };

    println!(
        "[JOB_PLAN][RESULT] success_candidates={} total_candidates={} success_rate={:.4} threshold={:.2} status={}",
        success_candidates,
        total_candidates,
        success_rate,
        candidate_plan.job_accept_threshold,
        job_status
    );
    println!("[BATCH][CORE1] end bundles={}", bundles.len());
    Ok(bundles)
}

fn decide_fallback_default_select(
    base_default: DefaultSelect,
    candidate_plan: &CandidateGenerationPlan,
    cand1: Option<&Result<String, String>>,
    cand2: Option<&Result<String, String>>,
    cand3: Option<&Result<String, String>>,
) -> DefaultSelect {
    if base_default == DefaultSelect::Original {
        return DefaultSelect::Original;
    }

    for candidate_no in candidate_plan.priority() {
        let success = match candidate_no {
            1 => cand1.map(|v| v.is_ok()).unwrap_or(false),
            2 => cand2.map(|v| v.is_ok()).unwrap_or(false),
            3 => cand3.map(|v| v.is_ok()).unwrap_or(false),
            _ => false,
        };

        if success {
            return match candidate_no {
                1 => DefaultSelect::Candidate1,
                2 => DefaultSelect::Candidate2,
                3 => DefaultSelect::Candidate3,
                _ => DefaultSelect::Original,
            };
        }
    }

    DefaultSelect::Original
}

fn translate_segments_for_cells(
    label: &str,
    logical_cells: &[LogicalCell],
    policies: &[TranslationPolicyDecision],
    adapter: &dyn TranslatorAdapter,
    batch_max_items: usize,
    batch_max_chars: usize,
    from_lang: Lang,
    to_lang: Lang,
    direction: &dyn DirectionProfile,
) -> Vec<Result<String, String>> {
    let mut results: Vec<Result<String, String>> = logical_cells
        .iter()
        .map(|cell| Ok(cell.source_text.clone()))
        .collect();

    let mut cell_plans: Vec<Option<SegmentCellPlan>> = vec![None; logical_cells.len()];
    let mut request_plans: Vec<SegmentRequestPlan> = Vec::new();

    for (cell_idx, logical_cell) in logical_cells.iter().enumerate() {
        if !policies[cell_idx].translate_candidates {
            continue;
        }

        let segments = split_preserving_structure(&logical_cell.source_text);
        let mut request_positions = Vec::new();

        for (segment_idx, seg) in segments.iter().enumerate() {
            if should_translate_segment(seg.target, &seg.text, direction) {
                request_positions.push(request_plans.len());
                // \n/全角スペースをPrivate Use Areaトークンで保護（翻訳エンジンが変換しないよう）
                let protected_text = seg.text
                    .replace('\n', "\u{E001}NL\u{E002}")
                    .replace('\u{3000}', "\u{E001}FS\u{E002}");
                request_plans.push(SegmentRequestPlan {
                    segment_idx,
                    text: protected_text,
                });
            }
        }

        if request_positions.is_empty() {
            results[cell_idx] = Ok(logical_cell.source_text.clone());
        } else {
            cell_plans[cell_idx] = Some(SegmentCellPlan {
                original_text: logical_cell.source_text.clone(),
                segments,
                request_positions,
            });
        }
    }

    println!(
        "[BATCH][{}][{}] collected_requests={}",
        label,
        adapter.provider_kind().as_label(),
        request_plans.len()
    );

    if request_plans.is_empty() {
        return results;
    }

    let mut translated_texts: Vec<Option<String>> = vec![None; request_plans.len()];
    let mut request_errors: Vec<Option<String>> = vec![None; request_plans.len()];

    let batches = build_segment_batches(&request_plans, batch_max_items, batch_max_chars);

    for (batch_idx, batch_range) in batches.into_iter().enumerate() {
        let start = batch_range.start;
        let end = batch_range.end;
        let flush_reason = batch_range.reason;
        let requests: Vec<TranslateRequest> = request_plans[start..end]
            .iter()
            .enumerate()
            .map(|(offset, plan)| TranslateRequest {
                request_id: format!("{}-{}-{}", label, batch_idx + 1, offset + 1),
                provider: adapter.provider_kind(),
                text: plan.text.clone(),
                from_lang,
                to_lang,
                timeout_ms: 1000,
            })
            .collect();

        let char_count: usize = requests.iter().map(|r| r.text.chars().count()).sum();
        println!(
            "[BATCH][{}][{}] flush reason={} batch={} count={} chars={}",
            label,
            adapter.provider_kind().as_label(),
            flush_reason.as_label(),
            batch_idx + 1,
            requests.len(),
            char_count
        );

        match adapter.translate_batch(&requests) {
            Ok(translations) => {
                if translations.len() != requests.len() {
                    let msg = format!(
                        "segment translation count mismatch: requests={} responses={}",
                        requests.len(),
                        translations.len()
                    );
                    for idx in start..end {
                        request_errors[idx] = Some(msg.clone());
                    }
                    continue;
                }

                for (idx, trans) in (start..end).zip(translations.into_iter()) {
                    translated_texts[idx] = Some(trans.translated_text);
                }
            }
            Err(e) => {
                let msg = e.message;
                println!(
                    "[BATCH][{}][{}][ERROR] batch={} count={} message={}",
                    label,
                    adapter.provider_kind().as_label(),
                    batch_idx + 1,
                    requests.len(),
                    msg
                );
                for idx in start..end {
                    request_errors[idx] = Some(msg.clone());
                }
            }
        }
    }

    for (cell_idx, plan_opt) in cell_plans.into_iter().enumerate() {
        let Some(plan) = plan_opt else {
            continue;
        };

        let mut rebuilt = String::new();
        let mut cell_error: Option<String> = None;

        for (segment_idx, seg) in plan.segments.iter().enumerate() {
            if should_translate_segment(seg.target, &seg.text, direction) {
                let req_pos = plan
                    .request_positions
                    .iter()
                    .find(|&&pos| request_plans[pos].segment_idx == segment_idx)
                    .copied();

                let Some(req_pos) = req_pos else {
                    cell_error = Some("segment request missing".to_string());
                    break;
                };

                if let Some(err) = &request_errors[req_pos] {
                    cell_error = Some(err.clone());
                    break;
                }

                match &translated_texts[req_pos] {
                    Some(text) => {
                        // トークンを元の改行・全角スペースに復元
                        let restored = text
                            .replace("\u{E001}NL\u{E002}", "\n")
                            .replace("\u{E001}FS\u{E002}", "\u{3000}");
                        rebuilt.push_str(&restored);
                    }
                    None => {
                        cell_error = Some("segment translation missing".to_string());
                        break;
                    }
                }
            } else {
                rebuilt.push_str(&seg.text);
            }
        }

        results[cell_idx] = match cell_error {
            Some(err) => Err(err),
            None => Ok(rebuilt),
        };

        if let Err(err) = &results[cell_idx] {
            println!(
                "[BATCH][{}][{}][CELL_ERROR] cell={} original={} error={}",
                label,
                adapter.provider_kind().as_label(),
                logical_cells[cell_idx].anchor_address,
                plan.original_text,
                err
            );
        }
    }

    results
}

// No.1 fix: candidate3（whole 経路）でも改行・全角スペースを保護する。
// candidate1/2（segments 経路, analyzer.rs 内）と同一の PUA トークンを使用し、
// MT エンジンが「\n + 全角スペース」を「\n\n」に変換して改行倍増・U+3000消失する不具合を防ぐ。
fn protect_structure_for_whole(text: &str) -> String {
    text.replace('\n', "\u{E001}NL\u{E002}")
        .replace('\u{3000}', "\u{E001}FS\u{E002}")
}

fn restore_structure_for_whole(text: &str) -> String {
    text.replace("\u{E001}NL\u{E002}", "\n")
        .replace("\u{E001}FS\u{E002}", "\u{3000}")
}

fn translate_whole_for_cells(
    label: &str,
    logical_cells: &[LogicalCell],
    policies: &[TranslationPolicyDecision],
    adapter: &dyn TranslatorAdapter,
    batch_max_items: usize,
    batch_max_chars: usize,
    from_lang: Lang,
    to_lang: Lang,
) -> Vec<Result<String, String>> {
    let mut results: Vec<Result<String, String>> = logical_cells
        .iter()
        .map(|cell| Ok(cell.source_text.clone()))
        .collect();

    let mut request_plans: Vec<WholeRequestPlan> = Vec::new();

    for (cell_idx, logical_cell) in logical_cells.iter().enumerate() {
        if !policies[cell_idx].translate_candidates {
            continue;
        }

        // F-2: 数式セルは構文を翻訳機へ渡さない。後段でリテラルのみ翻訳して再合成する。
        if is_formula_cell(logical_cell) {
            continue;
        }

        request_plans.push(WholeRequestPlan {
            cell_idx,
            // 送信前に改行・全角スペースを退避（候補1/2 と同じ保護）
            text: protect_structure_for_whole(&logical_cell.source_text),
        });
    }

    println!(
        "[BATCH][{}][{}] collected_requests={}",
        label,
        adapter.provider_kind().as_label(),
        request_plans.len()
    );

    if request_plans.is_empty() {
        // 通常whole対象が無くても、数式セルのリテラル翻訳は必ず実行する（F-2）
        translate_formula_literals(
            label,
            logical_cells,
            policies,
            adapter,
            batch_max_items,
            batch_max_chars,
            from_lang,
            to_lang,
            &mut results,
        );
        return results;
    }

    let batches = build_whole_batches(&request_plans, batch_max_items, batch_max_chars);

    for (batch_idx, batch_range) in batches.into_iter().enumerate() {
        let start = batch_range.start;
        let end = batch_range.end;
        let flush_reason = batch_range.reason;
        let requests: Vec<TranslateRequest> = request_plans[start..end]
            .iter()
            .enumerate()
            .map(|(offset, plan)| TranslateRequest {
                request_id: format!("{}-{}-{}", label, batch_idx + 1, offset + 1),
                provider: adapter.provider_kind(),
                text: plan.text.clone(),
                from_lang,
                to_lang,
                timeout_ms: 1000,
            })
            .collect();

        let char_count: usize = requests.iter().map(|r| r.text.chars().count()).sum();
        println!(
            "[BATCH][{}][{}] flush reason={} batch={} count={} chars={}",
            label,
            adapter.provider_kind().as_label(),
            flush_reason.as_label(),
            batch_idx + 1,
            requests.len(),
            char_count
        );

        match adapter.translate_batch(&requests) {
            Ok(translations) => {
                if translations.len() != requests.len() {
                    let msg = format!(
                        "whole translation count mismatch: requests={} responses={}",
                        requests.len(),
                        translations.len()
                    );
                    for plan in &request_plans[start..end] {
                        results[plan.cell_idx] = Err(msg.clone());
                    }
                    continue;
                }

                for (plan, trans) in request_plans[start..end].iter().zip(translations.into_iter()) {
                    // 受信後に退避していた改行・全角スペースを復元
                    results[plan.cell_idx] = Ok(restore_structure_for_whole(&trans.translated_text));
                }
            }
            Err(e) => {
                let msg = e.message;
                println!(
                    "[BATCH][{}][{}][ERROR] batch={} count={} message={}",
                    label,
                    adapter.provider_kind().as_label(),
                    batch_idx + 1,
                    requests.len(),
                    msg
                );
                for plan in &request_plans[start..end] {
                    results[plan.cell_idx] = Err(msg.clone());
                }
            }
        }
    }

    // F-2: 数式セルはリテラルのみ翻訳して再合成する（構文は翻訳機へ渡さない）
    translate_formula_literals(
        label,
        logical_cells,
        policies,
        adapter,
        batch_max_items,
        batch_max_chars,
        from_lang,
        to_lang,
        &mut results,
    );

    results
}

/// F-2: 数式セル（CellKind=Formula 系）かどうか。
/// FormulaRaw と SharedFormulaParent を数式として扱う。
fn is_formula_cell(cell: &LogicalCell) -> bool {
    matches!(
        cell.cell_kind,
        LogicalCellKind::FormulaRaw | LogicalCellKind::SharedFormulaParent
    )
}

/// F-2: 数式セルのリテラルだけを翻訳して数式を再合成する。
/// 数式構文（=, 関数, 参照, 括弧, 区切り）は翻訳機へ渡さない。
#[allow(clippy::too_many_arguments)]
fn translate_formula_literals(
    label: &str,
    logical_cells: &[LogicalCell],
    policies: &[TranslationPolicyDecision],
    adapter: &dyn TranslatorAdapter,
    batch_max_items: usize,
    batch_max_chars: usize,
    from_lang: Lang,
    to_lang: Lang,
    results: &mut [Result<String, String>],
) {
    struct CellTpl {
        cell_idx: usize,
        template: String,
        lit_start: usize,
        lit_count: usize,
    }

    let mut cell_tpls: Vec<CellTpl> = Vec::new();
    let mut flat_literals: Vec<String> = Vec::new();

    for (cell_idx, logical_cell) in logical_cells.iter().enumerate() {
        if !policies[cell_idx].translate_candidates {
            continue;
        }
        if !is_formula_cell(logical_cell) {
            continue;
        }

        let (template, lits) = extract_quoted_literals(&logical_cell.source_text);
        if lits.is_empty() {
            // リテラルなし数式 → 翻訳機を呼ばず原文維持
            results[cell_idx] = Ok(logical_cell.source_text.clone());
            continue;
        }

        let lit_start = flat_literals.len();
        let lit_count = lits.len();
        for lit in lits {
            flat_literals.push(lit);
        }
        cell_tpls.push(CellTpl {
            cell_idx,
            template,
            lit_start,
            lit_count,
        });
    }

    if flat_literals.is_empty() {
        return;
    }

    // リテラルを平坦な翻訳キューに（WholeRequestPlan.cell_idx は平坦インデックスとして使う）
    let lit_plans: Vec<WholeRequestPlan> = flat_literals
        .iter()
        .enumerate()
        .map(|(i, lit)| WholeRequestPlan {
            cell_idx: i,
            text: lit.clone(),
        })
        .collect();

    let mut translated_literals: Vec<Option<String>> = vec![None; flat_literals.len()];

    let batches = build_whole_batches(&lit_plans, batch_max_items, batch_max_chars);
    for (batch_idx, batch_range) in batches.into_iter().enumerate() {
        let start = batch_range.start;
        let end = batch_range.end;
        let requests: Vec<TranslateRequest> = lit_plans[start..end]
            .iter()
            .enumerate()
            .map(|(offset, plan)| TranslateRequest {
                request_id: format!("{}-lit-{}-{}", label, batch_idx + 1, offset + 1),
                provider: adapter.provider_kind(),
                text: plan.text.clone(),
                from_lang,
                to_lang,
                timeout_ms: 1000,
            })
            .collect();

        match adapter.translate_batch(&requests) {
            Ok(translations) => {
                if translations.len() != requests.len() {
                    // 数不一致のバッチは未翻訳のまま（原文リテラルを維持）
                    continue;
                }
                for (plan, trans) in lit_plans[start..end].iter().zip(translations.into_iter()) {
                    translated_literals[plan.cell_idx] = Some(trans.translated_text);
                }
            }
            Err(_e) => {
                // バッチ失敗は未翻訳のまま（原文リテラルを維持）
                continue;
            }
        }
    }

    // 数式ごとに再合成（翻訳できなかったリテラルは原文を使う）
    for tpl in &cell_tpls {
        let mut lits: Vec<String> = Vec::with_capacity(tpl.lit_count);
        for k in 0..tpl.lit_count {
            let gi = tpl.lit_start + k;
            let lit = translated_literals[gi]
                .clone()
                .unwrap_or_else(|| flat_literals[gi].clone());
            lits.push(lit);
        }
        results[tpl.cell_idx] = Ok(reassemble_formula(&tpl.template, &lits));
    }
}

#[derive(Debug, Clone, Copy)]
struct BatchRange {
    start: usize,
    end: usize,
    reason: FlushReason,
}

#[derive(Debug, Clone, Copy)]
enum FlushReason {
    BatchLimit,
    CharLimit,
    EndOfInput,
}

impl FlushReason {
    fn as_label(self) -> &'static str {
        match self {
            FlushReason::BatchLimit => "batch_limit",
            FlushReason::CharLimit => "char_limit",
            FlushReason::EndOfInput => "end_of_input",
        }
    }
}

fn build_segment_batches(
    plans: &[SegmentRequestPlan],
    batch_max_items: usize,
    batch_max_chars: usize,
) -> Vec<BatchRange> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut count = 0usize;
    let mut chars = 0usize;

    for (idx, plan) in plans.iter().enumerate() {
        let text_chars = plan.text.chars().count().max(1);
        let would_exceed_items = count > 0 && count + 1 > batch_max_items;
        let would_exceed_chars = count > 0 && chars + text_chars > batch_max_chars;

        if would_exceed_items || would_exceed_chars {
            let reason = if would_exceed_items {
                FlushReason::BatchLimit
            } else {
                FlushReason::CharLimit
            };

            out.push(BatchRange {
                start,
                end: idx,
                reason,
            });

            start = idx;
            count = 0;
            chars = 0;
        }

        count += 1;
        chars += text_chars;
    }

    if count > 0 {
        out.push(BatchRange {
            start,
            end: plans.len(),
            reason: FlushReason::EndOfInput,
        });
    }

    out
}

fn build_whole_batches(
    plans: &[WholeRequestPlan],
    batch_max_items: usize,
    batch_max_chars: usize,
) -> Vec<BatchRange> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut count = 0usize;
    let mut chars = 0usize;

    for (idx, plan) in plans.iter().enumerate() {
        let text_chars = plan.text.chars().count().max(1);
        let would_exceed_items = count > 0 && count + 1 > batch_max_items;
        let would_exceed_chars = count > 0 && chars + text_chars > batch_max_chars;

        if would_exceed_items || would_exceed_chars {
            let reason = if would_exceed_items {
                FlushReason::BatchLimit
            } else {
                FlushReason::CharLimit
            };

            out.push(BatchRange {
                start,
                end: idx,
                reason,
            });

            start = idx;
            count = 0;
            chars = 0;
        }

        count += 1;
        chars += text_chars;
    }

    if count > 0 {
        out.push(BatchRange {
            start,
            end: plans.len(),
            reason: FlushReason::EndOfInput,
        });
    }

    out
}

fn should_translate_segment(
    seg_target: bool,
    text: &str,
    direction: &dyn DirectionProfile,
) -> bool {
    if !seg_target {
        return false;
    }

    let st = analyze_text_structure(text);
    // 段階2(contains_japanese_like)・段階3(閾値)を direction に統合。
    // ja2zh では kanji>=1 || kana>=3。実コード同値証明により現行挙動と一致。
    direction.should_translate_by_text_structure(&st)
}

fn diag_log(
    logical_cell: &LogicalCell,
    seg1: &Result<String, String>,
    seg2: &Result<String, String>,
    whole3: &Result<String, String>,
    direction: &dyn DirectionProfile,
) {
    if logical_cell.anchor_address != "A2" && logical_cell.anchor_address != "A7" {
        return;
    }

    let path = std::env::var("ETB_DIAG_PATH").unwrap_or_else(|_| "etb_diag_generate.txt".to_string());
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "=== {} {} {:?} ===", logical_cell.sheet_name, logical_cell.anchor_address, logical_cell.cell_kind);
        let _ = writeln!(f, "SOURCE={}", logical_cell.source_text);
        let segs = split_preserving_structure(&logical_cell.source_text);
        for (i, seg) in segs.iter().enumerate() {
            let _ = writeln!(
                f,
                "SEG[{i}] target={} send={} text={}",
                seg.target,
                should_translate_segment(seg.target, &seg.text, direction),
                seg.text
            );
        }
        let _ = writeln!(f, "CAND1={:?}", seg1);
        let _ = writeln!(f, "CAND2={:?}", seg2);
        let _ = writeln!(f, "CAND3={:?}", whole3);
        let _ = writeln!(f);
    }
}
