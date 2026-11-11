use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::credentials::{Credentials, ServiceAccountCredentials};
use crate::error::{Error, Result};

const SCOPE: &str = "https://www.googleapis.com/auth/spreadsheets";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const SHEETS_BASE: &str = "https://sheets.googleapis.com/v4/spreadsheets";

/// The four Google Sheets v4 calls Skipper needs. Implemented over HTTP by
/// [`HttpSheetsApi`]; tests can provide an in-memory fake.
pub trait SheetsApi {
    /// `GET /spreadsheets/{id}` — metadata (sheet titles and ids).
    fn get_spreadsheet(&self, spreadsheet_id: &str) -> Result<Value>;
    /// `GET /spreadsheets/{id}/values/{range}`.
    fn get_values(&self, spreadsheet_id: &str, range: &str) -> Result<Value>;
    /// `POST /spreadsheets/{id}/values/{range}:append?valueInputOption=RAW`.
    fn append_values(
        &self,
        spreadsheet_id: &str,
        range: &str,
        rows: Vec<Vec<String>>,
    ) -> Result<()>;
    /// `POST /spreadsheets/{id}:batchUpdate`.
    fn batch_update(&self, spreadsheet_id: &str, requests: Vec<Value>) -> Result<()>;
}

/// Sheets API over HTTPS, authenticated with a service-account JWT
/// (OAuth 2.0 `jwt-bearer` grant). The access token is cached until shortly
/// before it expires.
pub struct HttpSheetsApi {
    account: ServiceAccountCredentials,
    agent: ureq::Agent,
    token: Mutex<Option<(String, Instant)>>,
}

impl HttpSheetsApi {
    pub fn new(credentials: &Credentials) -> Result<Self> {
        let account = credentials.service_account()?;
        if account.client_email.is_empty() || account.private_key.is_empty() {
            return Err(Error::new(
                "credentials missing client_email or private_key",
            ));
        }
        Ok(HttpSheetsApi {
            account,
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(30))
                .build(),
            token: Mutex::new(None),
        })
    }

    fn access_token(&self) -> Result<String> {
        let mut guard = self.token.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((tok, exp)) = guard.as_ref() {
            if Instant::now() < *exp {
                return Ok(tok.clone());
            }
        }
        let token_uri = if self.account.token_uri.is_empty() {
            DEFAULT_TOKEN_URI
        } else {
            &self.account.token_uri
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error(e.to_string()))?
            .as_secs();
        let claims = json!({
            "iss": self.account.client_email,
            "scope": SCOPE,
            "aud": token_uri,
            "iat": now,
            "exp": now + 3600,
        });
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        if !self.account.private_key_id.is_empty() {
            header.kid = Some(self.account.private_key_id.clone());
        }
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(self.account.private_key.as_bytes())?;
        let assertion = jsonwebtoken::encode(&header, &claims, &key)?;

        let resp: Value = self
            .agent
            .post(token_uri)
            .send_form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &assertion),
            ])?
            .into_json()?;
        let tok = resp["access_token"]
            .as_str()
            .ok_or_else(|| Error::new("token response without access_token"))?
            .to_string();
        let ttl = resp["expires_in"]
            .as_u64()
            .unwrap_or(3600)
            .saturating_sub(60);
        *guard = Some((tok.clone(), Instant::now() + Duration::from_secs(ttl)));
        Ok(tok)
    }

    fn bearer(&self) -> Result<String> {
        Ok(format!("Bearer {}", self.access_token()?))
    }
}

impl SheetsApi for HttpSheetsApi {
    fn get_spreadsheet(&self, id: &str) -> Result<Value> {
        let url = format!("{SHEETS_BASE}/{}", encode(id));
        Ok(self
            .agent
            .get(&url)
            .set("Authorization", &self.bearer()?)
            .call()?
            .into_json()?)
    }

    fn get_values(&self, id: &str, range: &str) -> Result<Value> {
        let url = format!("{SHEETS_BASE}/{}/values/{}", encode(id), encode(range));
        Ok(self
            .agent
            .get(&url)
            .set("Authorization", &self.bearer()?)
            .call()?
            .into_json()?)
    }

    fn append_values(&self, id: &str, range: &str, rows: Vec<Vec<String>>) -> Result<()> {
        let url = format!(
            "{SHEETS_BASE}/{}/values/{}:append?valueInputOption=RAW",
            encode(id),
            encode(range)
        );
        self.agent
            .post(&url)
            .set("Authorization", &self.bearer()?)
            .send_json(json!({ "values": rows }))?;
        Ok(())
    }

    fn batch_update(&self, id: &str, requests: Vec<Value>) -> Result<()> {
        let url = format!("{SHEETS_BASE}/{}:batchUpdate", encode(id));
        self.agent
            .post(&url)
            .set("Authorization", &self.bearer()?)
            .send_json(json!({ "requests": requests }))?;
        Ok(())
    }
}

/// Percent-encode a URL path segment (RFC 3986 unreserved chars kept).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn encodes_path_segments() {
        assert_eq!(super::encode("My Sheet/1'"), "My%20Sheet%2F1%27");
        assert_eq!(super::encode("skipper-rust"), "skipper-rust");
    }
}
