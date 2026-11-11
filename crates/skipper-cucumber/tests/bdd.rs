//! Runs the features with Skipper. With SKIPPER_E2E set, a handoff cache
//! disables one scenario; its last step fails if it runs anyway.

use cucumber::{given, then, when, World};

#[derive(Debug, Default, World)]
struct Counter(u32);

#[given("a counter")]
fn a_counter(w: &mut Counter) {
    w.0 = 0;
}

#[when("it is incremented")]
fn increment(w: &mut Counter) {
    w.0 += 1;
}

#[then("it is 1")]
fn is_one(w: &mut Counter) {
    assert_eq!(w.0, 1);
}

#[then("the scenario must have been skipped")]
fn must_skip(_: &mut Counter) {
    assert!(
        std::env::var_os("SKIPPER_E2E").is_none(),
        "scenario should have been skipped"
    );
}

#[tokio::main]
async fn main() {
    let features = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/features");
    if std::env::var_os("SKIPPER_E2E").is_some() {
        let dir = std::env::temp_dir().join(format!("skipper-cuke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let id = "crates/skipper-cucumber/tests/features/gating.feature > Skipper gating > disabled in the e2e check";
        let cache = serde_json_like(id);
        std::fs::write(dir.join("cache.json"), cache).unwrap();
        std::env::set_var("SKIPPER_WORKER_CACHE_FILE", dir.join("cache.json"));
        std::env::set_var("SKIPPER_DISCOVERED_DIR", &dir);
        skipper_cucumber::run::<Counter>(features).await;
        let found = skipper_core::CacheManager::merge_discovered_ids(&dir);
        skipper_core::CacheManager::cleanup(&dir);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.contains(&id.to_string()));
        eprintln!("[skipper] e2e ok");
        return;
    }
    skipper_cucumber::run::<Counter>(features).await;
}

fn serde_json_like(id: &str) -> String {
    format!(r#"{{"{}": "2099-01-02T00:00:00+00:00"}}"#, id)
}
