// umya の xlsx 読み込みを panic から守る共通ヘルパ。
//
// 背景: umya-spreadsheet は特定のファイルで Err を返さず内部 panic する。
// read() を直接呼ぶと panic が main を貫きプロセスごと即死する（可用性欠陥）。
// ここで catch_unwind により panic を捕捉し、通常の Err へ変換する。
//
// 方針:
//   - umya への依存（read 呼び出し）は本ファイルへ一本化する。
//   - 他モジュールは reader::xlsx::read を直接呼ばず、必ず safe_read_xlsx を使う。
//   - AssertUnwindSafe は本ヘルパ内に閉じ込め、外へ漏らさない。
//   - panic payload と呼び出し元はログ（stderr）へ残す。客には見せない。
//   - 返す Err 文字列には Web 側が判定するための安定キーを含める。

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use umya_spreadsheet::Spreadsheet;

/// Web 側がこのキーを検出して客向け文言（ja/zh）に変換する安定トークン。
pub const WORKBOOK_PARSE_FAILED: &str = "WORKBOOK_PARSE_FAILED";

/// xlsx を読み込む。umya が内部 panic しても即死させず Err を返す。
/// caller: 呼び出し元の識別子（ログ用。例 "source_workbook_reader"）。
pub fn safe_read_xlsx(path: &str, caller: &str) -> Result<Spreadsheet, String> {
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        umya_spreadsheet::reader::xlsx::read(Path::new(path))
    }));

    match outcome {
        // 正常読み込み（従来挙動と同一）
        Ok(Ok(book)) => Ok(book),
        // umya が Err を返した（従来挙動と同一の読み取りエラー）
        Ok(Err(e)) => Err(format!("workbook read error (caller={caller}): {e}")),
        // umya が内部 panic した（本ヘルパの主目的：即死を防ぐ）
        Err(payload) => {
            let detail = payload_to_string(&payload);
            eprintln!(
                "[{WORKBOOK_PARSE_FAILED}] caller={caller} panic_payload={detail}"
            );
            Err(format!("{WORKBOOK_PARSE_FAILED}: caller={caller}"))
        }
    }
}

fn payload_to_string(payload: &Box<dyn Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}
