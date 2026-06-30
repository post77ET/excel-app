use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::infra::config_loader::TranslatorConfig;
use crate::ui::types::UiRow;
use umya_spreadsheet::{Workbook, Worksheet};

pub const INTERNAL_SHEET_NAME: &str = "__ETB_INTERNAL";
pub const INTERNAL_APP_ID: &str = "CORE1_ETB_UI";
pub const INTERNAL_VERSION: &str = "v1";

#[derive(Debug, Clone)]
pub struct InternalMetadata {
    pub app_id: String,
    pub version: String,
    pub ui_sheet_name: String,
    // header は「表示キャッシュ」。Generate の出力ヘッダ文字列をそのまま保持する。
    // Header is persisted to preserve exact Generate output (display cache).
    // Future UI redesigns may regenerate headers from provider (provider is the truth source).
    pub candidate1_header: String,
    pub candidate2_header: String,
    pub candidate3_header: String,
    // provider / method は「真実源（業務データ）」。内部値で保持し、ハッシュ対象外。
    pub candidate1_provider: String,
    pub candidate2_provider: String,
    pub candidate3_provider: String,
    pub candidate1_method: String,
    pub candidate2_method: String,
    pub candidate3_method: String,
    pub row_count: usize,
    pub immutable_hash: String,
}

impl InternalMetadata {
    /// providers / methods は候補1..3の内部値ラベル（真実源）。
    /// 例 providers=["GOOGLE","AMAZON","None"], methods=["split","split","none"]。
    /// header は従来どおり config から生成（表示キャッシュ。出力不変＝回帰なし）。
    pub fn from_rows(
        rows: &[UiRow],
        config: &TranslatorConfig,
        providers: &[String; 3],
        methods: &[String; 3],
    ) -> Self {
        let candidate1_header = format!("candidate1 = {}", config.candidate1_provider.as_label());
        let candidate2_header = if rows.iter().any(|r| r.candidate2.is_some()) {
            format!("candidate2 = {}", config.candidate2_provider.as_label())
        } else {
            "candidate2 = None".to_string()
        };
        let candidate3_header = if rows.iter().any(|r| r.candidate3.is_some()) {
            format!("candidate3 = {}", config.candidate3_provider.as_label())
        } else {
            "candidate3 = None".to_string()
        };
        let immutable_hash =
            compute_immutable_hash(rows, &candidate1_header, &candidate2_header, &candidate3_header);

        Self {
            app_id: INTERNAL_APP_ID.to_string(),
            version: INTERNAL_VERSION.to_string(),
            ui_sheet_name: "TRANSLATION_UI".to_string(),
            candidate1_header,
            candidate2_header,
            candidate3_header,
            candidate1_provider: providers[0].clone(),
            candidate2_provider: providers[1].clone(),
            candidate3_provider: providers[2].clone(),
            candidate1_method: methods[0].clone(),
            candidate2_method: methods[1].clone(),
            candidate3_method: methods[2].clone(),
            row_count: rows.len(),
            immutable_hash,
        }
    }
}

pub fn write_internal_metadata_sheet_into_book(
    book: &mut Workbook,
    metadata: &InternalMetadata,
) -> Result<(), String> {
    if book.sheet_by_name(INTERNAL_SHEET_NAME).is_ok() {
        let _ = book.remove_sheet_by_name(INTERNAL_SHEET_NAME);
    }

    let _ = book.new_sheet(INTERNAL_SHEET_NAME);
    let sheet = book
        .sheet_by_name_mut(INTERNAL_SHEET_NAME)
        .map_err(|_| "internal metadata sheet create error".to_string())?;

    let pairs = [
        ("app_id", metadata.app_id.as_str()),
        ("version", metadata.version.as_str()),
        ("ui_sheet_name", metadata.ui_sheet_name.as_str()),
        ("candidate1_header", metadata.candidate1_header.as_str()),
        ("candidate2_header", metadata.candidate2_header.as_str()),
        ("candidate3_header", metadata.candidate3_header.as_str()),
        // 真実源（業務データ）。ハッシュ非対象。表示は header、判断は provider/method。
        ("candidate1_provider", metadata.candidate1_provider.as_str()),
        ("candidate2_provider", metadata.candidate2_provider.as_str()),
        ("candidate3_provider", metadata.candidate3_provider.as_str()),
        ("candidate1_method", metadata.candidate1_method.as_str()),
        ("candidate2_method", metadata.candidate2_method.as_str()),
        ("candidate3_method", metadata.candidate3_method.as_str()),
        ("row_count", &metadata.row_count.to_string()),
        ("immutable_hash", metadata.immutable_hash.as_str()),
    ];

    for (idx, (key, value)) in pairs.iter().enumerate() {
        let row = idx + 1;
        sheet.cell_mut(format!("A{}", row)).set_value(*key);
        sheet.cell_mut(format!("B{}", row)).set_value(*value);
    }

    hide_sheet(sheet);
    Ok(())
}

