use crate::adapters::types::ProviderKind;
use serde::Serialize;

pub const MINIMUM_PRICE_YEN: u32 = 300;
pub const CELL_UNIT_PRICE_YEN: f64 = 0.30;
pub const CHAR_UNIT_PRICE_YEN: f64 = 0.03;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProviderEstimate {
    pub provider: String,
    pub candidate_no: u8,
    pub request_count: usize,
    pub sent_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BillingEstimate {
    pub mode: String,
    pub selected_sheets: Vec<String>,
    pub logical_cells: usize,
    pub translatable_cells: usize,
    pub candidate_units: usize,
    pub planned_requests: usize,
    pub planned_sent_chars: usize,
    pub cell_unit_price_yen: f64,
    pub char_unit_price_yen: f64,
    pub cell_based_price_yen: u32,
    pub char_based_price_yen: u32,
    pub metered_price_yen: u32,
    pub minimum_price_yen: u32,
    pub billing_price_yen: u32,
    pub minimum_applied: bool,
    pub message: String,
    pub provider_estimates: Vec<ProviderEstimate>,
}

impl BillingEstimate {
    pub fn print_for_powershell(&self) {
        println!("=== BILLING ESTIMATE / FIXED BILLING PRICE ===");
        println!("mode = {}", self.mode);
        println!("selected_sheets = {:?}", self.selected_sheets);
        println!("logical_cells = {}", self.logical_cells);
        println!("translatable_cells = {}", self.translatable_cells);
        println!("candidate_units = {}", self.candidate_units);
        println!("planned_requests = {}", self.planned_requests);
        println!("planned_sent_chars = {}", self.planned_sent_chars);
        println!("cell_based_price_yen = {}", self.cell_based_price_yen);
        println!("char_based_price_yen = {}", self.char_based_price_yen);
        println!("metered_price_yen = {}", self.metered_price_yen);
        println!("minimum_price_yen = {}", self.minimum_price_yen);
        println!("billing_price_yen = {}", self.billing_price_yen);
        println!("minimum_applied = {}", self.minimum_applied);
        for p in &self.provider_estimates {
            println!(
                "provider candidate{} {} requests={} chars={}",
                p.candidate_no,
                p.provider,
                p.request_count,
                p.sent_chars
            );
        }
        println!("message = {}", self.message);
        println!("=== END BILLING ESTIMATE ===");
    }
}

pub struct EstimateInput {
    pub mode: String,
    pub selected_sheets: Vec<String>,
    pub logical_cells: usize,
    pub translatable_cells: usize,
    pub candidate_units: usize,
    pub candidate1_provider: Option<ProviderKind>,
    pub candidate1_requests: usize,
    pub candidate1_chars: usize,
    pub candidate2_provider: Option<ProviderKind>,
    pub candidate2_requests: usize,
    pub candidate2_chars: usize,
    pub candidate3_provider: Option<ProviderKind>,
    pub candidate3_requests: usize,
    pub candidate3_chars: usize,
}

pub fn calculate_billing_estimate(input: EstimateInput) -> BillingEstimate {
    let planned_requests = input.candidate1_requests + input.candidate2_requests + input.candidate3_requests;
    let planned_sent_chars = input.candidate1_chars + input.candidate2_chars + input.candidate3_chars;

    let cell_based_raw = input.candidate_units as f64 * CELL_UNIT_PRICE_YEN;
    let char_based_raw = planned_sent_chars as f64 * CHAR_UNIT_PRICE_YEN;

    let cell_based_price_yen = floor_to_10_yen(cell_based_raw);
    let char_based_price_yen = floor_to_10_yen(char_based_raw);
    let metered_price_yen = cell_based_price_yen.max(char_based_price_yen);
    let minimum_applied = metered_price_yen < MINIMUM_PRICE_YEN;
    let billing_price_yen = if minimum_applied {
        MINIMUM_PRICE_YEN
    } else {
        metered_price_yen
    };

    let message = if minimum_applied {
        format!(
            "現在の翻訳対象の従量計算額は{}円です。最低料金は{}円となるため、現在は{}円でのご請求となります。翻訳対象を追加しても、従量計算額が{}円以内であれば請求額は変わりません。このまま進めますか？ Y=続ける / N=翻訳対象を再検討する",
            metered_price_yen,
            MINIMUM_PRICE_YEN,
            billing_price_yen,
            MINIMUM_PRICE_YEN
        )
    } else {
        format!(
            "現在の翻訳対象の従量計算額は{}円です。今回の請求確定額は{}円です。このまま進めますか？ Y=続ける / N=翻訳対象を再検討する",
            metered_price_yen,
            billing_price_yen
        )
    };

    let mut provider_estimates = Vec::new();
    if let Some(provider) = input.candidate1_provider {
        provider_estimates.push(ProviderEstimate {
            provider: provider.as_label().to_string(),
            candidate_no: 1,
            request_count: input.candidate1_requests,
            sent_chars: input.candidate1_chars,
        });
    }
    if let Some(provider) = input.candidate2_provider {
        provider_estimates.push(ProviderEstimate {
            provider: provider.as_label().to_string(),
            candidate_no: 2,
            request_count: input.candidate2_requests,
            sent_chars: input.candidate2_chars,
        });
    }
    if let Some(provider) = input.candidate3_provider {
        provider_estimates.push(ProviderEstimate {
            provider: provider.as_label().to_string(),
            candidate_no: 3,
            request_count: input.candidate3_requests,
            sent_chars: input.candidate3_chars,
        });
    }

    BillingEstimate {
        mode: input.mode,
        selected_sheets: input.selected_sheets,
        logical_cells: input.logical_cells,
        translatable_cells: input.translatable_cells,
        candidate_units: input.candidate_units,
        planned_requests,
        planned_sent_chars,
        cell_unit_price_yen: CELL_UNIT_PRICE_YEN,
        char_unit_price_yen: CHAR_UNIT_PRICE_YEN,
        cell_based_price_yen,
        char_based_price_yen,
        metered_price_yen,
        minimum_price_yen: MINIMUM_PRICE_YEN,
        billing_price_yen,
        minimum_applied,
        message,
        provider_estimates,
    }
}

fn floor_to_10_yen(value: f64) -> u32 {
    let yen = value.floor() as u32;
    (yen / 10) * 10
}
