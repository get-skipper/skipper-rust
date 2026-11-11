use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Result;
use crate::logger::{log, warn};

const CACHE_FILE_NAME: &str = "cache.json";

/// Temp directory shared between the orchestrating process (`cargo skipper`)
/// and the test binaries: it holds the resolver handoff (`cache.json`) and one
/// JSON array of discovered test IDs per writer.
pub struct CacheManager;

impl CacheManager {
    /// Create `skipper-*` in the temp dir and write `cache.json` into it.
    /// Point `SKIPPER_WORKER_CACHE_FILE` at the returned file and
    /// `SKIPPER_DISCOVERED_DIR` at its parent.
    pub fn write_resolver_cache(data: &[u8]) -> Result<PathBuf> {
        let dir = std::env::temp_dir().join(format!("skipper-{}", unique()));
        std::fs::create_dir_all(&dir)?;
        let file = dir.join(CACHE_FILE_NAME);
        std::fs::write(&file, data)?;
        log(format!("wrote resolver cache to {}", file.display()));
        Ok(file)
    }

    /// Write `ids` to a uniquely named file in `dir`.
    pub fn write_discovered_ids(dir: &Path, ids: &[String]) -> Result<()> {
        let file = dir.join(format!("{}.json", unique()));
        std::fs::write(file, serde_json::to_vec(ids)?)?;
        Ok(())
    }

    /// Merge every discovered-ID file (sorted by name), deduplicated on the exact string.
    pub fn merge_discovered_ids(dir: &Path) -> Vec<String> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        files.retain(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_name().is_some_and(|n| n != CACHE_FILE_NAME)
        });
        files.sort();

        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for f in files {
            match std::fs::read(&f)
                .map_err(crate::Error::from)
                .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).map_err(Into::into))
            {
                Ok(ids) => out.extend(ids.into_iter().filter(|id| seen.insert(id.clone()))),
                Err(e) => warn(format!("cannot read discovered file {}: {e}", f.display())),
            }
        }
        out
    }

    pub fn cleanup(dir: &Path) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// `{pid}-{nanos}-{counter}`: unique across processes and threads.
fn unique() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!(
        "{}-{nanos}-{:08x}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_merge() {
        let file = CacheManager::write_resolver_cache(b"{}").unwrap();
        let dir = file.parent().unwrap();
        CacheManager::write_discovered_ids(dir, &["a".into(), "b".into()]).unwrap();
        CacheManager::write_discovered_ids(dir, &["b".into(), "c".into()]).unwrap();
        std::fs::write(dir.join("broken.json"), "nope").unwrap();
        let mut ids = CacheManager::merge_discovered_ids(dir);
        ids.sort();
        assert_eq!(ids, ["a", "b", "c"]);
        CacheManager::cleanup(dir);
        assert!(!dir.exists());
    }
}
