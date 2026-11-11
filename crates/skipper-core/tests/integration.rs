//! Talks to the real shared spreadsheet. Skipped (passes with a note) when no
//! credentials are available: set GOOGLE_CREDS_B64 or put
//! service-account-skipper-bot.json at the workspace root.

use std::path::Path;

use skipper_core::{fetch_all, Credentials, HttpSheetsApi, SkipperConfig, SkipperResolver};

const SPREADSHEET_ID: &str = "1Nbjfhklw11uVbi6OCOSeCJI_PThJYzQLlZQbThb4Zvs";

fn config() -> Option<SkipperConfig> {
    let creds = match std::env::var("GOOGLE_CREDS_B64") {
        Ok(b64) if !b64.is_empty() => Credentials::Base64(b64),
        _ => {
            let file = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../service-account-skipper-bot.json");
            if !file.exists() {
                eprintln!("[skipper] no credentials, skipping integration test");
                return None;
            }
            Credentials::File(file)
        }
    };
    let id = std::env::var("SKIPPER_SPREADSHEET_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| SPREADSHEET_ID.into());
    Some(SkipperConfig::new(id, creds).with_sheet_name("skipper-rust"))
}

#[test]
fn fetches_the_real_sheet() {
    let Some(cfg) = config() else { return };
    let api = HttpSheetsApi::new(&cfg.credentials).unwrap();
    let res = fetch_all(&api, &cfg).unwrap();
    assert_eq!(res.primary.sheet_name, "skipper-rust");
    assert!(
        res.primary.header.iter().any(|h| h == "testId"),
        "{:?}",
        res.primary.header
    );
    assert!(res.primary.sheet_id != 0);
}

#[test]
fn resolver_initializes_from_the_real_sheet() {
    let Some(cfg) = config() else { return };
    std::env::set_var("SKIPPER_FAIL_OPEN", "false");
    std::env::set_var("SKIPPER_CACHE_TTL", "0");
    let r = SkipperResolver::initialize(&cfg).unwrap();
    assert!(r.is_test_enabled("not/in/sheet.rs > anything"));
}
