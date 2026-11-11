use std::path::PathBuf;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Google service-account credentials, in one of three formats.
#[derive(Clone)]
pub enum Credentials {
    /// Path to the service-account JSON key file.
    File(PathBuf),
    /// Base64-encoded service-account JSON (e.g. a CI secret).
    Base64(String),
    /// Inline service-account fields.
    ServiceAccount(Box<ServiceAccountCredentials>),
}

impl std::fmt::Debug for Credentials {
    // Never print key material.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Credentials::File(p) => f.debug_tuple("File").field(p).finish(),
            Credentials::Base64(_) => f.write_str("Base64(<redacted>)"),
            Credentials::ServiceAccount(sa) => f
                .debug_struct("ServiceAccount")
                .field("client_email", &sa.client_email)
                .finish_non_exhaustive(),
        }
    }
}

/// The fields of a Google service-account JSON key.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ServiceAccountCredentials {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub private_key_id: String,
    pub private_key: String,
    pub client_email: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub auth_uri: String,
    #[serde(default)]
    pub token_uri: String,
    #[serde(default)]
    pub auth_provider_x509_cert_url: String,
    #[serde(default)]
    pub client_x509_cert_url: String,
}

impl Credentials {
    /// Return the raw service-account JSON bytes.
    pub fn resolve(&self) -> Result<Vec<u8>> {
        Ok(match self {
            Credentials::File(path) => std::fs::read(path).map_err(|e| {
                crate::Error(format!(
                    "cannot read credentials file {}: {e}",
                    path.display()
                ))
            })?,
            Credentials::Base64(enc) => {
                base64::engine::general_purpose::STANDARD.decode(enc.trim())?
            }
            Credentials::ServiceAccount(sa) => serde_json::to_vec(sa)?,
        })
    }

    /// Parse the resolved JSON into service-account fields.
    pub fn service_account(&self) -> Result<ServiceAccountCredentials> {
        Ok(serde_json::from_slice(&self.resolve()?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{"type":"service_account","private_key":"k","client_email":"a@b.c","token_uri":"https://t"}"#;

    #[test]
    fn base64_and_file_and_inline_resolve_to_same_account() {
        let b64 = base64::engine::general_purpose::STANDARD.encode(JSON);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sa.json");
        std::fs::write(&path, JSON).unwrap();

        for c in [Credentials::Base64(b64), Credentials::File(path)] {
            let sa = c.service_account().unwrap();
            assert_eq!(sa.client_email, "a@b.c");
            assert_eq!(sa.token_uri, "https://t");
        }
        let inline = Credentials::ServiceAccount(Box::new(ServiceAccountCredentials {
            client_email: "x@y.z".into(),
            private_key: "k".into(),
            ..Default::default()
        }));
        assert_eq!(inline.service_account().unwrap().client_email, "x@y.z");
    }

    #[test]
    fn debug_redacts_secrets() {
        let s = format!("{:?}", Credentials::Base64("c2VjcmV0".into()));
        assert!(!s.contains("c2VjcmV0"));
    }
}
