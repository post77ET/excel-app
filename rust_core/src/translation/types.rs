use anyhow::Result;

#[derive(Debug, Clone, Copy)]
pub enum TranslationProvider {
    Amazon,
    Google,
    DeepL,
}

pub trait TranslationProviderDisplay {
    fn short_name(&self) -> &'static str;
}

impl TranslationProviderDisplay for TranslationProvider {
    fn short_name(&self) -> &'static str {
        match self {
            TranslationProvider::Amazon => "AMAZON",
            TranslationProvider::Google => "GOOGLE",
            TranslationProvider::DeepL => "DEEPL",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CandidateSlot {
    Candidate1,
    Candidate2,
    Candidate3,
}

impl CandidateSlot {
    pub fn short_name(&self) -> &'static str {
        match self {
            CandidateSlot::Candidate1 => "C1",
            CandidateSlot::Candidate2 => "C2",
            CandidateSlot::Candidate3 => "C3",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CandidatePlan {
    pub candidate1: TranslationProvider,
    pub candidate2: TranslationProvider,
    pub candidate3: TranslationProvider,
}

impl Default for CandidatePlan {
    fn default() -> Self {
        Self {
            candidate1: TranslationProvider::Google,
            candidate2: TranslationProvider::Amazon,
            candidate3: TranslationProvider::DeepL,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranslationRequestOwned {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
}

impl TranslationRequestOwned {
    pub fn as_borrowed(&self) -> TranslationRequest<'_> {
        TranslationRequest {
            text: &self.text,
            source_lang: &self.source_lang,
            target_lang: &self.target_lang,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TranslationRequest<'a> {
    pub text: &'a str,
    pub source_lang: &'a str,
    pub target_lang: &'a str,
}

#[derive(Debug)]
pub struct CandidateOutput {
    pub slot: CandidateSlot,
    pub provider: TranslationProvider,
    pub result: Result<String>,
}

#[derive(Debug)]
pub struct CandidateBundle {
    pub outputs: Vec<CandidateOutput>,
}

#[derive(Debug, Clone)]
pub struct WorkTableRow {
    // A列: 元セル位置
    pub origin_cell: String,
    // B列: セル属性
    pub cell_attr: String,
    // C列以降: 分解片 or 全文
    pub parts: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorkTableTranslatedRow {
    pub origin_cell: String,
    pub cell_attr: String,
    pub translated_parts: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct BatchLimits {
    pub max_items: usize,
    pub max_chars: usize,
    pub amazon_parallelism: usize,
}

impl Default for BatchLimits {
    fn default() -> Self {
        Self {
            max_items: 50,
            max_chars: 4000,
            amazon_parallelism: 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexedTextUnit {
    pub row_index: usize,
    pub part_index: usize,
    pub request: TranslationRequestOwned,
}