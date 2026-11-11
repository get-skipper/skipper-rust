//! Skipper for [cucumber-rs](https://github.com/cucumber-rs/cucumber).
//!
//! ```ignore
//! #[tokio::main]
//! async fn main() {
//!     skipper_cucumber::run::<MyWorld>("tests/features").await;
//! }
//! ```
//!
//! Test ID: `tests/features/auth.feature > Feature name > [Rule name >] Scenario name`.
//! Disabled scenarios are filtered out of the run (and listed on stderr).

use std::fmt::Debug;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cucumber::gherkin::{Feature, Rule, Scenario};
use cucumber::writer::Stats;
use cucumber::World;
pub use skipper_core;
use skipper_core::{
    build_report, build_test_id, emit_summary, warn, CacheManager, SheetsWriter, SkipperConfig,
    SkipperMode, SkipperResolver,
};

/// Run every feature under `input` with Skipper gating, then emit the
/// quarantine report (and sync in `SKIPPER_MODE=sync`). Panics if a scenario
/// failed, like `Cucumber::run_and_exit`.
pub async fn run<W>(input: impl Into<PathBuf>)
where
    W: World + Debug + cucumber::codegen::WorldInventory,
{
    let skipper = Skipper::from_env();
    let writer = W::cucumber()
        .filter_run(input.into(), skipper.filter())
        .await;
    skipper.finish();
    if writer.execution_has_failed() {
        panic!(
            "{} step(s) failed, {} parsing error(s), {} hook error(s)",
            writer.failed_steps(),
            writer.parsing_errors(),
            writer.hook_errors()
        );
    }
}

/// Building blocks for a custom `Cucumber` setup:
/// `W::cucumber()….filter_run(path, skipper.filter()).await; skipper.finish();`
pub struct Skipper {
    resolver: Option<Arc<SkipperResolver>>,
    discovered: Arc<Mutex<Vec<String>>>,
}

impl Skipper {
    /// See [`SkipperResolver::from_env`]. Panics only with `SKIPPER_FAIL_OPEN=false`
    /// when the spreadsheet cannot be loaded.
    pub fn from_env() -> Self {
        let resolver = match SkipperResolver::from_env() {
            Ok(r) => r.map(Arc::new),
            Err(e) => panic!("[skipper] cannot load spreadsheet: {e}"),
        };
        Skipper {
            resolver,
            discovered: Arc::default(),
        }
    }

    /// Scenario filter for `Cucumber::filter_run`: `false` for disabled scenarios.
    pub fn filter(&self) -> impl Fn(&Feature, Option<&Rule>, &Scenario) -> bool + 'static {
        let resolver = self.resolver.clone();
        let discovered = Arc::clone(&self.discovered);
        move |feature, rule, scenario| {
            let Some(resolver) = &resolver else {
                return true;
            };
            let id = scenario_id(feature, rule, scenario);
            discovered
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(id.clone());
            let enabled = resolver.is_test_enabled(&id);
            skipper_core::log(format!("scenario {id:?} enabled={enabled}"));
            if !enabled {
                let _ = writeln!(std::io::stderr(), "{}", resolver.skip_message(&id));
            }
            enabled
        }
    }

    /// Hand discovered IDs to `cargo skipper` when running under it; otherwise
    /// emit the report and, in sync mode, reconcile the sheet.
    pub fn finish(self) {
        let Some(resolver) = self.resolver else {
            return;
        };
        let mut seen = std::collections::HashSet::new();
        let discovered: Vec<String> =
            std::mem::take(&mut *self.discovered.lock().unwrap_or_else(|e| e.into_inner()))
                .into_iter()
                .filter(|id| seen.insert(id.clone()))
                .collect();

        if let Some(dir) = std::env::var_os("SKIPPER_DISCOVERED_DIR") {
            if let Err(e) = CacheManager::write_discovered_ids(Path::new(&dir), &discovered) {
                warn(format!("cannot record discovered scenarios: {e}"));
            }
            return;
        }

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
        if SkipperMode::from_env() == SkipperMode::Sync {
            match SkipperConfig::from_env() {
                Some(cfg) => {
                    if let Err(e) = SheetsWriter::new(cfg).sync(&discovered) {
                        panic!("[skipper] sync failed: {e}");
                    }
                }
                None => warn("SKIPPER_MODE=sync but Skipper is not configured"),
            }
        }
    }
}

/// `features/x.feature > Feature > [Rule >] Scenario`.
pub fn scenario_id(feature: &Feature, rule: Option<&Rule>, scenario: &Scenario) -> String {
    let file = feature
        .path
        .as_deref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut parts = vec![feature.name.as_str()];
    if let Some(r) = rule {
        parts.push(r.name.as_str());
    }
    parts.push(scenario.name.as_str());
    build_test_id(&file, &parts)
}
