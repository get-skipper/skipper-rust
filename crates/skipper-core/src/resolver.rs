use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::HttpSheetsApi;
use crate::client::{fetch_all, TestEntry};
use crate::config::SkipperConfig;
use crate::error::Result;
use crate::logger::{log, warn};
use crate::testid::normalize_test_id;

const DEFAULT_CACHE_FILE: &str = ".skipper-cache.json";
const DEFAULT_CACHE_TTL: u64 = 300;

type Cache = HashMap<String, Option<DateTime<Utc>>>;

/// Decides whether a test runs. Tests not in the spreadsheet always run
/// (opt-out model); a test with a future `disabledUntil` is skipped.
#[derive(Debug, Clone, Default)]
pub struct SkipperResolver {
    cache: Cache,
}

#[derive(Serialize, Deserialize)]
struct DiskCache {
    ts: f64,
    entries: Cache,
}

impl SkipperResolver {
    /// Fetch the spreadsheet once. See [`SkipperResolver::initialize_with`]
    /// for the fallback rules.
    pub fn initialize(config: &SkipperConfig) -> Result<Self> {
        Self::initialize_with(|| {
            let api = HttpSheetsApi::new(&config.credentials)?;
            Ok(fetch_all(&api, config)?.entries)
        })
    }

    /// Build the resolver from `fetch`. If it fails: use the disk cache
    /// (`SKIPPER_CACHE_FILE`, default `.skipper-cache.json`) when it is younger
    /// than `SKIPPER_CACHE_TTL` seconds (default 300, `0` disables it); else,
    /// with `SKIPPER_FAIL_OPEN` (default true), run every test; else error.
    pub fn initialize_with(fetch: impl FnOnce() -> Result<Vec<TestEntry>>) -> Result<Self> {
        log("initializing resolver");
        let cache_file = std::env::var("SKIPPER_CACHE_FILE")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_CACHE_FILE.into());
        let ttl = std::env::var("SKIPPER_CACHE_TTL")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_CACHE_TTL);
        let fail_open = std::env::var("SKIPPER_FAIL_OPEN")
            .map(|v| !v.trim().eq_ignore_ascii_case("false"))
            .unwrap_or(true);

        match fetch() {
            Ok(entries) => {
                let cache: Cache = entries
                    .into_iter()
                    .map(|e| (normalize_test_id(&e.test_id), e.disabled_until))
                    .collect();
                log(format!(
                    "loaded {} test entries from spreadsheet",
                    cache.len()
                ));
                if ttl > 0 {
                    write_disk_cache(Path::new(&cache_file), &cache);
                }
                Ok(SkipperResolver { cache })
            }
            Err(err) => {
                if ttl > 0 {
                    if let Some((age, cache)) = read_disk_cache(Path::new(&cache_file), ttl) {
                        warn(format!("API failed ({err}), using cache ({age:.0}s old)"));
                        return Ok(SkipperResolver { cache });
                    }
                }
                if fail_open {
                    warn(format!(
                        "API failed ({err}), no valid cache — running all tests (fail-open)"
                    ));
                    return Ok(SkipperResolver::default());
                }
                Err(err)
            }
        }
    }

    /// Resolver for a test process: rehydrate from `SKIPPER_WORKER_CACHE_FILE`
    /// (written by `cargo skipper`) or fetch with [`SkipperConfig::from_env`].
    /// `Ok(None)` when Skipper is not configured (every test runs).
    pub fn from_env() -> Result<Option<Self>> {
        if let Some(file) = std::env::var_os("SKIPPER_WORKER_CACHE_FILE") {
            match std::fs::read(&file)
                .map_err(crate::Error::from)
                .and_then(|b| Self::from_marshaled_cache(&b))
            {
                Ok(r) => return Ok(Some(r)),
                Err(e) => warn(format!("cannot read worker cache, fetching instead: {e}")),
            }
        }
        match SkipperConfig::from_env() {
            Some(cfg) => Self::initialize(&cfg).map(Some),
            None => Ok(None),
        }
    }

    pub fn is_test_enabled(&self, test_id: &str) -> bool {
        match self.cache.get(&normalize_test_id(test_id)) {
            Some(Some(until)) => Utc::now() >= *until,
            _ => true,
        }
    }

    /// Instant the test is re-enabled (midnight UTC after the sheet date).
    pub fn disabled_until(&self, test_id: &str) -> Option<DateTime<Utc>> {
        self.cache
            .get(&normalize_test_id(test_id))
            .copied()
            .flatten()
    }

    /// `[skipper] SKIP <id>: test disabled until <sheet date>`.
    pub fn skip_message(&self, test_id: &str) -> String {
        let mut msg = format!("[skipper] SKIP {test_id}: test disabled");
        if let Some(until) = self.disabled_until(test_id) {
            // Stored instant is midnight after the sheet date; show the sheet date.
            let day = until - chrono::Duration::days(1);
            msg.push_str(&format!(" until {}", day.format("%Y-%m-%d")));
        }
        msg
    }

    /// Normalized test ID → re-enable instant (`None`: listed, no date).
    pub fn entries(&self) -> &HashMap<String, Option<DateTime<Utc>>> {
        &self.cache
    }

    /// Serialize for the main-process → test-binary handoff
    /// (`SKIPPER_WORKER_CACHE_FILE`).
    pub fn marshal_cache(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(&self.cache)?)
    }

    pub fn from_marshaled_cache(data: &[u8]) -> Result<Self> {
        let raw: Cache = serde_json::from_slice(data)?;
        Ok(SkipperResolver {
            cache: raw
                .into_iter()
                .map(|(k, v)| (normalize_test_id(&k), v))
                .collect(),
        })
    }
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or_default()
}

