use crate::core2::structure_types::LogicalCell;
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum EntryRoute { Translate, Skip, Warn, Reject }
#[derive(Debug, Clone)] pub struct Core1Policy { pub translator_order:Vec<crate::adapters::types::ProviderKind>, pub max_retry_count:u32 }
#[derive(Debug, Clone)] pub struct Core1AnalyzeInput { pub logical_cell:LogicalCell, pub source_lang:crate::adapters::types::Lang, pub target_lang:crate::adapters::types::Lang, pub policy:Core1Policy }
#[derive(Debug, Clone)] pub struct Segment { pub text:String, pub target:bool }
#[derive(Debug, Clone)] pub struct Core1AnalysisResult { pub logical_cell_id:String, pub route:EntryRoute, pub shape:Option<String>, pub note:String, pub segments:Vec<Segment> }
#[derive(Debug, Clone, Default)] pub struct CandidateAlarms { pub candidate1_alarm:Option<String>, pub candidate2_alarm:Option<String>, pub candidate3_alarm:Option<String> }
#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum DefaultSelect { Original=0, Candidate1=1, Candidate2=2, Candidate3=3 }
#[derive(Debug, Clone)] pub struct CandidateBundle { pub logical_cell_id:String, pub original:String, pub candidate1:Option<String>, pub candidate2:Option<String>, pub candidate3:Option<String>, pub default_select:DefaultSelect, pub alarms:CandidateAlarms, pub note:String }
