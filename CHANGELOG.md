# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] – 2026-11-11

### Added

- `skipper-core`: resolver (opt-out model, strict `YYYY-MM-DD` dates expiring at
  midnight UTC after the given day), Google Sheets client with service-account
  JWT auth, reference-sheet merge (latest date wins), sync writer, disk cache
  fallback (`SKIPPER_CACHE_TTL`), fail-open (`SKIPPER_FAIL_OPEN`) and quarantine
  report (`skipper-report.json`, `GITHUB_STEP_SUMMARY`).
- `get-skipper`: `#[skipper]` attribute for `#[test]`, `#[rstest]` and
  `#[tokio::test]`, plus `skip_if_disabled!()`.
- `skipper-cucumber`: scenario gating for cucumber-rs.
- `cargo-skipper`: `cargo skipper <subcommand>` runner — one fetch per run,
  report and `SKIPPER_MODE=sync`.

[Unreleased]: https://github.com/get-skipper/skipper-rust/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/get-skipper/skipper-rust/releases/tag/v1.0.0