fn write_disk_cache(path: &Path, cache: &Cache) {
    let payload = DiskCache {
        ts: now_secs(),
        entries: cache.clone(),
    };
    let res = serde_json::to_vec(&payload)
        .map_err(crate::Error::from)
        .and_then(|b| std::fs::write(path, b).map_err(Into::into));
    if let Err(e) = res {
        warn(format!(
            "could not write API cache to {}: {e}",
            path.display()
        ));
    }
}

fn read_disk_cache(path: &Path, ttl: u64) -> Option<(f64, Cache)> {
    let disk: DiskCache = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let age = now_secs() - disk.ts;
    (age <= ttl as f64).then_some((age, disk.entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::parse_disabled_until;
    use crate::Error;
    use std::sync::Mutex;

    // Env vars are process-global; serialize the tests that touch them.
    static ENV: Mutex<()> = Mutex::new(());

    fn entry(id: &str, date: Option<&str>) -> TestEntry {
        TestEntry {
            test_id: id.into(),
            disabled_until: date.and_then(|d| parse_disabled_until(d, 0).unwrap()),
            notes: String::new(),
            row: 0,
        }
    }

    fn with_env<T>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
        let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
        for (k, v) in vars {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        f()
    }

    #[test]
    fn opt_out_model() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("c.json");
        let r = with_env(
            &[
                ("SKIPPER_CACHE_FILE", file.to_str()),
                ("SKIPPER_CACHE_TTL", None),
            ],
            || {
                SkipperResolver::initialize_with(|| {
                    Ok(vec![
                        entry("a.rs > future", Some("2099-01-01")),
                        entry("a.rs > past", Some("2020-01-01")),
                        entry("a.rs > nodate", None),
                    ])
                })
                .unwrap()
            },
        );
        assert!(!r.is_test_enabled("A.rs  > FUTURE"));
        assert!(r.is_test_enabled("a.rs > past"));
        assert!(r.is_test_enabled("a.rs > nodate"));
        assert!(r.is_test_enabled("a.rs > unknown"));
        assert!(r.disabled_until("a.rs > future").is_some());

        let back = SkipperResolver::from_marshaled_cache(&r.marshal_cache().unwrap()).unwrap();
        assert!(!back.is_test_enabled("a.rs > future"));
        assert_eq!(back.entries().len(), 3);
    }

    #[test]
    fn falls_back_to_disk_cache_then_fail_open() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("c.json");
        let fail = || Err(Error::new("boom"));
        with_env(
            &[
                ("SKIPPER_CACHE_FILE", file.to_str()),
                ("SKIPPER_CACHE_TTL", Some("60")),
                ("SKIPPER_FAIL_OPEN", None),
            ],
            || {
                SkipperResolver::initialize_with(|| Ok(vec![entry("x > y", Some("2099-01-01"))]))
                    .unwrap();
                let cached = SkipperResolver::initialize_with(fail).unwrap();
                assert!(!cached.is_test_enabled("x > y"));
            },
        );
        with_env(&[("SKIPPER_CACHE_TTL", Some("0"))], || {
            let open = SkipperResolver::initialize_with(fail).unwrap();
            assert!(open.is_test_enabled("x > y"));
        });
        with_env(&[("SKIPPER_FAIL_OPEN", Some("FALSE"))], || {
            assert!(SkipperResolver::initialize_with(fail).is_err());
        });
        with_env(
            &[
                ("SKIPPER_FAIL_OPEN", None),
                ("SKIPPER_CACHE_TTL", None),
                ("SKIPPER_CACHE_FILE", None),
            ],
            || {},
        );
    }
}
