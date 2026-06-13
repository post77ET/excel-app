use std::env;

use core1_etb::app::apply_orchestrator::run_apply_pipeline;
use core1_etb::app::generate_orchestrator::run_generate_pipeline;
use core1_etb::entry::entry_state::EntryError;
use core1_etb::entry::generate_entry_pipeline::run_generate_select_pipeline;
use core1_etb::entry::estimate_entry_pipeline::run_estimate_select_pipeline;

fn main() {
    dotenvy::dotenv().ok();
    println!("DEEPL_API_KEY={:?}", std::env::var("DEEPL_API_KEY"));
    println!(
        "GOOGLE_APPLICATION_CREDENTIALS={:?}",
        std::env::var("GOOGLE_APPLICATION_CREDENTIALS")
    );
    println!("AWS_ACCESS_KEY_ID={:?}", std::env::var("AWS_ACCESS_KEY_ID"));
    println!(
        "AWS_SECRET_ACCESS_KEY={:?}",
        mask_secret(std::env::var("AWS_SECRET_ACCESS_KEY").ok())
    );
    println!("CORE1_ETb boot");

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("ERROR: mode not specified");
        print_usage();
        std::process::exit(1);
    }

    match args[1].as_str() {
        "generate" => {
            if args.len() < 3 {
                eprintln!("ERROR: input file required for generate");
                print_usage();
                std::process::exit(1);
            }
            let input = &args[2];
            println!("MODE = GENERATE");
            println!("INPUT = {}", input);

            // Generate入口を正式入口に統一する。
            // 入口だけがPowerShell/Webで異なり、CORE本体は同じgenerate pipelineを通す。
            env::remove_var("ETB_UI_INPUT");
            env::set_var("ETB_INPUT_PATH", input);

            match run_generate_pipeline() {
                Ok(output_path) => {
                    println!("OUTPUT = {}", output_path);
                    println!("SUCCESS");
                }
                Err(e) => {
                    eprintln!("ERROR: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        "apply" => {
            if args.len() < 4 {
                eprintln!("ERROR: ui and base file required for apply");
                print_usage();
                std::process::exit(1);
            }
            let ui = &args[2];
            let base = &args[3];
            println!("MODE = APPLY");
            println!("UI_INPUT = {}", ui);
            println!("BASE = {}", base);
            env::set_var("ETB_UI_INPUT", ui);
            env::set_var("ETB_INPUT_PATH", base);
            match run_apply_pipeline() {
                Ok(_) => println!("SUCCESS"),
                Err(e) => {
                    eprintln!("ERROR: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        "generate-select" => {
            if args.len() < 3 {
                eprintln!("ERROR: input file required for generate-select");
                print_usage();
                std::process::exit(1);
            }
            let input = &args[2];
            println!("MODE = GENERATE_SELECT_COMPAT");
            println!("INPUT = {}", input);
            println!("[COMPAT] generate-select is kept as an alias of the official generate pipeline.");
            match run_generate_select_pipeline(input) {
                Ok(result) => {
                    println!("SUCCESS");
                    println!("JOB_ID = {}", result.job_id);
                    println!("OUTPUT = {}", result.output_ui_path.display());
                    println!("SELECTED = {:?}", result.selected_sheets);
                }
                Err(EntryError::UserExit) => {
                    println!("USER_EXIT");
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("ERROR: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        "estimate-select" => {
            if args.len() < 3 {
                eprintln!("ERROR: input file required for estimate-select");
                print_usage();
                std::process::exit(1);
            }
            let input = &args[2];
            println!("MODE = ESTIMATE_SELECT");
            println!("INPUT = {}", input);
            match run_estimate_select_pipeline(input) {
                Ok(estimate) => {
                    println!("SUCCESS");
                    println!("BILLING_PRICE_YEN = {}", estimate.billing_price_yen);
                    println!("METERED_PRICE_YEN = {}", estimate.metered_price_yen);
                    println!("MINIMUM_APPLIED = {}", estimate.minimum_applied);
                }
                Err(EntryError::UserExit) => {
                    println!("USER_EXIT");
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("ERROR: {:?}", e);
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("ERROR: unknown mode: {}", args[1]);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("USAGE:");
    eprintln!("  cargo run --bin core1_etb -- generate <input.xlsx>");
    eprintln!("  cargo run --bin core1_etb -- apply <ui.xlsx> <base.xlsx>");
    eprintln!("  cargo run --bin core1_etb -- generate-select <input.xlsx>   # compatibility alias");
    eprintln!("  cargo run --bin core1_etb -- estimate-select <input.xlsx>   # billing estimate / fixed billing price");
}

fn mask_secret(value: Option<String>) -> Option<String> {
    value.map(|v| {
        if v.is_empty() {
            return "<empty>".to_string();
        }
        if v.len() <= 8 {
            return "***".to_string();
        }
        let head = &v[..4];
        let tail = &v[v.len() - 4..];
        format!("{}***{}", head, tail)
    })
}
