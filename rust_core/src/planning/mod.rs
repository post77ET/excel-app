// ============================================================
// planning モジュール（Phase 1 で新設）
//
// 翻訳ジョブ全体の実行条件を表す ExecutionPlan を提供する。
// Phase 1 では最小構成として direction_id / billing_mode の2項目を持つ。
//
// Phase 1 の入力源（恒等マッピング）:
//   - direction_id : 環境変数 ETB_DIRECTION_ID があれば採用、無ければ既定 "ja2zh"
//   - billing_mode : 環境変数 ETB_BILLING_MODE があれば採用、無ければ
//                    既存 job_plan.mode から導出（experience->"free" / paid->"paid_standard"）
// いずれも未設定時は現行挙動と完全に一致する。
//
// Phase 6 で env 依存を撤去し、Web から渡される ExecutionPlan を唯一の入力源とする。
// ============================================================

use crate::direction::{self, DirectionProfile};
use crate::entry::job_plan_settings::{JobCourseMode, JobPlanSettings};
use crate::plan::{self, PlanPolicy};

#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub direction_id: String,
    pub billing_mode: String,
}

impl ExecutionPlan {
    /// Phase 1: 実行条件を確定する。未設定時は現行挙動と一致する恒等マッピング。
    pub fn from_runtime(job_plan: &JobPlanSettings) -> Self {
        let direction_id = std::env::var("ETB_DIRECTION_ID")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "ja2zh".to_string());

        let (billing_mode, billing_source) = match std::env::var("ETB_BILLING_MODE")
            .ok()
            .filter(|v| !v.trim().is_empty())
        {
            Some(v) => (v, "env"),
            None => {
                let derived = match job_plan.mode {
                    JobCourseMode::Experience => "free".to_string(),
                    JobCourseMode::Paid => "paid_standard".to_string(),
                };
                (derived, "derived_from_job_plan_mode")
            }
        };

        let direction_source = if std::env::var("ETB_DIRECTION_ID")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .is_some()
        {
            "env"
        } else {
            "default"
        };

        println!(
            "[EXECUTION_PLAN] direction_id={} (source={}) billing_mode={} (source={})",
            direction_id, direction_source, billing_mode, billing_source
        );

        ExecutionPlan {
            direction_id,
            billing_mode,
        }
    }

    pub fn resolve_direction(&self) -> Result<Box<dyn DirectionProfile>, String> {
        direction::resolve(&self.direction_id)
    }

    pub fn resolve_plan(&self) -> Result<Box<dyn PlanPolicy>, String> {
        plan::resolve(&self.billing_mode)
    }
}
