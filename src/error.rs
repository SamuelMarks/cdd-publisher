//! Error types for the `cdd-publisher` application.

use derive_more::{Display, Error, From};

/// The main error type for `cdd-publisher`.
#[derive(Debug, Display, Error, From)]
pub enum PublisherError {
    /// An unknown error occurred.
    #[display("An unknown error occurred")]
    Unknown,

    /// A Redis error occurred.
    #[display("Redis error: {}", _0)]
    Redis(redis::RedisError),

    /// A JSON serialization/deserialization error occurred.
    #[display("JSON error: {}", _0)]
    Json(serde_json::Error),

    /// An HTTP request error occurred.
    #[display("HTTP error: {}", _0)]
    Http(reqwest::Error),

    /// An I/O error occurred.
    #[display("I/O error: {}", _0)]
    Io(std::io::Error),

    /// A Zip extraction error occurred.
    #[display("Zip error: {}", _0)]
    Zip(zip::result::ZipError),

    /// A publishing error occurred.
    #[display("Publish error: {}", _0)]
    #[error(ignore)]
    Publish(String),
}

/// The standard Result type for `cdd-publisher`.
pub type Result<T> = std::result::Result<T, PublisherError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = PublisherError::Unknown;
        assert_eq!(err.to_string(), "An unknown error occurred");
    }

    #[test]
    fn test_error_from_redis() {
        let io_err = std::io::Error::other("IO Error");
        let redis_err = redis::RedisError::from(io_err);
        let err: PublisherError = redis_err.into();
        assert!(err.to_string().starts_with("Redis error:"));
    }

    #[test]
    fn test_error_from_json() {
        let json_result = serde_json::from_str::<serde_json::Value>("invalid json");
        let json_err = json_result.expect_err("Expected error");
        let err: PublisherError = json_err.into();
        assert!(err.to_string().starts_with("JSON error:"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::other("test io error");
        let err: PublisherError = io_err.into();
        assert!(err.to_string().starts_with("I/O error: test io error"));
    }

    #[test]
    fn test_error_from_zip() {
        let zip_err = zip::result::ZipError::FileNotFound;
        let err: PublisherError = zip_err.into();
        assert!(err.to_string().starts_with("Zip error:"));
    }

    #[test]
    fn test_error_publish() {
        let err = PublisherError::Publish("test failure".to_string());
        assert_eq!(err.to_string(), "Publish error: test failure");
    }
}
