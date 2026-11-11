# skipper-rust

[![CI](https://github.com/get-skipper/skipper-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/get-skipper/skipper-rust/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/get-skipper.svg)](https://crates.io/crates/get-skipper)
[![License](https://img.shields.io/github/license/get-skipper/skipper-rust)](LICENSE)

Test-gating for Rust via Google Spreadsheet. Enable or disable tests without changing code — just update a date in a Google Sheet.

A Rust port of [get-skipper/skipper](https://github.com/get-skipper/skipper), supporting the built-in test harness (`cargo test`), [rstest](https://github.com/la10736/rstest), `#[tokio::test]` and [cucumber-rs](https://github.com/cucumber-rs/cucumber).

---

## How it works

A Google Spreadsheet stores test IDs with optional `disabledUntil` dates:

| testId | disabledUntil | notes |
|--------|---------------|-------|
| `tests/auth.rs > can_log_in` | | |
| `tests/payment.rs > checkout::with_valid_card` | `2099-12-31` | Flaky on CI |
| `tests/features/auth.feature > Auth > Login works` | `2026-06-01` | Under investigation |

- **Empty `disabledUntil`** → test runs normally
- **Past date** → test runs normally
- **Future date** → test is skipped (it stays disabled through the whole `disabledUntil` day, UTC)

Tests not listed in the spreadsheet **always run** (opt-out model).

---

## Crates

| Crate | Use it for |
|-------|-----------|
| [`get-skipper`](https://crates.io/crates/get-skipper) | `#[skipper]` attribute for `#[test]`, `#[rstest]`, `#[tokio::test]` |
| [`skipper-cucumber`](https://crates.io/crates/skipper-cucumber) | cucumber-rs scenarios |
| [`cargo-skipper`](https://crates.io/crates/cargo-skipper) | `cargo skipper test`: one fetch per run, quarantine report, sync mode |
| [`skipper-core`](https://crates.io/crates/skipper-core) | Shared core (resolver, Sheets client, writer, caches) |
| `skipper-macros` | Proc-macro behind `#[skipper]` (re-exported by `get-skipper`) |

The facade crate is `get-skipper` because `skipper` is taken on crates.io.

---

## Installation

```bash
cargo add --dev get-skipper          # libtest / rstest / tokio
cargo add --dev skipper-cucumber     # cucumber-rs
cargo install cargo-skipper          # optional runner (report + sync)
```

---

## Google Sheets setup

1. Create a Google Spreadsheet with the following columns in row 1:
   - `testId`
   - `disabledUntil`
   - `notes` (optional)

2. Create a Google Cloud service account and download the JSON key file.

3. Share the spreadsheet with the service account's email (`client_email` in the JSON).

4. Note the spreadsheet ID from the URL:
   `https://docs.google.com/spreadsheets/d/YOUR_SPREADSHEET_ID/edit`

---

## Configuration

Rust's test harness has no global setup hook, so Skipper is configured with environment variables — conveniently set in `.cargo/config.toml`:

```toml
[env]
SKIPPER_SPREADSHEET_ID = "your-spreadsheet-id"
SKIPPER_CREDENTIALS_FILE = { value = "service-account.json", relative = true }
SKIPPER_SHEET_NAME = "my-project"          # optional, default: first sheet
```

Credentials, in order of precedence:

| Variable | Use case |
|----------|----------|
| `SKIPPER_CREDENTIALS_FILE` | Path to the service-account JSON (local development) |
| `GOOGLE_CREDS_B64` | Base64-encoded service-account JSON (CI secrets) |

Without a spreadsheet ID and credentials, Skipper does nothing and every test runs.
For programmatic use, `skipper_core::SkipperConfig` also accepts
`Credentials::ServiceAccount(...)` with inline fields.

---

## Usage

### `cargo test` (libtest), rstest, tokio

```rust
use get_skipper::skipper;

#[skipper]
#[test]
fn can_log_in() {
    // ...
}

#[skipper]
#[test]
fn returns_result() -> Result<(), String> {
    Ok(()) // a skipped test returns Ok(())
}

#[skipper]
#[rstest]
#[case(1)]
#[case(2)]
fn parametrized(#[case] n: u32) {
    // all cases share one ID: `tests/auth.rs > parametrized`
}

#[skipper]
#[tokio::test]
async fn async_test() {}
```

Put `#[skipper]` **first**, above the test attribute. Where an attribute does not fit, call the macro at the top of the test body:

```rust
#[test]
fn legacy() {
    get_skipper::skip_if_disabled!();
}
```

**Test ID format:** `{file} > {module path} > {fn name}`, with the crate name dropped:

| Test | ID |
|------|----|
| `fn can_log_in` in `tests/auth.rs` | `tests/auth.rs > can_log_in` |
| `fn works` in `mod tests` of `src/lib.rs` (crate in a workspace) | `crates/foo/src/lib.rs > tests > works` |

> **Note:** libtest cannot mark a test as skipped at runtime, so a disabled test returns early and is
> reported as `ok`. Skipper prints `[skipper] SKIP <id>: test disabled until <date>` on stderr
> (visible even without `--nocapture`), and the quarantine report lists it.

### `cargo skipper` (recommended in CI)

```bash
cargo skipper test --workspace        # any cargo subcommand works:
cargo skipper nextest run             # cargo-nextest
```

`cargo skipper` fetches the spreadsheet **once**, hands the result to every test binary, then:

- writes the quarantine report to `skipper-report.json` and `$GITHUB_STEP_SUMMARY` (or stdout);
- with `SKIPPER_MODE=sync`, reconciles the spreadsheet with the tests that ran.

Plain `cargo test` works too (read-only): each test binary fetches the sheet, with a
`.skipper-cache.json` disk cache shared between them.

### cucumber-rs

```rust
// tests/bdd.rs  (with `harness = false` for this [[test]] in Cargo.toml)
use cucumber::World;

#[derive(Debug, Default, World)]
struct MyWorld;

#[tokio::main]
async fn main() {
    skipper_cucumber::run::<MyWorld>("tests/features").await;
}
```

Disabled scenarios are filtered out of the run. For a custom `Cucumber` builder:

```rust
let skipper = skipper_cucumber::Skipper::from_env();
MyWorld::cucumber()
    .max_concurrent_scenarios(1)
    .filter_run("tests/features", skipper.filter())
    .await;
skipper.finish(); // report + sync
```

**Test ID format:** `tests/features/auth.feature > Feature > [Rule >] Scenario`

---

## Modes

| Mode | Env var | Behavior |
|------|---------|----------|
| `read-only` (default) | — | Fetch spreadsheet, skip disabled tests |
| `sync` | `SKIPPER_MODE=sync` | Same as above, then reconcile spreadsheet with discovered tests |

Sync runs from `cargo skipper` (or from `skipper_cucumber::run`). New tests are added to the
spreadsheet; existing rows are never modified. Deletion of orphaned rows requires
`SKIPPER_SYNC_ALLOW_DELETE=true`. Only tests that actually run are discovered, so run the
full suite when syncing with deletion enabled.

---

## Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SKIPPER_SPREADSHEET_ID` | — | Spreadsheet to read. Required to enable Skipper |
| `SKIPPER_CREDENTIALS_FILE` | — | Service-account JSON file (wins over `GOOGLE_CREDS_B64`) |
| `GOOGLE_CREDS_B64` | — | Base64-encoded service-account JSON |
| `SKIPPER_SHEET_NAME` | first sheet | Primary sheet name |
| `SKIPPER_REFERENCE_SHEETS` | — | Comma-separated read-only sheets to merge in |
| `SKIPPER_MODE` | `read-only` | Set to `sync` to enable sync mode |
| `SKIPPER_DEBUG` | — | Any non-empty value enables verbose debug logging |
| `SKIPPER_FAIL_OPEN` | `true` | When the API is unreachable and no fresh disk cache exists, run all tests. Set to `false` to fail instead |
| `SKIPPER_CACHE_TTL` | `300` | Seconds the on-disk cache stays valid as an API fallback. `0` disables it |
| `SKIPPER_CACHE_FILE` | `.skipper-cache.json` | Location of the on-disk cache |
| `SKIPPER_SYNC_ALLOW_DELETE` | `false` | In sync mode, also delete rows of tests that no longer exist |

`SKIPPER_WORKER_CACHE_FILE` and `SKIPPER_DISCOVERED_DIR` are set by `cargo skipper` for the test binaries.

---

## Test ID matching

- Paths are relative to the workspace root (the outermost `Cargo.toml` with `[workspace]`), using `/`
- Title parts are separated by ` > `
- IDs are case-insensitive and whitespace-collapsed for matching

---

## Reference sheets

Inherit disabled tests from additional read-only sheets:

```bash
SKIPPER_REFERENCE_SHEETS=GlobalDisabled,FlakyTests
```

When the same test ID appears in multiple sheets, the most restrictive (latest) `disabledUntil` date wins.

---

## CI example

```yaml
- run: cargo install cargo-skipper
- name: Run tests
  run: cargo skipper test --workspace
  env:
    SKIPPER_SPREADSHEET_ID: ${{ secrets.SKIPPER_SPREADSHEET_ID }}
    GOOGLE_CREDS_B64: ${{ secrets.GOOGLE_CREDS_B64 }}
```

---

## License

[MIT](LICENSE)
