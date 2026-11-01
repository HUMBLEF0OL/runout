//! `AppError` exactly as API.md §1: serializes to `{code, message, existing_id?, retry_after_s?, detail?}`.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code {
    NotFound,
    Validation,
    Duplicate,
    InvalidLink,
    UnsupportedLink,
    Offline,
    Io,
    Integrity,
    Conflict,
    SpotifyAuth,
    SpotifyForbidden,
    SpotifyBusy,
    ImportUnrecognized,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Serialize, thiserror::Error)]
#[error("{code:?}: {message}")]
pub struct AppError {
    pub code: Code,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_s: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            existing_id: None,
            retry_after_s: None,
            detail: None,
        }
    }
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(Code::Validation, message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(Code::NotFound, message)
    }
    pub fn offline() -> Self {
        Self::new(Code::Offline, "You're offline.")
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(Code::Internal, message)
    }
    /// FR-004: the UI offers "open existing".
    pub fn duplicate(existing_id: i64) -> Self {
        Self {
            existing_id: Some(existing_id),
            ..Self::new(Code::Duplicate, "That's already in your library.")
        }
    }
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        use rusqlite::{Error, ErrorCode};
        match &e {
            Error::QueryReturnedNoRows => Self::not_found("Not found."),
            Error::SqliteFailure(f, _) if f.code == ErrorCode::ConstraintViolation => {
                Self::validation("That value isn't allowed.").with_detail(e.to_string())
            }
            Error::SqliteFailure(f, _)
                if matches!(f.code, ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) =>
            {
                Self::new(Code::Integrity, "The library file is damaged.")
                    .with_detail(e.to_string())
            }
            _ => Self::internal("Database error.").with_detail(e.to_string()),
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::new(Code::Io, "Couldn't read or write a file.").with_detail(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::internal("Couldn't read JSON.").with_detail(e.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_connect() || e.is_timeout() {
            Self::offline()
        } else if e.is_redirect() {
            // net.rs's redirect policy refuses hops off the allow-list and > 5 hops.
            Self::validation("The link redirected somewhere Runout doesn't go.")
        } else {
            // without_url: never echo URLs (they can carry tokens or codes) into errors.
            Self::internal("Network error.").with_detail(e.without_url().to_string())
        }
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        Self::internal("App error.").with_detail(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_per_api_md() {
        let v = serde_json::to_value(AppError::duplicate(7)).unwrap();
        assert_eq!(
            v,
            json!({"code": "duplicate", "message": "That's already in your library.", "existing_id": 7})
        );
        let v = serde_json::to_value(AppError::new(Code::SpotifyBusy, "busy")).unwrap();
        assert_eq!(v, json!({"code": "spotify_busy", "message": "busy"}));
    }

    #[test]
    fn maps_sqlite_errors() {
        let c = rusqlite::Connection::open_in_memory().unwrap();
        let e: AppError = c
            .query_row("SELECT 1 WHERE 0", [], |r| r.get::<_, i64>(0))
            .unwrap_err()
            .into();
        assert_eq!(e.code, Code::NotFound);
        c.execute_batch("CREATE TABLE t(x CHECK (x > 0))").unwrap();
        let e: AppError = c
            .execute("INSERT INTO t VALUES (0)", [])
            .unwrap_err()
            .into();
        assert_eq!(e.code, Code::Validation);
    }
}
