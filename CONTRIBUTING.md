# Contributing to skipper-rust

Thank you for your interest in contributing!

---

## Requirements

- Rust 1.88+ (stable)
- A Google Cloud service account (for integration testing against a real spreadsheet)

## Setup

```bash
git clone https://github.com/get-skipper/skipper-rust.git
cd skipper-rust
cargo build --workspace
```

Put `service-account-skipper-bot.json` at the repository root (gitignored) or set
`GOOGLE_CREDS_B64` to run the integration tests; without it they are skipped.

## Running tests

```bash
make test
```

## Linting

```bash
make lint
```

Linting must pass before a pull request can be merged.

---

## Commit messages

All commits **must** follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
type(scope): short description

[optional body]

[optional footer]
```

### Types

| Type | When to use |
|------|-------------|
| `feat` | A new feature or integration |
| `fix` | A bug fix |
| `docs` | Documentation changes only |
| `refactor` | Code change that neither fixes a bug nor adds a feature |
| `test` | Adding or updating tests |
| `chore` | Build process, dependency updates, tooling |

### Scopes

| Scope | Applies to |
|-------|------------|
| `core` | `crates/skipper-core` |
| `macros` | `crates/skipper-macros` |
| `skipper` | `crates/get-skipper` |
| `cucumber` | `crates/skipper-cucumber` |
| `cargo` | `crates/cargo-skipper` |

### Examples

```
feat(macros): support #[skipper] on tests returning Result
fix(core): keep the sheet row index when blank rows are present
docs(cucumber): document the custom builder setup
test(core): cover reference-sheet merge collisions
chore: bump ureq
```

---

## Changelog

All notable changes are recorded in [CHANGELOG.md](CHANGELOG.md) following the
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format.

- Entries live under `## [Unreleased]` until a release is cut.
- Every change belongs to one of: `Added`, `Changed`, `Deprecated`, `Removed`,
  `Fixed`, `Security`.
- When a new version is released, rename `## [Unreleased]` to
  `## [x.y.z] – YYYY-MM-DD`, add a fresh empty `## [Unreleased]` section at
  the top, and append a comparison link at the bottom of the file.

---

## Pull requests

1. Fork the repository and create a branch:
   ```bash
   git checkout -b feat/my-feature
   ```

2. Make your changes. Ensure:
   - `make test` passes
   - `make lint` passes
   - New functionality has corresponding unit tests

3. Commit using Conventional Commits format (see above).

4. Open a pull request with a clear title and description.

---

## Project structure

```
crates/skipper-core/      # Resolver, Sheets client (JWT auth), writer, caches, report
crates/skipper-macros/    # #[skipper] proc-macro
crates/get-skipper/       # Facade: re-exports + skip_if_disabled!()
crates/skipper-cucumber/  # cucumber-rs integration
crates/cargo-skipper/     # `cargo skipper` runner (handoff cache, report, sync)
```

Each framework integration should:
- Build its resolver with `SkipperResolver::from_env()` (handoff cache first, then a fetch)
- Skip disabled tests the framework's way and print `resolver.skip_message(id)`
- Write discovered test IDs to `SKIPPER_DISCOVERED_DIR` when set
- Otherwise emit the report and run `SheetsWriter::sync()` itself in sync mode

