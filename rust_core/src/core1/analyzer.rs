use crate::adapters::translator_trait::TranslatorAdapter;
use crate::adapters::types::{Lang, TranslateRequest};
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
use crate::entry::job_plan_settings::Method;
use crate::infra::app_error::AppError;
use std::fs::OpenOptions;
use std::io::Write;

#[derive(Debug, Clone)]
struct SegmentRequestPlan {
    segment_idx: usize,
    text: String,
    token_idx: usize,
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
    token_idx: usize,
}


#[derive(Debug, Clone)]
pub struct CandidateGenerationPlan {
    pub enabled_candidates: Vec<u8>,
    pub default_candidate_priority: Vec<u8>,
    pub job_accept_threshold: f64,
    // C-3/C-4: 候補ごとの翻訳方式（split/whole）。CandidateConfig.method 由来。
    // 既定は 1=split, 2=split, 3=whole（従来挙動）。
    pub methods: [Method; 3],
}

impl Default for CandidateGenerationPlan {
    fn default() -> Self {
        Self {
            enabled_candidates: vec![1, 2, 3],
            default_candidate_priority: vec![1, 2, 3],
            job_accept_threshold: 0.80,
            methods: [
                Method::default_for_index(1),
                Method::default_for_index(2),
                Method::default_for_index(3),
            ],
        }
    }
}

impl CandidateGenerationPlan {
    fn is_enabled(&self, candidate_no: u8) -> bool {
        self.enabled_candidates.contains(&candidate_no)
    }

    /// C-3/C-4: 候補番号 -> 翻訳方式（CandidateConfig.method 由来）。
    fn method(&self, candidate_no: u8) -> Method {
        match candidate_no {
            1 => self.methods[0],
            2 => self.methods[1],
            3 => self.methods[2],
            _ => Method::default_for_index(candidate_no),
        }
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

        // C-4: 候補ごとに method で見積り方式を選ぶ（Generate の実行方式と一致させる）。
        // split → estimate_segment_requests / whole → estimate_whole_requests。
        for candidate_no in 1u8..=3 {
            if !candidate_plan.is_enabled(candidate_no) {
                continue;
            }
            out.candidate_units += 1;
            let (requests, chars) = match candidate_plan.method(candidate_no) {
                Method::Split => estimate_segment_requests(&logical_cell.source_text, direction),
                Method::Whole => estimate_whole_requests(logical_cell),
            };
            match candidate_no {
                1 => { out.candidate1_requests += requests; out.candidate1_chars += chars; }
                2 => { out.candidate2_requests += requests; out.candidate2_chars += chars; }
                3 => { out.candidate3_requests += requests; out.candidate3_chars += chars; }
                _ => {}
            }
        }
    }

    Ok(out)
}