fn hide_sheet(sheet: &mut Worksheet) {
    // シートを非表示にする（UIには表示しない内部メタデータシート）
    sheet.set_sheet_state("hidden".to_string());
}

fn normalize_hash_text(v: &str) -> String {
    // 改行表現の差異（生成時の in-memory と Apply時の calamine 読み戻し）を吸収する。
    // \r\n / 単独 \r をすべて \n に正規化し、複数行候補での hash 不一致を防ぐ。
    v.replace("\r\n", "\n").replace('\r', "\n")
}

fn hash_text(value: &str, hasher: &mut DefaultHasher) {
    normalize_hash_text(value).hash(hasher);
}

fn hash_opt_text(value: &Option<String>, hasher: &mut DefaultHasher) {
    value
        .as_ref()
        .map(|v| normalize_hash_text(v))
        .hash(hasher);
}

pub fn compute_immutable_hash(
    rows: &[UiRow],
    candidate1_header: &str,
    candidate2_header: &str,
    candidate3_header: &str,
) -> String {
    let mut hasher = DefaultHasher::new();

    "TRANSLATION_UI".hash(&mut hasher);
    candidate1_header.hash(&mut hasher);
    candidate2_header.hash(&mut hasher);
    candidate3_header.hash(&mut hasher);
    rows.len().hash(&mut hasher);

    for row in rows {
        row.logical_cell_id.hash(&mut hasher);
        row.sheet_name.hash(&mut hasher);
        row.anchor_address.hash(&mut hasher);
        row.cell_kind.hash(&mut hasher);

        hash_text(&row.original, &mut hasher);
        hash_text(&row.original_writeback, &mut hasher);

        row.writeback_mode.hash(&mut hasher);

        hash_opt_text(&row.candidate1, &mut hasher);
        hash_opt_text(&row.candidate2, &mut hasher);
        hash_opt_text(&row.candidate3, &mut hasher);

        row.default_select.hash(&mut hasher);

        hash_opt_text(&row.alarms.candidate1_alarm, &mut hasher);
        hash_opt_text(&row.alarms.candidate2_alarm, &mut hasher);
        hash_opt_text(&row.alarms.candidate3_alarm, &mut hasher);

        hash_text(&row.note, &mut hasher);
    }

    format!("{:016x}", hasher.finish())
}


// ============================================================
// C-2: __ETB_INTERNAL リーダ
//
// Apply は再注入時にラベルを作り直さず、Generate が保存した値を読み戻す。
// provider/method は真実源、header は表示キャッシュ。
// 旧ファイル（provider/method 欄なし）は呼び出し側で既定補完＋legacyログ。
// ============================================================

/// 候補1件分の復元結果。provider/method が無い旧ファイルでは None。
#[derive(Debug, Clone, Default)]
pub struct RecoveredCandidate {
    pub header: Option<String>,
    pub provider: Option<String>,
    pub method: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RecoveredInternal {
    pub candidate1: RecoveredCandidate,
    pub candidate2: RecoveredCandidate,
    pub candidate3: RecoveredCandidate,
}

/// UI Excel の __ETB_INTERNAL シートから candidateN_header/provider/method を読み取る。
/// シートが無い等で読めない場合は None（呼び出し側で legacy フォールバック）。
pub fn read_internal_from_ui_file(ui_workbook_path: &str) -> Option<RecoveredInternal> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut workbook = open_workbook_auto(ui_workbook_path).ok()?;
    let range = workbook.worksheet_range(INTERNAL_SHEET_NAME).ok()?;

    // A列=key, B列=value のマップ化（apply_guard と同方式）
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in range.rows() {
        if row.len() < 2 {
            continue;
        }
        let key = match &row[0] {
            Data::String(s) => s.trim().to_string(),
            _ => continue,
        };
        if key.is_empty() {
            continue;
        }
        let value = match &row[1] {
            Data::String(s) => s.to_string(),
            Data::Int(i) => i.to_string(),
            Data::Float(fl) => fl.to_string(),
            Data::Bool(bl) => bl.to_string(),
            _ => String::new(),
        };
        map.insert(key, value);
    }

    let pick = |k: &str| -> Option<String> {
        map.get(k).map(|s| s.to_string()).filter(|s| !s.is_empty())
    };
    let cand = |n: u8| RecoveredCandidate {
        header: pick(&format!("candidate{}_header", n)),
        provider: pick(&format!("candidate{}_provider", n)),
        method: pick(&format!("candidate{}_method", n)),
    };

    Some(RecoveredInternal {
        candidate1: cand(1),
        candidate2: cand(2),
        candidate3: cand(3),
    })
}
