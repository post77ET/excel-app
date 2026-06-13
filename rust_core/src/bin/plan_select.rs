use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

// ============================================================
// COURSE / TRANSLATION PLAN SELECTION CHECK TOOL
//
// This binary is a Rust-side confirmation tool for development and
// field-debug use. Final user-facing course selection should be handled
// by Web UI.
//
// Course definition:
// - Experience course: normal Generate/UI/Apply flow, but translation
//   support target is fixed to A1:D5.
// - Paid course: normal Generate/UI/Apply flow for selected sheets.
//
// Candidate1/2/3 are translation METHODS.
// Google/Amazon/DeepL are PROVIDERS.
// DeepL is not allowed for Candidate1/2 split translation.
// ============================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobPlanSettings {
    mode: String,
    plan_name: String,
    enabled_candidates: Vec<u8>,
    candidate1_provider: Option<String>,
    candidate2_provider: Option<String>,
    candidate3_provider: Option<String>,
    default_candidate_priority: Vec<u8>,
    job_accept_threshold: f64,
    charge_unit: String,
    experience_range: String,
    note: String,
}

fn main() {
    println!("============================================================");
    println!("CORE1_ETB Course / Translation Plan Selection Check Tool");
    println!("============================================================");
    println!("This is a Rust-side confirmation tool only.");
    println!("Final user-facing course selection should be handled by Web UI.");
    println!();

    println!("Select course:");
    println!("  1) EXPERIENCE : A1:D5 only, normal Generate/UI/Apply flow");
    println!("  2) PAID       : selected sheets, normal Generate/UI/Apply flow");
    println!();

    let course = read_choice("Course number", &["1", "2"]);

    println!();
    println!("Select translation plan:");
    println!("  1) STANDARD_3_CANDIDATES  : Candidate1 + Candidate2 + Candidate3");
    println!("  2) LOW_COST_CANDIDATE1    : Candidate1 only");
    println!("  3) LOW_COST_CANDIDATE3    : Candidate3 only");
    println!();

    let plan = read_choice("Plan number", &["1", "2", "3"]);

    let mut settings = match plan.as_str() {
        "1" => build_standard_plan(),
        "2" => build_candidate1_only_plan(),
        "3" => build_candidate3_only_plan(),
        _ => unreachable!(),
    };

    if course == "1" {
        settings.mode = "experience".to_string();
        settings.experience_range = "A1:D5".to_string();
        settings.note = format!(
            "Experience course. Translation support target is fixed to A1:D5. {}",
            settings.note
        );
    } else {
        settings.mode = "paid".to_string();
    }

    println!();
    println!("Selected settings:");
    println!("  mode                       = {}", settings.mode);
    println!("  plan_name                  = {}", settings.plan_name);
    println!("  enabled_candidates         = {:?}", settings.enabled_candidates);
    println!("  candidate1_provider        = {:?}", settings.candidate1_provider);
    println!("  candidate2_provider        = {:?}", settings.candidate2_provider);
    println!("  candidate3_provider        = {:?}", settings.candidate3_provider);
    println!("  default_candidate_priority = {:?}", settings.default_candidate_priority);
    println!("  job_accept_threshold       = {:.2}", settings.job_accept_threshold);
    println!("  charge_unit                = {}", settings.charge_unit);
    if settings.mode == "experience" {
        println!("  experience_range           = {}", settings.experience_range);
    } else {
        println!("  experience_range           = N/A (paid mode)");
    }
    println!();

    let ok = read_choice("Write config/job_plan_settings.json? 1=YES 2=NO", &["1", "2"]);
    if ok != "1" {
        println!("USER_CANCEL");
        return;
    }

    let output_path = Path::new("config/job_plan_settings.json");
    if let Some(parent) = output_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            eprintln!("ERROR: failed to create config directory: {}", e);
            std::process::exit(1);
        }
    }

    let json = match serde_json::to_string_pretty(&settings) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ERROR: failed to serialize settings: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = fs::write(output_path, json) {
        eprintln!("ERROR: failed to write {}: {}", output_path.display(), e);
        std::process::exit(1);
    }

    println!("SUCCESS");
    println!("OUTPUT = {}", output_path.display());
}

fn build_standard_plan() -> JobPlanSettings {
    JobPlanSettings {
        mode: "paid".to_string(),
        plan_name: "STANDARD_3_CANDIDATES".to_string(),
        enabled_candidates: vec![1, 2, 3],
        candidate1_provider: Some("Google".to_string()),
        candidate2_provider: Some("Amazon".to_string()),
        candidate3_provider: Some("DeepL".to_string()),
        default_candidate_priority: vec![1, 2, 3],
        job_accept_threshold: 0.80,
        charge_unit: "fixed_estimate_billing".to_string(),
        experience_range: "A1:D5".to_string(),
        note: "Standard plan. Candidate columns must not be falsified. DefaultSelect may fallback by priority only.".to_string(),
    }
}

fn build_candidate1_only_plan() -> JobPlanSettings {
    println!();
    println!("Candidate1 is split-translation method.");
    println!("Allowed providers:");
    println!("  1) Google");
    println!("  2) Amazon");
    println!("DeepL is intentionally NOT allowed for Candidate1.");
    println!();

    let provider_choice = read_choice("Candidate1 provider", &["1", "2"]);
    let provider = if provider_choice == "1" { "Google" } else { "Amazon" };

    JobPlanSettings {
        mode: "paid".to_string(),
        plan_name: "LOW_COST_CANDIDATE1_ONLY".to_string(),
        enabled_candidates: vec![1],
        candidate1_provider: Some(provider.to_string()),
        candidate2_provider: None,
        candidate3_provider: None,
        default_candidate_priority: vec![1],
        job_accept_threshold: 0.80,
        charge_unit: "fixed_estimate_billing".to_string(),
        experience_range: "A1:D5".to_string(),
        note: "Low-cost plan. Candidate1 only. Apply is still required for final delivery.".to_string(),
    }
}

fn build_candidate3_only_plan() -> JobPlanSettings {
    JobPlanSettings {
        mode: "paid".to_string(),
        plan_name: "LOW_COST_CANDIDATE3_ONLY".to_string(),
        enabled_candidates: vec![3],
        candidate1_provider: None,
        candidate2_provider: None,
        candidate3_provider: Some("DeepL".to_string()),
        default_candidate_priority: vec![3],
        job_accept_threshold: 0.80,
        charge_unit: "fixed_estimate_billing".to_string(),
        experience_range: "A1:D5".to_string(),
        note: "Low-cost plan. Candidate3 whole-cell translation only. Apply is still required for final delivery.".to_string(),
    }
}

fn read_choice(label: &str, allowed: &[&str]) -> String {
    loop {
        print!("{} > ", label);
        if let Err(e) = io::stdout().flush() {
            eprintln!("ERROR: failed to flush stdout: {}", e);
            std::process::exit(1);
        }

        let mut input = String::new();
        if let Err(e) = io::stdin().read_line(&mut input) {
            eprintln!("ERROR: failed to read stdin: {}", e);
            std::process::exit(1);
        }

        let value = input.trim().to_string();
        if allowed.iter().any(|v| *v == value) {
            return value;
        }

        println!("Invalid input. Allowed: {:?}", allowed);
    }
}
