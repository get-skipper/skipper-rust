use std::collections::HashMap;

use chrono::{DateTime, Days, NaiveDate, Utc};
use serde_json::Value;

use crate::api::SheetsApi;
use crate::config::SkipperConfig;
use crate::error::{Error, Result};
use crate::logger::{log, warn};
use crate::testid::normalize_test_id;

/// One spreadsheet row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestEntry {
    pub test_id: String,
    /// Instant the test is re-enabled (midnight UTC after the sheet date);
    /// `None` means no date, i.e. enabled.
    pub disabled_until: Option<DateTime<Utc>>,
    pub notes: String,
    /// 0-based row index in the sheet (0 is the header).
    pub row: usize,
}

/// A single fetched sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetFetchResult {
    pub sheet_name: String,
    pub sheet_id: i64,
    pub header: Vec<String>,
    pub entries: Vec<TestEntry>,
}

/// The primary sheet plus all entries merged with the reference sheets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchAllResult {
    pub primary: SheetFetchResult,
    pub entries: Vec<TestEntry>,
}

/// Fetch the primary sheet and merge every reference sheet into it.
/// A failing reference sheet is skipped with a warning; a failing primary is an error.
pub fn fetch_all(api: &dyn SheetsApi, config: &SkipperConfig) -> Result<FetchAllResult> {
    let meta = api.get_spreadsheet(&config.spreadsheet_id)?;
    let primary_name = match config.sheet_name.as_deref().filter(|s| !s.is_empty()) {
        Some(name) => name.to_string(),
        None => meta["sheets"][0]["properties"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    };

    let primary = fetch_sheet(api, config, &meta, &primary_name)?;
    let mut merged = merge_entries(Vec::new(), primary.entries.clone());
    for name in &config.reference_sheets {
        match fetch_sheet(api, config, &meta, name) {
            Ok(r) => merged = merge_entries(merged, r.entries),
            Err(e) => warn(format!("cannot fetch reference sheet {name:?}: {e}")),
        }
    }
    Ok(FetchAllResult {
        primary,
        entries: merged,
    })
}

fn fetch_sheet(
    api: &dyn SheetsApi,
    config: &SkipperConfig,
    meta: &Value,
    sheet_name: &str,
) -> Result<SheetFetchResult> {
    let resp = api.get_values(&config.spreadsheet_id, sheet_name)?;
    let sheet_id = sheet_id_by_name(meta, sheet_name);
    let rows: Vec<Vec<String>> = resp["values"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    r.as_array()
                        .map(|c| c.iter().map(cell).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();

    let Some((header_row, data)) = rows.split_first() else {
        return Ok(SheetFetchResult {
            sheet_name: sheet_name.into(),
            sheet_id,
            header: Vec::new(),
            entries: Vec::new(),
        });
    };
    let header: Vec<String> = header_row.iter().map(|h| h.trim().to_string()).collect();
    let col = |name: &str| header.iter().position(|h| h == name);
    let id_idx = col(&config.test_id_column).ok_or_else(|| {
        Error(format!(
            "column {:?} not found in sheet {sheet_name:?}",
            config.test_id_column
        ))
    })?;
    let until_idx = col(&config.disabled_until_column);
    let notes_idx = col("notes");

    let mut entries = Vec::new();
    for (i, row) in data.iter().enumerate() {
        let test_id = row.get(id_idx).map(|s| s.trim()).unwrap_or_default();
        if test_id.is_empty() {
            continue;
        }
        let disabled_until = match until_idx.and_then(|j| row.get(j)) {
            Some(raw) => parse_disabled_until(raw, i + 2)?,
            None => None,
        };
        let notes = notes_idx
            .and_then(|j| row.get(j))
            .cloned()
            .unwrap_or_default();
        entries.push(TestEntry {
            test_id: test_id.to_string(),
            disabled_until,
            notes,
            row: i + 1,
        });
    }
    log(format!(
        "fetched {} entries from sheet {sheet_name:?}",
        entries.len()
    ));
    Ok(SheetFetchResult {
        sheet_name: sheet_name.into(),
        sheet_id,
        header,
        entries,
    })
}

fn cell(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn sheet_id_by_name(meta: &Value, name: &str) -> i64 {
    meta["sheets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["properties"]["title"].as_str() == Some(name))
        .and_then(|s| s["properties"]["sheetId"].as_i64())
        .unwrap_or(0)
}

/// Parse a `disabledUntil` cell. Only strict `YYYY-MM-DD` is accepted; the
/// result is midnight UTC of the *following* day, so the test stays disabled
/// for the whole calendar day. Empty means no date. `row` (1-based sheet row,
/// 0 = unknown) only appears in the error message.
pub fn parse_disabled_until(raw: &str, row: usize) -> Result<Option<DateTime<Utc>>> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let shaped = s.len() == 10
        && s.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        });
    let date = shaped
        .then(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .flatten()
        .and_then(|d| d.checked_add_days(Days::new(1)))
        .ok_or_else(|| {
            let loc = if row > 0 {
                format!("row {row}: ")
            } else {
                String::new()
            };
            Error(format!(
                "[skipper] {loc}invalid disabledUntil {s:?}. Use YYYY-MM-DD."
            ))
        })?;
    Ok(Some(
        date.and_hms_opt(0, 0, 0).unwrap_or_default().and_utc(),
    ))
}

/// Merge `incoming` into `existing` by normalized ID; on collision the later
/// (more restrictive) date wins and a date always beats no date.
fn merge_entries(mut existing: Vec<TestEntry>, incoming: Vec<TestEntry>) -> Vec<TestEntry> {
    let mut idx: HashMap<String, usize> = existing
        .iter()
        .enumerate()
        .map(|(i, e)| (normalize_test_id(&e.test_id), i))
        .collect();
    for e in incoming {
        let nid = normalize_test_id(&e.test_id);
        match idx.get(&nid) {
            Some(&i) => {
                if e.disabled_until > existing[i].disabled_until {
                    existing[i].disabled_until = e.disabled_until;
                }
            }
            None => {
                idx.insert(nid, existing.len());
                existing.push(e);
            }
        }
    }
    existing
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::credentials::Credentials;
    use serde_json::json;
    use std::cell::RefCell;

    #[test]
    fn date_is_next_midnight_utc() {
        let d = parse_disabled_until(" 2026-04-01 ", 0).unwrap().unwrap();
        assert_eq!(d.to_rfc3339(), "2026-04-02T00:00:00+00:00");
        assert_eq!(parse_disabled_until("  ", 0).unwrap(), None);
    }

    #[test]
    fn rejects_bad_dates() {
        for bad in [
            "2026-4-01",
            "01/04/2026",
            "2026-02-30",
            "2026-04-01T00:00:00",
            "２026-04-01",
        ] {
            assert!(parse_disabled_until(bad, 3).is_err(), "{bad}");
        }
        let e = parse_disabled_until("nope", 7).unwrap_err();
        assert!(e.0.contains("row 7"), "{e}");
    }

    #[test]
    fn merge_keeps_latest_date() {
        let d = |s| parse_disabled_until(s, 0).unwrap();
        let e = |id: &str, until| TestEntry {
            test_id: id.into(),
            disabled_until: until,
            notes: String::new(),
            row: 0,
        };
        let merged = merge_entries(
            vec![e("a > x", d("2026-01-01")), e("b > y", None)],
            vec![
                e("A >  X", d("2025-01-01")),
                e("b > y", d("2027-01-01")),
                e("c > z", None),
            ],
        );
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].disabled_until, d("2026-01-01"));
        assert_eq!(merged[1].disabled_until, d("2027-01-01"));
    }

    /// In-memory Sheets fake shared with the writer tests.
    #[derive(Default)]
    pub(crate) struct FakeApi {
        pub sheets: Vec<(String, i64, Value)>,
        pub appended: RefCell<Vec<Vec<String>>>,
        pub batches: RefCell<Vec<Value>>,
    }

    impl SheetsApi for FakeApi {
        fn get_spreadsheet(&self, _: &str) -> Result<Value> {
            let sheets: Vec<Value> = self
                .sheets
                .iter()
                .map(|(t, id, _)| json!({"properties": {"title": t, "sheetId": id}}))
                .collect();
            Ok(json!({ "sheets": sheets }))
        }
        fn get_values(&self, _: &str, range: &str) -> Result<Value> {
            self.sheets
                .iter()
                .find(|(t, _, _)| t == range)
                .map(|(_, _, v)| json!({ "values": v }))
                .ok_or_else(|| Error(format!("no sheet {range}")))
        }
        fn append_values(&self, _: &str, _: &str, rows: Vec<Vec<String>>) -> Result<()> {
            self.appended.borrow_mut().extend(rows);
            Ok(())
        }
        fn batch_update(&self, _: &str, requests: Vec<Value>) -> Result<()> {
            self.batches.borrow_mut().extend(requests);
            Ok(())
        }
    }

    pub(crate) fn config() -> SkipperConfig {
        SkipperConfig::new("sid", Credentials::Base64(String::new()))
    }

    #[test]
    fn fetches_primary_and_reference_sheets() {
        let api = FakeApi {
            sheets: vec![
                (
                    "main".into(),
                    11,
                    json!([
                        [" testId ", "disabledUntil", "notes"],
                        ["a.rs > t1", "2099-01-01", "flaky"],
                        ["", "2099-01-01"],
                        ["a.rs > t2"]
                    ]),
                ),
                (
                    "ref".into(),
                    12,
                    json!([["testId", "disabledUntil"], ["A.rs > T2", "2099-06-01"]]),
                ),
            ],
            ..Default::default()
        };
        let mut cfg = config().with_reference_sheets(["ref", "missing"]);
        let r = fetch_all(&api, &cfg).unwrap();
        assert_eq!(r.primary.sheet_name, "main");
        assert_eq!(r.primary.sheet_id, 11);
        assert_eq!(r.primary.entries.len(), 2);
        assert_eq!(r.primary.entries[0].notes, "flaky");
        assert_eq!(
            r.entries[1].disabled_until,
            parse_disabled_until("2099-06-01", 0).unwrap()
        );

        cfg.test_id_column = "id".into();
        assert!(fetch_all(&api, &cfg)
            .unwrap_err()
            .0
            .contains("column \"id\" not found"));
    }

    #[test]
    fn invalid_date_fails_fetch() {
        let api = FakeApi {
            sheets: vec![(
                "s".into(),
                1,
                json!([["testId", "disabledUntil"], ["x > y", "soon"]]),
            )],
            ..Default::default()
        };
        assert!(fetch_all(&api, &config()).unwrap_err().0.contains("row 2"));
    }
}
