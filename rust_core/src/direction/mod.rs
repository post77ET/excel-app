// ============================================================
// direction モジュール（Phase 1 で新設）
//
// 翻訳方向を「プロファイル」として表現する差し込み口。
// Phase 1 では中身は現行の JA→ZH 挙動と完全に同一（恒等マッピング）。
//   - resolve() が direction_id を受け取り、対応する DirectionProfile を返す
//   - 返したプロファイルの lang_pair() が実際の翻訳リクエスト構築に流れる
//
// Phase 2 で、analyzer 等に散っている方向依存ロジック（翻訳要否の閾値など）を
// 本モジュール配下へ実移設する。Phase 1 では移設しない。
// ============================================================

use crate::adapters::types::Lang;

pub mod ja_zh;

pub trait DirectionProfile: Send + Sync {
    /// 正規化済みの方向ID（例: "ja2zh"）
    fn id(&self) -> &'static str;

    /// (翻訳元, 翻訳先) の言語ペア
    fn lang_pair(&self) -> (Lang, Lang);
}

/// direction_id から DirectionProfile を解決する。
///
/// Phase 1 方針:
/// - "ja2zh"（別表記含む）は現行どおり (Ja, Zh) を返す
/// - 未知の id は **エラー**（fail-fast）。誤った翻訳方向を黙って既定へ戻すのは危険なため。
///   既定値の適用は ExecutionPlan::from_runtime 側（未指定時に "ja2zh"）で行い、
///   ここでは「明示的に渡された不正な値」を拒否する。
///
/// 呼び出された事実は必ずログへ残す（resolve を本当に通った証跡）。
pub fn resolve(direction_id: &str) -> Result<Box<dyn DirectionProfile>, String> {
    let normalized = direction_id.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "ja2zh" | "ja_zh" | "jazh" => {
            println!(
                "[DIRECTION][resolve] direction_id=\"{}\" -> profile=ja2zh",
                direction_id
            );
            Ok(Box::new(ja_zh::JaZhProfile))
        }
        other => {
            println!(
                "[DIRECTION][resolve][ERROR] unknown direction_id=\"{}\"",
                other
            );
            Err(format!(
                "unknown direction_id=\"{}\" (Phase 1 でサポートする方向: ja2zh)",
                other
            ))
        }
    }
}
