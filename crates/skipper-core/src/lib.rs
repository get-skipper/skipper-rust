//! Core of [Skipper](https://get-skipper.github.io): test-gating via Google Spreadsheet.
//!
//! Most users want the `get-skipper` crate (the `#[skipper]` attribute) or
//! `skipper-cucumber`. This crate holds the shared pieces: the resolver, the
//! Google Sheets client, the sync writer, caches and the quarantine report.

mod api;
mod cache;
mod client;
mod config;
mod credentials;
mod error;
mod logger;
mod mode;
mod report;
mod resolver;
mod testid;
mod writer;

pub use api::{HttpSheetsApi, SheetsApi};
pub use cache::CacheManager;
pub use client::{fetch_all, parse_disabled_until, FetchAllResult, SheetFetchResult, TestEntry};
pub use config::SkipperConfig;
pub use credentials::{Credentials, ServiceAccountCredentials};
pub use error::{Error, Result};
pub use logger::{debug_enabled, log, warn};
pub use mode::SkipperMode;
pub use report::{build_markdown_summary, build_report, emit_summary, Report};
pub use resolver::SkipperResolver;
pub use testid::{build_test_id, find_project_root, normalize_test_id, relative_to_root};
pub use writer::SheetsWriter;
