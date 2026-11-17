use serde::Serialize;

/// Application-wide error type. Serialised to the frontend as `{ kind, message }`.
/// Messages must never contain secrets; provider errors are passed through
/// `security::redact` before being wrapped here.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(String),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("not supported: {0}")]
    Unsupported(String),
    #[error("not configured: {0}")]
    NotConfigured(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Database(_) => "database",
            AppError::Keychain(_) => "keychain",
            AppError::Network(_) => "network",
            AppError::Auth(_) => "auth",
            AppError::RateLimited(_) => "rate_limited",
            AppError::Provider(_) => "provider",
            AppError::Unsupported(_) => "unsupported",
            AppError::NotConfigured(_) => "not_configured",
            AppError::Invalid(_) => "invalid",
            AppError::Io(_) => "io",
            AppError::Other(_) => "other",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("kind", self.kind())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Database(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Invalid(e.to_string())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        AppError::Other(format!("background task failed: {e}"))
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        // reqwest errors include the URL; our URLs never contain secrets
        // (keys are always sent in headers), so this is safe to surface.
        if e.is_timeout() {
            AppError::Network("request timed out".into())
        } else if e.is_connect() {
            AppError::Network("could not connect to server".into())
        } else {
            AppError::Network(e.without_url().to_string())
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
