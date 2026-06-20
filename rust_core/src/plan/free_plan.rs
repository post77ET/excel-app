// 無料（体験版）プランポリシー。
// Phase 1 では現行 experience モードの範囲制限 A1:D5 と完全に同一。

use crate::entry::job_plan_settings::{EXPERIENCE_MAX_COL, EXPERIENCE_MAX_ROW};
use crate::plan::{CellScope, PlanPolicy};

pub struct FreePlan;

impl PlanPolicy for FreePlan {
    fn id(&self) -> &'static str {
        "free"
    }

    fn cell_scope(&self) -> CellScope {
        // 現行の体験版範囲（A1:D5）と同一の値を使う。
        CellScope::Range {
            max_row: EXPERIENCE_MAX_ROW,
            max_col: EXPERIENCE_MAX_COL,
        }
    }

    // billing は Phase 5 で実装。Phase 1 では既定（false）のまま。
}
