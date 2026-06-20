// ============================================================
// plan モジュール（Phase 1 で新設）
//
// プラン（無料/有料）ごとに変わる振る舞いを表現する差し込み口。
// Phase 1 では中身は現行挙動と完全に同一（恒等マッピング）。
//   - free  -> 現行 experience モードの範囲制限 A1:D5 と同一
//   - paid_standard -> 現行 paid モードと同一（範囲制限なし）
//   - 課金（billing）は Phase 1 では行わない。Phase 5 で実装。
//
// Phase 3 で、generate/estimate パイプラインに直書きされている
// 体験版範囲フィルタを本モジュール配下へ実移設する。
// ============================================================

pub mod free_plan;
pub mod paid_standard;

/// 翻訳対象セルの範囲制約。
/// 1始まりの (col, row) で判定する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellScope {
    /// 範囲制限なし（全セル対象）
    Full,
    /// A1 起点の矩形範囲に限定（max_col 列・max_row 行まで）
    Range { max_row: u32, max_col: u32 },
}

impl CellScope {
    /// 1始まりの (col, row) がスコープ内かどうか。
    pub fn contains(&self, col: u32, row: u32) -> bool {
        match *self {
            CellScope::Full => true,
            CellScope::Range { max_row, max_col } => {
                row >= 1 && row <= max_row && col >= 1 && col <= max_col
            }
        }
    }
}

pub trait PlanPolicy: Send + Sync {
    /// 正規化済みのプランID（例: "free", "paid_standard"）
    fn id(&self) -> &'static str;

    /// 翻訳対象セルの範囲制約
    fn cell_scope(&self) -> CellScope;

    /// 課金を行うか。Phase 1 では常に false（Phase 5 で実装）。
    fn billing_enabled(&self) -> bool {
        false
    }
}

/// billing_mode から PlanPolicy を解決する。
///
/// Phase 1 方針:
/// - "free"（experience 相当）は範囲 A1:D5、課金なし
/// - "paid_standard"（paid 相当）は全範囲、課金は Phase 5 まで未実装
/// - 未知の id は **エラー**（fail-fast）。既定値の適用は ExecutionPlan 側で行う。
///
/// 呼び出された事実は必ずログへ残す（resolve を本当に通った証跡）。
pub fn resolve(billing_mode: &str) -> Result<Box<dyn PlanPolicy>, String> {
    let normalized = billing_mode.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "free" | "experience" | "trial" => {
            println!(
                "[PLAN][resolve] billing_mode=\"{}\" -> policy=free",
                billing_mode
            );
            Ok(Box::new(free_plan::FreePlan))
        }
        "paid_standard" | "paid" | "full" => {
            println!(
                "[PLAN][resolve] billing_mode=\"{}\" -> policy=paid_standard",
                billing_mode
            );
            Ok(Box::new(paid_standard::PaidStandardPlan))
        }
        other => {
            println!(
                "[PLAN][resolve][ERROR] unknown billing_mode=\"{}\"",
                other
            );
            Err(format!(
                "unknown billing_mode=\"{}\" (サポート: free, paid_standard)",
                other
            ))
        }
    }
}
