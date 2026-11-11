//! End-to-end check of `#[skipper]`: the `drives_*` test re-runs this test
//! binary with a handoff cache that disables the `gated_*` tests, then checks
//! they were skipped (their bodies fail when they run in that child process).

use std::process::Command;

use get_skipper::{skip_if_disabled, skipper};
use rstest::rstest;

fn in_child() -> bool {
    std::env::var_os("SKIPPER_E2E").is_some()
}

#[skipper]
#[test]
fn gated_plain() {
    assert!(!in_child(), "should have been skipped");
}

#[skipper]
#[test]
fn gated_result() -> Result<(), String> {
    if in_child() {
        return Err("should have been skipped".into());
    }
    Ok(())
}

#[skipper]
#[rstest]
#[case(1)]
#[case(2)]
fn gated_rstest(#[case] n: u32) {
    assert!(!in_child(), "case {n} should have been skipped");
}

#[skipper]
#[tokio::test]
async fn gated_async() {
    assert!(!in_child(), "should have been skipped");
}

#[test]
fn gated_macro() {
    skip_if_disabled!();
    assert!(!in_child(), "should have been skipped");
}

#[skipper]
#[test]
fn enabled_runs() {}

#[test]
fn drives_skipping_in_a_child_process() {
    if in_child() {
        return;
    }
    let dir = tempfile_dir();
    let disabled = [
        "gated_plain",
        "gated_result",
        "gated_rstest",
        "gated_async",
        "gated_macro",
    ];
    let cache: serde_json::Map<String, serde_json::Value> = disabled
        .iter()
        .map(|n| {
            (
                format!("crates/get-skipper/tests/attribute.rs > {n}"),
                "2099-01-02T00:00:00+00:00".into(),
            )
        })
        .collect();
    let cache_file = dir.join("cache.json");
    std::fs::write(&cache_file, serde_json::to_vec(&cache).unwrap()).unwrap();

    let out = Command::new(std::env::current_exe().unwrap())
        .args(["gated_", "enabled_runs", "--test-threads=1"])
        .env("SKIPPER_E2E", "1")
        .env("SKIPPER_WORKER_CACHE_FILE", &cache_file)
        .env("SKIPPER_DISCOVERED_DIR", &dir)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "child failed:\n{stdout}\n{stderr}");
    for n in disabled {
        assert!(
            stderr.contains(&format!("[skipper] SKIP crates/get-skipper/tests/attribute.rs > {n}: test disabled until 2099-01-01")),
            "{n} not reported as skipped:\n{stderr}"
        );
    }

    let mut found = get_skipper::CacheManager::merge_discovered_ids(&dir);
    found.sort();
    assert_eq!(found.len(), 6, "{found:?}");
    assert!(found.contains(&"crates/get-skipper/tests/attribute.rs > enabled_runs".to_string()));
    get_skipper::CacheManager::cleanup(&dir);
}

fn tempfile_dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("skipper-attr-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}
