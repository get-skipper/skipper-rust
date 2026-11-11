use std::collections::HashMap;
use std::io::Write;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Quarantine-debt summary emitted at the end of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub generated_at: String,
    pub currently_suppressed: usize,
    pub suppressed_this_run: Vec<String>,
    pub expiring_this_week: usize,
    pub re_enabled_this_run: Vec<String>,
    pub quarantine_days_debt: i64,
}

/// `entries`: resolver state; `suppressed`: IDs skipped in this run;
/// `re_enabled`: IDs that ran although listed with an already expired date.
pub fn build_report(
    entries: &HashMap<String, Option<DateTime<Utc>>>,
    suppressed: &[String],
    re_enabled: &[String],
) -> Report {
    let now = Utc::now();
    let week = now + Duration::days(7);
    let active: Vec<DateTime<Utc>> = entries
        .values()
        .flatten()
        .copied()
        .filter(|d| *d > now)
        .collect();
    let sorted = |v: &[String]| {
        let mut v = v.to_vec();
        v.sort();
        v.dedup();
        v
    };
    Report {
        generated_at: now.to_rfc3339(),
        currently_suppressed: active.len(),
        suppressed_this_run: sorted(suppressed),
        expiring_this_week: active.iter().filter(|d| **d <= week).count(),
        re_enabled_this_run: sorted(re_enabled),
        quarantine_days_debt: active.iter().map(|d| (*d - now).num_days().max(0)).sum(),
    }
}

pub fn build_markdown_summary(r: &Report) -> String {
    let mut md = format!(
        "## Skipper Quarantine Report\n\n\
         | Metric | Value |\n\
         |--------|-------|\n\
         | Currently suppressed | {} |\n\
         | Suppressed this run | {} |\n\
         | Expiring this week | {} |\n\
         | Re-enabled this run | {} |\n\
         | Quarantine-days debt | {} |\n\n",
        r.currently_suppressed,
        r.suppressed_this_run.len(),
        r.expiring_this_week,
        r.re_enabled_this_run.len(),
        r.quarantine_days_debt
    );
    for (title, ids) in [
        ("Suppressed this run", &r.suppressed_this_run),
        ("Re-enabled this run", &r.re_enabled_this_run),
    ] {
        if !ids.is_empty() {
            md.push_str(&format!("### {title}\n\n"));
            for id in ids {
                md.push_str(&format!("- `{id}`\n"));
            }
            md.push('\n');
        }
    }
    md
}

/// Append the markdown to `$GITHUB_STEP_SUMMARY` (else print it to stdout)
/// and write `skipper-report.json` in the current directory.
pub fn emit_summary(r: &Report) -> Result<()> {
    let md = build_markdown_summary(r);
    match std::env::var("GITHUB_STEP_SUMMARY")
        .ok()
        .filter(|s| !s.is_empty())
    {
        Some(path) => std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(md.as_bytes())?,
        None => println!("{md}"),
    }
    std::fs::write("skipper-report.json", serde_json::to_vec_pretty(r)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_active_quarantine() {
        let now = Utc::now();
        let entries = HashMap::from([
            (
                "a".to_string(),
                Some(now + Duration::days(3) + Duration::hours(1)),
            ),
            (
                "b".to_string(),
                Some(now + Duration::days(30) + Duration::hours(1)),
            ),
            ("c".to_string(), Some(now - Duration::days(1))),
            ("d".to_string(), None),
        ]);
        let r = build_report(&entries, &["b".into(), "a".into()], &["c".into()]);
        assert_eq!(r.currently_suppressed, 2);
        assert_eq!(r.expiring_this_week, 1);
        assert_eq!(r.quarantine_days_debt, 33);
        assert_eq!(r.suppressed_this_run, ["a", "b"]);
        let md = build_markdown_summary(&r);
        assert!(md.starts_with("## Skipper Quarantine Report"));
        assert!(md.contains("### Re-enabled this run\n\n- `c`"));
    }
}
