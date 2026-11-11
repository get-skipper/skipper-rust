use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::api::{HttpSheetsApi, SheetsApi};
use crate::client::fetch_all;
use crate::config::SkipperConfig;
use crate::error::{Error, Result};
use crate::logger::{log, warn};
use crate::testid::normalize_test_id;

/// Reconciles the primary sheet with the test IDs discovered in a run:
/// new IDs are appended (empty `disabledUntil`); stale rows of the same files
/// are deleted only with `SKIPPER_SYNC_ALLOW_DELETE=true`. Existing rows are
/// never modified.
pub struct SheetsWriter {
    config: SkipperConfig,
}

impl SheetsWriter {
    pub fn new(config: SkipperConfig) -> Self {
        SheetsWriter { config }
    }

    pub fn sync(&self, discovered: &[String]) -> Result<()> {
        let api = HttpSheetsApi::new(&self.config.credentials)?;
        self.sync_with(&api, discovered)
    }

    pub fn sync_with(&self, api: &dyn SheetsApi, discovered: &[String]) -> Result<()> {
        log(format!("syncing {} discovered test IDs", discovered.len()));
        let primary = fetch_all(api, &self.config)?.primary;
        let id_idx = primary
            .header
            .iter()
            .position(|h| *h == self.config.test_id_column)
            .ok_or_else(|| {
                Error(format!(
                    "column {:?} not found in sheet {:?}",
                    self.config.test_id_column, primary.sheet_name
                ))
            })?;

        let discovered_set: HashSet<String> =
            discovered.iter().map(|id| normalize_test_id(id)).collect();
        let mut owned_files = HashSet::new();
        let mut owned_bases = HashSet::new();
        for nid in &discovered_set {
            if let Some((file, _)) = nid.split_once(" > ") {
                owned_files.insert(file.to_string());
                owned_bases.insert(file.rsplit('/').next().unwrap_or(file).to_string());
            }
        }

        let existing: HashMap<String, &str> = primary
            .entries
            .iter()
            .map(|e| (normalize_test_id(&e.test_id), e.test_id.as_str()))
            .collect();

        let mut rows_to_delete: Vec<usize> = Vec::new();
        for e in &primary.entries {
            let nid = normalize_test_id(&e.test_id);
            let Some((file, _)) = nid.split_once(" > ") else {
                rows_to_delete.push(e.row);
                continue;
            };
            let owned =
                owned_files.contains(file) || (!file.contains('/') && owned_bases.contains(file));
            if owned && !discovered_set.contains(&nid) {
                rows_to_delete.push(e.row);
            }
        }

        let allow_delete = std::env::var("SKIPPER_SYNC_ALLOW_DELETE")
            .is_ok_and(|v| v.trim().eq_ignore_ascii_case("true"));
        if !rows_to_delete.is_empty() && !allow_delete {
            warn(format!(
                "{} orphaned row(s) found but not deleted. Set SKIPPER_SYNC_ALLOW_DELETE=true to prune them.",
                rows_to_delete.len()
            ));
            rows_to_delete.clear();
        }
        if !rows_to_delete.is_empty() {
            rows_to_delete.sort_unstable_by(|a, b| b.cmp(a));
            let requests = rows_to_delete
                .iter()
                .map(|&r| {
                    json!({"deleteDimension": {"range": {
                        "sheetId": primary.sheet_id,
                        "dimension": "ROWS",
                        "startIndex": r,
                        "endIndex": r + 1,
                    }}})
                })
                .collect();
            api.batch_update(&self.config.spreadsheet_id, requests)?;
            log(format!(
                "deleted {} rows from spreadsheet",
                rows_to_delete.len()
            ));
        }

        let mut added = HashSet::new();
        let rows: Vec<Vec<String>> = discovered
            .iter()
            .filter(|id| {
                let nid = normalize_test_id(id);
                !existing.contains_key(&nid) && added.insert(nid)
            })
            .map(|id| {
                let mut row = vec![String::new(); primary.header.len()];
                row[id_idx] = id.clone();
                row
            })
            .collect();
        if !rows.is_empty() {
            let n = rows.len();
            api.append_values(&self.config.spreadsheet_id, &primary.sheet_name, rows)?;
            log(format!("appended {n} new test IDs to spreadsheet"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::tests::{config, FakeApi};

    fn api() -> FakeApi {
        FakeApi {
            sheets: vec![(
                "main".into(),
                42,
                json!([
                    ["notes", "testId", "disabledUntil"],
                    ["", "tests/a.rs > kept", "2099-01-01"],
                    ["", "", ""],
                    ["", "tests/a.rs > gone", ""],
                    ["", "malformed", ""],
                    ["", "tests/other.rs > untouched", ""]
                ]),
            )],
            ..Default::default()
        }
    }

    // One test: both halves touch SKIPPER_SYNC_ALLOW_DELETE.
    #[test]
    fn sync_appends_and_prunes() {
        let api = api();
        std::env::remove_var("SKIPPER_SYNC_ALLOW_DELETE");
        let ids = vec![
            "tests/a.rs > Kept".to_string(),
            "tests/a.rs > new".into(),
            "tests/a.rs > NEW".into(),
        ];
        SheetsWriter::new(config()).sync_with(&api, &ids).unwrap();
        assert_eq!(
            *api.appended.borrow(),
            vec![vec!["".to_string(), "tests/a.rs > new".into(), "".into()]]
        );
        assert!(api.batches.borrow().is_empty());

        let api = self::api();
        let ids = vec!["tests/a.rs > kept".to_string()];
        std::env::set_var("SKIPPER_SYNC_ALLOW_DELETE", "true");
        let res = SheetsWriter::new(config()).sync_with(&api, &ids);
        std::env::remove_var("SKIPPER_SYNC_ALLOW_DELETE");
        res.unwrap();
        let starts: Vec<u64> = api
            .batches
            .borrow()
            .iter()
            .map(|r| {
                r["deleteDimension"]["range"]["startIndex"]
                    .as_u64()
                    .unwrap()
            })
            .collect();
        assert_eq!(starts, [4, 3]); // blank row 2 is not shifted onto
        assert_eq!(
            api.batches.borrow()[0]["deleteDimension"]["range"]["sheetId"],
            42
        );
    }
}
