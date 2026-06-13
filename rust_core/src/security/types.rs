#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityResult {
    Reject,
    Warn,
    Pass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecuritySeverity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct SecurityCheckRecord {
    pub check_name: &'static str,
    pub result: SecurityResult,
    pub reason: String,
    pub evidence: String,
    pub severity: SecuritySeverity,
}

#[derive(Debug, Clone)]
pub struct SecurityReport {
    pub file_path: String,
    pub final_result: SecurityResult,
    pub records: Vec<SecurityCheckRecord>,
}
