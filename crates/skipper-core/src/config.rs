use crate::credentials::Credentials;

/// Configuration shared by the resolver and the writer.
#[derive(Debug, Clone)]
pub struct SkipperConfig {
    pub spreadsheet_id: String,
    pub credentials: Credentials,
    /// Primary sheet; `None` means the first sheet of the spreadsheet.
    pub sheet_name: Option<String>,
    /// Extra read-only sheets merged into the primary (latest date wins).
    pub reference_sheets: Vec<String>,
    pub test_id_column: String,
    pub disabled_until_column: String,
}

impl SkipperConfig {
    pub fn new(spreadsheet_id: impl Into<String>, credentials: Credentials) -> Self {
        SkipperConfig {
            spreadsheet_id: spreadsheet_id.into(),
            credentials,
            sheet_name: None,
            reference_sheets: Vec::new(),
            test_id_column: "testId".into(),
            disabled_until_column: "disabledUntil".into(),
        }
    }

    pub fn with_sheet_name(mut self, name: impl Into<String>) -> Self {
        self.sheet_name = Some(name.into());
        self
    }

    pub fn with_reference_sheets<I, S>(mut self, sheets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.reference_sheets = sheets.into_iter().map(Into::into).collect();
        self
    }

    /// Build a config from the environment:
    /// `SKIPPER_SPREADSHEET_ID`, credentials from `SKIPPER_CREDENTIALS_FILE`
    /// (wins) or `GOOGLE_CREDS_B64`, optional `SKIPPER_SHEET_NAME` and
    /// comma-separated `SKIPPER_REFERENCE_SHEETS`.
    /// Returns `None` when Skipper is not configured.
    pub fn from_env() -> Option<Self> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let id = var("SKIPPER_SPREADSHEET_ID")?;
        let creds = if let Some(path) = var("SKIPPER_CREDENTIALS_FILE") {
            Credentials::File(path.into())
        } else {
            Credentials::Base64(var("GOOGLE_CREDS_B64")?)
        };
        let mut cfg = SkipperConfig::new(id, creds);
        cfg.sheet_name = var("SKIPPER_SHEET_NAME");
        if let Some(refs) = var("SKIPPER_REFERENCE_SHEETS") {
            cfg.reference_sheets = refs
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect();
        }
        Some(cfg)
    }
}
