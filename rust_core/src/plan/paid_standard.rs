// 有料スタンダードプランポリシー。
// Phase 1 では現行 paid モードと完全に同一（範囲制限なし）。
// 課金処理は Phase 5 で billing() 系を実装する。Phase 1 では行わない。

use crate::plan::{CellScope, PlanPolicy};

pub struct PaidStandardPlan;

impl PlanPolicy for PaidStandardPlan {
    fn id(&self) -> &'static str {
        "paid_standard"
    }

    fn cell_scope(&self) -> CellScope {
        // 現行 paid モードと同一（範囲制限なし）。
        CellScope::Full
    }

    // billing_enabled() は Phase 1 では既定（false）のまま。
    // Phase 5 でここに pricing 連携を実装する。
}
