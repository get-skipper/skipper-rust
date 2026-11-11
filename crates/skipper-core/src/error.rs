use std::fmt;

/// Error type for every fallible Skipper operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(msg: impl Into<String>) -> Self {
        Error(msg.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

macro_rules! from_err {
    ($($t:ty),*) => {$(
        impl From<$t> for Error {
            fn from(e: $t) -> Self { Error(e.to_string()) }
        }
    )*};
}

from_err!(
    std::io::Error,
    serde_json::Error,
    base64::DecodeError,
    jsonwebtoken::errors::Error
);

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::Status(code, resp) => {
                let url = resp.get_url().to_string();
                let body = resp.into_string().unwrap_or_default();
                Error(format!("HTTP {code} from {url}: {body}"))
            }
            other => Error(other.to_string()),
        }
    }
}
