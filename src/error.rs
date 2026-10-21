//! Error types for the `cdd-publisher` application.
//!
//! Uses a single comprehensive error enum `PublisherError` powered by `derive_more`
//! (`Display`, `Error`, `From`) across the entire crate.

use crate::domain::Registry;
use derive_more::{Display, Error, From};

/// The unified error type for `cdd-publisher`.
#[derive(Debug, Display, Error, From)]
pub enum PublisherError {
    /// A Redis broker operation failed.
    #[display("Redis error: {_0}")]
    Redis(redis::RedisError),

    /// A Redis value parsing operation failed.
    #[display("Redis parsing error: {_0}")]
    RedisParsing(redis::ParsingError),

    /// A JSON serialization or deserialization operation failed.
    #[display("JSON error: {_0}")]
    Json(serde_json::Error),

    /// An HTTP client request or status check failed.
    #[display("HTTP error: {_0}")]
    Http(reqwest::Error),

    /// A filesystem or subprocess I/O operation failed.
    #[display("I/O error: {_0}")]
    Io(std::io::Error),

    /// A ZIP archive extraction operation failed.
    #[display("Zip error: {_0}")]
    Zip(zip::result::ZipError),

    /// A tokio background blocking task failed to join.
    #[display("Task join error: {_0}")]
    Join(tokio::task::JoinError),

    /// An artifact URL had an unsupported archive extension.
    #[display("Unsupported artifact format: {url}")]
    #[error(ignore)]
    #[from(ignore)]
    UnsupportedArchiveFormat {
        /// The artifact URL or path that was rejected.
        url: String,
    },

    /// An archive entry attempted path traversal outside the extraction root (Zip-Slip).
    #[display("Unsafe archive entry path: {entry_path}")]
    #[error(ignore)]
    #[from(ignore)]
    UnsafeArchivePath {
        /// The offending relative path inside the archive.
        entry_path: String,
    },

    /// A job requested a registry that is not supported by `cdd-publisher`.
    #[display("Unsupported registry: {registry}")]
    #[error(ignore)]
    #[from(ignore)]
    UnsupportedRegistry {
        /// The unsupported registry name.
        registry: String,
    },

    /// The required authentication token for the target registry was not configured.
    #[display("{} is missing", registry.token_env_var())]
    #[error(ignore)]
    #[from(ignore)]
    MissingRegistryToken {
        /// The registry whose authentication token is missing.
        registry: Registry,
    },

    /// A registry CLI subprocess exited with a non-zero status or failed validation.
    #[display("Publish error: {_0}")]
    #[error(ignore)]
    #[from(ignore)]
    Publish(String),
}

/// The standard `Result` alias for `cdd-publisher`.
pub type Result<T> = std::result::Result<T, PublisherError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_error_from_redis() {
        let io_err = std::io::Error::other("IO Error");
        let redis_err = redis::RedisError::from(io_err);
        let err: PublisherError = redis_err.into();
        assert!(err.to_string().starts_with("Redis error:"));
        assert!(err.source().is_some());

        for res in [
            redis::FromRedisValue::from_redis_value(redis::Value::Nil),
            Ok::<i64, redis::ParsingError>(0),
        ] {
            if let Err(parse_err) = res {
                let p_err: PublisherError = parse_err.into();
                assert!(p_err.to_string().starts_with("Redis parsing error:"));
                assert!(p_err.source().is_some());
            }
        }
    }

    #[test]
    fn test_error_from_json() {
        for res in [
            serde_json::from_str::<serde_json::Value>("invalid json"),
            serde_json::from_str::<serde_json::Value>("null"),
        ] {
            if let Err(json_err) = res {
                let err: PublisherError = json_err.into();
                assert!(err.to_string().starts_with("JSON error:"));
                assert!(err.source().is_some());
            }
        }
    }

    #[tokio::test]
    async fn test_error_from_http() {
        let mock_server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(wiremock::ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        for url in [mock_server.uri(), "http://::invalid_url".to_string()] {
            if let Err(http_err) = client.get(&url).send().await {
                let err: PublisherError = http_err.into();
                assert!(err.to_string().starts_with("HTTP error:"));
                assert!(err.source().is_some());
            }
        }
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::other("test io error");
        let err: PublisherError = io_err.into();
        assert!(err.to_string().starts_with("I/O error: test io error"));
        assert!(err.source().is_some());
    }

    #[test]
    fn test_error_from_zip() {
        let zip_err = zip::result::ZipError::FileNotFound;
        let err: PublisherError = zip_err.into();
        assert!(err.to_string().starts_with("Zip error:"));
        assert!(err.source().is_some());
    }

    #[tokio::test]
    async fn test_error_from_join() {
        let ok_handle = tokio::task::spawn(std::future::ready(()));
        let abort_handle = tokio::task::spawn(std::future::pending::<()>());
        abort_handle.abort();
        for handle in [ok_handle, abort_handle] {
            if let Err(join_err) = handle.await {
                let err: PublisherError = join_err.into();
                assert!(err.to_string().starts_with("Task join error:"));
                assert!(err.source().is_some());
            }
        }
    }

    #[test]
    fn test_domain_error_variants() {
        let unsupported_fmt = PublisherError::UnsupportedArchiveFormat {
            url: "http://localhost/file.txt".to_string(),
        };
        assert_eq!(
            unsupported_fmt.to_string(),
            "Unsupported artifact format: http://localhost/file.txt"
        );

        let unsafe_path = PublisherError::UnsafeArchivePath {
            entry_path: "../etc/passwd".to_string(),
        };
        assert_eq!(
            unsafe_path.to_string(),
            "Unsafe archive entry path: ../etc/passwd"
        );

        let unsupported_reg = PublisherError::UnsupportedRegistry {
            registry: "rubygems".to_string(),
        };
        assert_eq!(
            unsupported_reg.to_string(),
            "Unsupported registry: rubygems"
        );

        let missing_npm = PublisherError::MissingRegistryToken {
            registry: Registry::Npm,
        };
        assert_eq!(missing_npm.to_string(), "NPM_TOKEN is missing");

        let missing_pypi = PublisherError::MissingRegistryToken {
            registry: Registry::PyPi,
        };
        assert_eq!(missing_pypi.to_string(), "TWINE_PASSWORD is missing");

        let missing_cargo = PublisherError::MissingRegistryToken {
            registry: Registry::Cargo,
        };
        assert_eq!(missing_cargo.to_string(), "CARGO_REGISTRY_TOKEN is missing");

        let err = PublisherError::Publish("test failure".to_string());
        assert_eq!(err.to_string(), "Publish error: test failure");
        assert!(err.source().is_none());
    }
}
