/// Operating mode, read from `SKIPPER_MODE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipperMode {
    /// Default: only read the spreadsheet and skip disabled tests.
    ReadOnly,
    /// `SKIPPER_MODE=sync`: also add new tests to the spreadsheet (and prune stale ones).
    Sync,
}

impl SkipperMode {
    pub fn from_env() -> Self {
        match std::env::var("SKIPPER_MODE").as_deref() {
            Ok("sync") => SkipperMode::Sync,
            _ => SkipperMode::ReadOnly,
        }
    }
}
