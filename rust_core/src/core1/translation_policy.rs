use crate::core1::text_structure_analyzer::TextStructure;
use crate::core2::structure_types::LogicalCellKind;

#[derive(Debug, Clone)]
pub struct TranslationPolicyDecision {
    pub translate_candidates: bool,
    pub note: String,
}

pub fn decide_translation_policy(
    cell_kind: LogicalCellKind,
    structure: &TextStructure,
) -> TranslationPolicyDecision {
    match cell_kind {
        LogicalCellKind::Empty | LogicalCellKind::NonTranslatable => TranslationPolicyDecision {
            translate_candidates: false,
            note: "empty/non-translatable cell".to_string(),
        },

        LogicalCellKind::SharedFormulaFollower => TranslationPolicyDecision {
            translate_candidates: false,
            note: "shared formula follower -> candidate translation skipped (parent/group driven)".to_string(),
        },

        LogicalCellKind::FormulaRaw | LogicalCellKind::SharedFormulaParent => {
            if structure.kanji_count >= 1 || structure.kana_kana_count >= 3 {
                TranslationPolicyDecision {
                    translate_candidates: true,
                    note: "formula cell -> candidates enabled".to_string(),
                }
            } else {
                TranslationPolicyDecision {
                    translate_candidates: false,
                    note: "formula cell without translatable JP text -> preserve".to_string(),
                }
            }
        }

        LogicalCellKind::Date => TranslationPolicyDecision {
            translate_candidates: false,
            note: "date cell -> preserve".to_string(),
        },

        LogicalCellKind::Number => TranslationPolicyDecision {
            translate_candidates: false,
            note: "number cell -> preserve".to_string(),
        },

        LogicalCellKind::HyperlinkText => TranslationPolicyDecision {
            translate_candidates: false,
            note: "hyperlink cell -> preserve".to_string(),
        },

        LogicalCellKind::Text => {
            if structure.kanji_count >= 1 || structure.kana_kana_count >= 3 {
                TranslationPolicyDecision {
                    translate_candidates: true,
                    note: "text cell -> candidates enabled".to_string(),
                }
            } else {
                TranslationPolicyDecision {
                    translate_candidates: false,
                    note: "text cell without translatable JP text -> preserve".to_string(),
                }
            }
        }
    }
}