use core1_etb::security::pipeline::{inspect_xlsx, print_report};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 { eprintln!("USAGE: cargo run --bin security_probe -- <file.xlsx>"); std::process::exit(1); }
    let report = inspect_xlsx(&args[1]);
    print_report(&report);
}