/// C-4: 文脈（whole）方式の見積り。candidate3 で固定だった数え方をヘルパ化。
/// F-2 整合: 数式セルは引用符内リテラルのみ（無しは 0/0）、非数式は全文1リクエスト。
fn estimate_whole_requests(logical_cell: &LogicalCell) -> (usize, usize) {
    if is_formula_cell(logical_cell) {
        let (_template, literals) = extract_quoted_literals(&logical_cell.source_text);
        if literals.is_empty() {
            (0, 0)
        } else {
            let chars = literals.iter().map(|l| l.chars().count()).sum::<usize>();
            (literals.len(), chars)
        }
    } else {
        (1, logical_cell.source_text.chars().count())
    }
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
                let m1 = candidate_plan.method(1);
                Some(s.spawn(move || {
                    Some(run_candidate_by_method(
                        "candidate1",
                        m1,
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
                let m2 = candidate_plan.method(2);
                Some(s.spawn(move || {
                    Some(run_candidate_by_method(
                        "candidate2",
                        m2,
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
                let m3 = candidate_plan.method(3);
                Some(s.spawn(move || {
                    Some(run_candidate_by_method(
                        "candidate3",
                        m3,
                        logical_cells,
                        policies,
                        adapter3,
                        safe_max_items,
                        safe_max_chars,
                        from_lang,
                        to_lang,
                        direction,
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

/// C-3: 候補の翻訳方式（split/whole）に応じて実処理関数をディスパッチする。
/// split=分割（direction を使う）/ whole=文脈（direction 不要）。
/// 戻り型は両関数とも Vec<Result<String, String>> で共通。
fn run_candidate_by_method(
    label: &str,
    method: Method,
    logical_cells: &[LogicalCell],
    policies: &[TranslationPolicyDecision],
    adapter: &dyn TranslatorAdapter,
    batch_max_items: usize,
    batch_max_chars: usize,
    from_lang: Lang,
    to_lang: Lang,
    direction: &dyn DirectionProfile,
) -> Vec<Result<String, String>> {
    match method {
        Method::Split => {
            println!("[ANALYZER][{label}] method=split");
            translate_segments_for_cells(
                label, logical_cells, policies, adapter,
                batch_max_items, batch_max_chars, from_lang, to_lang, direction,
            )
        }
        Method::Whole => {
            println!("[ANALYZER][{label}] method=whole");
            translate_whole_for_cells(
                label, logical_cells, policies, adapter,
                batch_max_items, batch_max_chars, from_lang, to_lang,
            )
        }
    }
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

        let segments = split_preserving_structure(&normalize_fullwidth_space(&logical_cell.source_text));
        let mut request_positions = Vec::new();
        let mut cell_token_error = false;

        for (segment_idx, seg) in segments.iter().enumerate() {
            if should_translate_segment(seg.target, &seg.text, direction) {
                // 衝突しない退避トークン表を選ぶ。全滅なら誤復元防止のため中断（エラー）。
                let token_idx = match select_token_table(&seg.text) {
                    Some(i) => i,
                    None => {
                        println!("[SPLIT][{label}][TOKEN_COLLISION_ABORT] cell_idx={cell_idx} segment_idx={segment_idx}");
                        cell_token_error = true;
                        break;
                    }
                };
                request_positions.push(request_plans.len());
                let protected_text = protect_with(&seg.text, token_idx);
                request_plans.push(SegmentRequestPlan {
                    segment_idx,
                    text: protected_text,
                    token_idx,
                });
            }
        }

        if cell_token_error {
            results[cell_idx] = Err("SPECIAL_TOKEN_COLLISION".to_string());
        } else if request_positions.is_empty() {
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
                        // トークンを元の改行・全角スペースに復元（退避時と同一テーブル）
                        let restored = restore_with(text, request_plans[req_pos].token_idx);
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
            None => {
                let normalized = normalize_punctuation_for_target(&rebuilt, to_lang);
                match detect_token_leak(&logical_cells[cell_idx].source_text, &normalized) {
                    Some(leak_msg) => {
                        println!(
                            "[SPLIT][{label}][TOKEN_LEAK] cell_idx={cell_idx} {leak_msg}"
                        );
                        Err(leak_msg)
                    }
                    None => Ok(normalized),
                }
            }
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

// No.1 fix: candidate3（whole 経路）でも改行を保護する。
// candidate1/2（segments 経路, analyzer.rs 内）と同一の PUA トークンを使用し、
// MT エンジンが改行を変形・消失させる不具合を防ぐ。
// ============================================================
// 特殊文字（改行）保護の共通処理（QA-2026-017 要求3）
//
// 翻訳エンジンに本文を渡すと改行が変形・消失する。
// 固有トークンへ退避し翻訳後に復元する。Google/Amazon/DeepL 共通。
//
// 旧実装は私用領域文字 U+E001/E002 で囲んでいたが、Google/Amazon は
// これを翻訳時に削除し中身の "NL"/"FS" だけ残す不具合があった（whole経路）。
// そこで数学用山括弧 U+27E6/U+27E7 等で囲む固有トークンに変更したが、
// QA-2026-020で同種の漏れ（囲み記号だけ剥がされ中身が残る）が再発することを確認。
// 仕様変更(2026-07-03)：全角スペースは保護対象から除外し（正確な個数・全半角の
// 区別に翻訳先言語では意味がないため）、改行のみを保護対象とした上で、
// 復元漏れは detect_token_leak() で検知しフォールバックする方針に変更。
// 素の "NL" 単独は本文と衝突するため必ず囲み付きにする。
// 特殊文字の追加は対応表（TokenTable）に1行足すだけで全エンジンに反映。
// ============================================================

/// 退避トークン表（候補）。条件:
/// - Google/Amazon/DeepL が削除・翻訳しない
/// - 本文と衝突しない
/// - 復元で一意判定できる
/// 実装時に3エンジンで実証し最終決定する。TABLE[0] を既定の単一真実源とする。
type TokenTable = &'static [(char, &'static str)];
// 仕様変更(2026-07-03): 全角スペース(U+3000)は保護対象から除外した。
// 理由：スペースの正確な個数・全角半角は翻訳先言語では意味を持たず、保護する
// 価値がない一方、保護記号(⟦⦃〚)が翻訳エンジンに破損させられ「FS」という
// ガベージが混入する不具合の原因になっていた（QA-2026-020）。
// 改行(\n)は箇条書き等の構造として意味を持つため、引き続き保護・復元する。
const SPECIAL_CHAR_TOKENS: TokenTable = &[
    ('\n', "\u{27E6}NL\u{27E7}"),
];
const TOKEN_TABLES: &[TokenTable] = &[
    SPECIAL_CHAR_TOKENS,
    &[('\n', "\u{2983}NL\u{2984}")],
    &[('\n', "\u{301A}NL\u{301B}")],
];

/// 本文に当該テーブルのトークンが含まれるか（衝突）。
fn table_collides(text: &str, table: TokenTable) -> bool {
    table.iter().any(|(_, tok)| text.contains(tok))
}

/// 本文と衝突しないテーブルを選ぶ。全テーブル衝突なら None（誤復元防止のため呼び出し側で中断）。
fn select_token_table(text: &str) -> Option<usize> {
    TOKEN_TABLES.iter().position(|t| !table_collides(text, t))
}

/// 保護トークン漏れ検知（QA-2026-020 / 仕様書対応）。
/// 「囲み記号(⟦⦃〚等)だけ翻訳エンジンに剥がされ、中身のNLが生テキストとして
/// 残る」という既知の破損パターンを、原文と復元後テキストの \n の
/// 個数比較で検知する。正規表現でNL等の文字列パターンを推測するより、
/// 実際に復元できた個数を直接数える方が、大文字小文字ゆれ（NL/nL等）や
/// 綴りゆれに影響されず確実。個数が原文に満たなければ復元漏れと断定できる。
/// （全角スペースは仕様変更により保護対象外のため判定しない）
fn detect_token_leak(source_text: &str, restored_text: &str) -> Option<String> {
    let src_nl = source_text.matches('\n').count();
    let out_nl = restored_text.matches('\n').count();

    if out_nl < src_nl {
        Some(format!(
            "保護トークン復元漏れの疑い（改行 {}/{} 件のみ復元）。翻訳エンジンが保護記号を破損させた可能性があります。",
            out_nl, src_nl
        ))
    } else {
        None
    }
}


/// 全角スペース(U+3000)を半角スペースへ正規化する（QA-2026-024対応）。
/// split方式は日本語部分と非日本語部分（スペース等）を先に分割してから
/// 翻訳対象部分だけprotect_with()を通すため、非翻訳セグメントに含まれる
/// 全角スペースはprotect_with()を経由せず素通りしてしまう。これを防ぐため、
/// セグメント分割より前に、セル全体（翻訳対象・対象外を問わず）に対して
/// 本関数を適用する。whole方式はprotect_with()内の変換のみで従来通り足りるが、
/// 二重適用しても副作用はないため一本化している。
fn normalize_fullwidth_space(text: &str) -> String {
    text.replace('\u{3000}', " ")
}

/// 指定テーブルで退避。
/// 全角スペース(U+3000)は保護せず、送信前に半角スペース1個へ正規化する
/// （個数・全角半角の区別は翻訳先言語では意味を持たないため。仕様変更2026-07-03）。
/// これにより U+3000 という文字自体を翻訳エンジンへ送らない。
fn protect_with(text: &str, idx: usize) -> String {
    let mut out = normalize_fullwidth_space(text);
    for (ch, tok) in TOKEN_TABLES[idx] {
        out = out.replace(*ch, tok);
    }
    out
}

/// 指定テーブルで復元。
fn restore_with(text: &str, idx: usize) -> String {
    let mut out = text.to_string();
    for (ch, tok) in TOKEN_TABLES[idx] {
        out = out.replace(tok, &ch.to_string());
    }
    out
}

/// QA-017要求5: 中→日(to_lang=Ja)の記号正規化（後処理）。
/// 中国語の全角コンマ「，」は日本語では「、」が自然。to_lang=Ja のときのみ適用。
/// 日→中(ja2zh)には適用しない。
fn normalize_punctuation_for_target(text: &str, to_lang: Lang) -> String {
    if to_lang == Lang::Ja {
        text.replace('\u{FF0C}', "\u{3001}") // ， -> 、
    } else {
        text.to_string()
    }
}

/// QA-017要求3: NL救済復元。トークン完全一致復元に失敗した場合のみ働く。
/// 適用条件（全て満たす場合のみ）:
///   1. 原文に改行がある（src_nl > 0）
///   2. 原文に "NL" を含まない（本文中NLとの誤判定防止）
///   3. 復元後の改行数が原文より不足（restored_nl < src_nl）
/// 処理: 訳文中の "NL"（前後・間の空白数に依存しない "N␣*L"）を改行へ置換する。
/// 残留トークン囲み記号を除去する。エンジンが囲み（⟦⟧/⦃⦄/〚〛）だけ残した場合の保険。
fn strip_residual_token_brackets(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(*c,
            '\u{27E6}' | '\u{27E7}' | '\u{2983}' | '\u{2984}' | '\u{301A}' | '\u{301B}'))
        .collect()
}

fn rescue_restore_nl(restored: &str, source_text: &str) -> String {
    let src_nl = source_text.matches('\n').count();
    let cur_nl = restored.matches('\n').count();
    let source_has_nl = source_text.contains("NL");
    if src_nl == 0 || source_has_nl || cur_nl >= src_nl {
        return restored.to_string();
    }
    // "N" + 空白0個以上 + "L" を改行へ。前後空白も1個ずつ吸収（依存はしない）。
    let mut out = String::with_capacity(restored.len());
    let chars: Vec<char> = restored.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == 'N' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j] == ' ' || chars[j] == '\u{3000}') {
                j += 1;
            }
            if j < chars.len() && chars[j] == 'L' {
                // 直前に積んだ末尾空白を1つ落とす
                while out.ends_with(' ') || out.ends_with('\u{3000}') {
                    out.pop();
                }
                out.push('\n');
                let mut k = j + 1;
                // 直後の空白を1つだけ吸収
                if k < chars.len() && (chars[k] == ' ' || chars[k] == '\u{3000}') {
                    k += 1;
                }
                i = k;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
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

        // 衝突しない退避トークン表を選ぶ。全滅なら誤復元防止のため中断（エラー）。
        let token_idx = match select_token_table(&logical_cell.source_text) {
            Some(i) => i,
            None => {
                println!("[WHOLE][{label}][TOKEN_COLLISION_ABORT] cell_idx={cell_idx}");
                results[cell_idx] = Err("SPECIAL_TOKEN_COLLISION".to_string());
                continue;
            }
        };

        request_plans.push(WholeRequestPlan {
            cell_idx,
            text: protect_with(&logical_cell.source_text, token_idx),
            token_idx,
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
                    let mut restored = restore_with(&trans.translated_text, plan.token_idx);
                    let src_nl = logical_cells[plan.cell_idx].source_text.matches('\n').count();
                    // 完全一致復元で改行が不足した場合のみ NL救済復元を試みる。
                    if restored.matches('\n').count() < src_nl {
                        restored = rescue_restore_nl(&restored, &logical_cells[plan.cell_idx].source_text);
                    }
                    // エンジンが囲み記号だけ残した場合に備え、残留トークン囲み記号を除去する。
                    restored = strip_residual_token_brackets(&restored);
                    // 記号正規化（中→日 の ，→、）。
                    restored = normalize_punctuation_for_target(&restored, to_lang);
                    // 救済後も改行数が一致しない場合のみ MISMATCH を記録。
                    let out_nl = restored.matches('\n').count();
                    if src_nl != out_nl {
                        println!(
                            "[WHOLE][NL_RESTORE_MISMATCH] candidate={} engine={} cell_idx={} src_newlines={} restored_newlines={}",
                            label,
                            adapter.provider_kind().as_label(),
                            plan.cell_idx,
                            src_nl,
                            out_nl
                        );
                    }
                    match detect_token_leak(&logical_cells[plan.cell_idx].source_text, &restored) {
                        Some(leak_msg) => {
                            println!(
                                "[WHOLE][{}][TOKEN_LEAK] engine={} cell_idx={} {}",
                                label,
                                adapter.provider_kind().as_label(),
                                plan.cell_idx,
                                leak_msg
                            );
                            results[plan.cell_idx] = Err(leak_msg);
                        }
                        None => {
                            results[plan.cell_idx] = Ok(restored);
                        }
                    }
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
    // 各リテラルごとに使用する保護テーブル（衝突があれば None ＝保護なしで送る）。
    let mut flat_literal_token_idx: Vec<Option<usize>> = Vec::new();

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
            flat_literal_token_idx.push(select_token_table(&lit));
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
    // 全てのテーブルで衝突した場合（flat_literal_token_idx[i]==None）は token_idx=usize::MAX
    // を「保護なしでそのまま送る」の目印として使う（select_token_table の全表衝突は極めて稀）。
    let lit_plans: Vec<WholeRequestPlan> = flat_literals
        .iter()
        .enumerate()
        .map(|(i, lit)| {
            let idx = flat_literal_token_idx[i];
            WholeRequestPlan {
                cell_idx: i,
                text: match idx {
                    Some(idx) => protect_with(lit, idx),
                    None => lit.clone(),
                },
                token_idx: idx.unwrap_or(usize::MAX),
            }
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
                    if plan.token_idx == usize::MAX {
                        // 全テーブル衝突のため無保護で送ったリテラル。復元処理は不要。
                        translated_literals[plan.cell_idx] = Some(trans.translated_text);
                        continue;
                    }
                    let restored = restore_with(&trans.translated_text, plan.token_idx);
                    let original_lit = &flat_literals[plan.cell_idx];
                    match detect_token_leak(original_lit, &restored) {
                        Some(leak_msg) => {
                            println!(
                                "[FORMULA_LIT][{label}][TOKEN_LEAK] lit_idx={} {leak_msg}",
                                plan.cell_idx
                            );
                            // 検知時は他候補と同様、未翻訳（原文リテラル）へフォールバック。
                        }
                        None => {
                            translated_literals[plan.cell_idx] = Some(restored);
                        }
                    }
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

    // Phase 3.6: 計測は direction 内部へ移設。源語中立な &str を渡すだけ。
    // ja2zh では内部で kanji>=1 || kana>=3 を評価（現行挙動と一致）。
    direction.should_translate_by_text(text)
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
