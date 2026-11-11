//! `cargo skipper <cargo args…>`, e.g. `cargo skipper test --workspace` or
//! `cargo skipper nextest run`.
//!
//! Fetches the spreadsheet once, hands the result to every test binary
//! (`SKIPPER_WORKER_CACHE_FILE`), collects the test IDs they report
//! (`SKIPPER_DISCOVERED_DIR`), then writes the quarantine report and, with
//! `SKIPPER_MODE=sync`, reconciles the sheet. Exits with cargo's exit code.

use std::process::{Command, ExitCode};

use skipper_core::{
    build_report, emit_summary, warn, CacheManager, SheetsWriter, SkipperConfig, SkipperMode,
    SkipperResolver,
};

fn main() -> ExitCode {
    // Invoked by cargo as `cargo-skipper skipper <args…>`; also allow running it directly.
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("skipper") {
        args.remove(0);
    }
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        eprintln!("usage: cargo skipper <cargo subcommand and args>  (e.g. `cargo skipper test --workspace`)");
        return ExitCode::from(2);
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());

    let Some(config) = SkipperConfig::from_env() else {
        eprintln!(
            "[skipper] SKIPPER_SPREADSHEET_ID / credentials not set — running without Skipper"
        );
        return run(Command::new(cargo).args(&args));
    };
    let resolver = match SkipperResolver::initialize(&config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[skipper] cannot load spreadsheet: {e}");
            return ExitCode::FAILURE;
        }
    };
    let cache_file = match resolver
        .marshal_cache()
        .and_then(|b| CacheManager::write_resolver_cache(&b))
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[skipper] cannot write worker cache: {e}");
            return ExitCode::FAILURE;
        }
    };
    let dir = cache_file
        .parent()
        .expect("cache file has a parent")
        .to_path_buf();

    let code = run(Command::new(cargo)
        .args(&args)
        .env("SKIPPER_WORKER_CACHE_FILE", &cache_file)
        .env("SKIPPER_DISCOVERED_DIR", &dir));

    let discovered = CacheManager::merge_discovered_ids(&dir);
    let now = chrono::Utc::now();
    let (suppressed, ran): (Vec<String>, Vec<String>) = discovered
        .iter()
        .cloned()
        .partition(|id| !resolver.is_test_enabled(id));
    let re_enabled: Vec<String> = ran
        .into_iter()
        .filter(|id| resolver.disabled_until(id).is_some_and(|d| d <= now))
        .collect();
    if let Err(e) = emit_summary(&build_report(resolver.entries(), &suppressed, &re_enabled)) {
        warn(format!("cannot write report: {e}"));
    }

    let mut code = code;
    if SkipperMode::from_env() == SkipperMode::Sync {
        if let Err(e) = SheetsWriter::new(config).sync(&discovered) {
            eprintln!("[skipper] sync failed: {e}");
            code = ExitCode::FAILURE;
        } else {
            eprintln!("[skipper] synced {} test IDs", discovered.len());
        }
    }
    CacheManager::cleanup(&dir);
    code
}

fn run(cmd: &mut Command) -> ExitCode {
    match cmd.status() {
        Ok(s) => match s.code() {
            Some(c) => ExitCode::from(c.clamp(0, 255) as u8),
            None => ExitCode::FAILURE,
        },
        Err(e) => {
            eprintln!("[skipper] cannot run cargo: {e}");
            ExitCode::FAILURE
        }
    }
}
