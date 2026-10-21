//! Strong domain primitives and value objects for `cdd-publisher`.
//!
//! This module eliminates stringly-typed primitive obsession by encapsulating
//! registries, secret tokens, archive formats, artifact URLs, and stream configuration
//! in validated, strongly-typed domain structures.

use crate::error::{PublisherError, Result};
use derive_more::{AsRef, Display, From, Into};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Supported external package registries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "lowercase")]
pub enum Registry {
    /// The Node Package Manager (`npm`) registry.
    #[display("npm")]
    Npm,
    /// The Python Package Index (`pypi`) registry.
    #[display("pypi")]
    PyPi,
    /// The Rust Crates (`cargo`) registry.
    #[display("cargo")]
    Cargo,
}

impl Registry {
    /// Returns the canonical lowercase string slice representation of the registry.
    ///
    /// # Examples
    ///
    /// - `Registry::Npm` -> `"npm"`
    /// - `Registry::PyPi` -> `"pypi"`
    /// - `Registry::Cargo` -> `"cargo"`
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::PyPi => "pypi",
            Self::Cargo => "cargo",
        }
    }

    /// Returns the environment variable name used for this registry's authentication token.
    #[must_use]
    pub const fn token_env_var(self) -> &'static str {
        match self {
            Self::Npm => "NPM_TOKEN",
            Self::PyPi => "TWINE_PASSWORD",
            Self::Cargo => "CARGO_REGISTRY_TOKEN",
        }
    }
}

impl FromStr for Registry {
    type Err = PublisherError;

    /// Parses a string slice into a [`Registry`].
    ///
    /// # Arguments
    ///
    /// * `s` - The string identifier of the registry (case-insensitive: `"npm"`, `"pypi"`, `"cargo"`).
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::UnsupportedRegistry`] if `s` does not match a known registry.
    fn from_str(s: &str) -> Result<Self> {
        if s.eq_ignore_ascii_case("npm") {
            Ok(Self::Npm)
        } else if s.eq_ignore_ascii_case("pypi") {
            Ok(Self::PyPi)
        } else if s.eq_ignore_ascii_case("cargo") {
            Ok(Self::Cargo)
        } else {
            Err(PublisherError::UnsupportedRegistry {
                registry: s.to_string(),
            })
        }
    }
}

/// A secret authentication token that redacts itself in `Debug` and `Display` output.
///
/// Wrapping registry tokens in `SecretToken` guarantees that secrets are never
/// accidentally written to standard output or error logs via formatting traits.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretToken(String);

impl SecretToken {
    /// Replacement placeholder used when redacting secrets from logs or output streams.
    pub const REDACTED_PLACEHOLDER: &'static str = "***";

    /// Creates a new [`SecretToken`] from any string-like value.
    ///
    /// # Arguments
    ///
    /// * `value` - The raw secret token string.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Explicitly exposes the underlying secret string slice for authentication operations.
    ///
    /// # Security
    ///
    /// Callers must never log the returned reference directly.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    /// Scrubs all occurrences of the secret token from the provided text,
    /// replacing them with [`SecretToken::REDACTED_PLACEHOLDER`].
    ///
    /// If the secret token is empty, returns the input text unchanged.
    ///
    /// # Arguments
    ///
    /// * `input` - The diagnostic or stderr text that may contain the raw secret.
    #[must_use]
    pub fn redact_from(&self, input: &str) -> String {
        if self.0.is_empty() {
            return input.to_string();
        }
        input.replace(&self.0, Self::REDACTED_PLACEHOLDER)
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretToken([REDACTED])")
    }
}

impl fmt::Display for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl From<String> for SecretToken {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SecretToken {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl Drop for SecretToken {
    fn drop(&mut self) {
        // Overwrite the secret bytes in memory prior to deallocation.
        // SAFETY: Replacing valid UTF-8 bytes with ASCII 0x00 ('\0') preserves valid UTF-8.
        unsafe {
            let bytes = self.0.as_bytes_mut();
            std::ptr::write_bytes(bytes.as_mut_ptr(), 0, bytes.len());
        }
    }
}

/// Supported archive compression and packaging formats for artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
pub enum ArchiveFormat {
    /// A ZIP archive (`.zip`).
    #[display("zip")]
    Zip,
    /// A Gzip-compressed TAR archive (`.tar.gz` or `.tgz`).
    #[display("tar.gz")]
    TarGz,
}

impl ArchiveFormat {
    /// Determines the [`ArchiveFormat`] from a URL or file path string.
    ///
    /// Query strings (`?...`) and fragments (`#...`) are stripped prior to
    /// performing a case-insensitive extension check.
    ///
    /// # Arguments
    ///
    /// * `url_or_path` - The URL or path string to inspect.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::UnsupportedArchiveFormat`] if the path does not end with
    /// `.zip`, `.tar.gz`, or `.tgz` (case-insensitively).
    pub fn from_url_or_path(url_or_path: &str) -> Result<Self> {
        let without_fragment = url_or_path.split('#').next().unwrap_or(url_or_path);
        let clean_path = without_fragment
            .split('?')
            .next()
            .unwrap_or(without_fragment);
        let path = std::path::Path::new(clean_path);
        let ext = path.extension();

        let is_tgz = ext.is_some_and(|e| e.eq_ignore_ascii_case("tgz"));
        let is_tar_gz = ext.is_some_and(|e| e.eq_ignore_ascii_case("gz"))
            && path
                .file_stem()
                .and_then(|stem| std::path::Path::new(stem).extension())
                .is_some_and(|e| e.eq_ignore_ascii_case("tar"));
        let is_zip = ext.is_some_and(|e| e.eq_ignore_ascii_case("zip"));

        if is_tar_gz || is_tgz {
            Ok(Self::TarGz)
        } else if is_zip {
            Ok(Self::Zip)
        } else {
            Err(PublisherError::UnsupportedArchiveFormat {
                url: url_or_path.to_string(),
            })
        }
    }
}

/// Strongly-typed identifier/URL for an artifact bundle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct ArtifactUrl(String);

impl ArtifactUrl {
    /// Creates a new [`ArtifactUrl`] from a string-like value.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL or identifier of the artifact.
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self(url.into())
    }

    /// Returns the underlying string slice of the artifact URL.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Determines the [`ArchiveFormat`] of this artifact URL.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::UnsupportedArchiveFormat`] if the URL extension is unsupported.
    pub fn archive_format(&self) -> Result<ArchiveFormat> {
        ArchiveFormat::from_url_or_path(&self.0)
    }
}

impl From<&str> for ArtifactUrl {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed Redis stream name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct StreamName(String);

impl StreamName {
    /// Creates a new [`StreamName`].
    ///
    /// # Arguments
    ///
    /// * `name` - The stream identifier.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the stream name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for StreamName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed Redis consumer group name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct GroupName(String);

impl GroupName {
    /// Creates a new [`GroupName`].
    ///
    /// # Arguments
    ///
    /// * `name` - The consumer group identifier.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the group name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for GroupName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed Redis consumer name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct ConsumerName(String);

impl ConsumerName {
    /// Creates a new [`ConsumerName`].
    ///
    /// # Arguments
    ///
    /// * `name` - The consumer instance identifier.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the consumer name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ConsumerName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed queue message ID (e.g. `"1700000000000-0"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct MessageId(String);

impl MessageId {
    /// Creates a new [`MessageId`].
    ///
    /// # Arguments
    ///
    /// * `id` - The broker-assigned message identifier.
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the message ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MessageId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed URL for the `cdd-control-plane` service.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, Into)]
#[serde(transparent)]
pub struct ControlPlaneUrl(String);

impl ControlPlaneUrl {
    /// Creates a new [`ControlPlaneUrl`], normalizing any trailing slashes.
    ///
    /// # Arguments
    ///
    /// * `url` - The base URL of the control plane (e.g. `"http://127.0.0.1:8080"`).
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        let raw = url.into();
        let normalized = raw.trim_end_matches('/').to_string();
        Self(normalized)
    }

    /// Returns the normalized base URL as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Constructs the full audit publish endpoint URL (`<base>/api/v1/audit/publish`).
    #[must_use]
    pub fn audit_publish_endpoint(&self) -> String {
        format!("{}/api/v1/audit/publish", self.0)
    }
}

impl From<String> for ControlPlaneUrl {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for ControlPlaneUrl {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Configuration for the event stream consumer group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamConfig {
    /// The name of the stream to read jobs from.
    pub stream_name: StreamName,
    /// The name of the consumer group.
    pub group_name: GroupName,
    /// The unique name of the worker consumer within the group.
    pub consumer_name: ConsumerName,
}

impl StreamConfig {
    /// Creates a new [`StreamConfig`].
    ///
    /// # Arguments
    ///
    /// * `stream_name` - The stream name.
    /// * `group_name` - The consumer group name.
    /// * `consumer_name` - The consumer name.
    #[must_use]
    pub fn new(
        stream_name: impl Into<StreamName>,
        group_name: impl Into<GroupName>,
        consumer_name: impl Into<ConsumerName>,
    ) -> Self {
        Self {
            stream_name: stream_name.into(),
            group_name: group_name.into(),
            consumer_name: consumer_name.into(),
        }
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self::new("publisher_jobs", "publisher_group", "worker_1")
    }
}

/// A raw unparsed message read from the event queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawQueueMessage {
    /// The broker message ID used for acknowledgement.
    pub id: MessageId,
    /// The raw byte payload of the message.
    pub payload: Vec<u8>,
}

impl RawQueueMessage {
    /// Creates a new [`RawQueueMessage`].
    ///
    /// # Arguments
    ///
    /// * `id` - The broker message identifier.
    /// * `payload` - The raw payload bytes.
    #[must_use]
    pub fn new(id: impl Into<MessageId>, payload: Vec<u8>) -> Self {
        Self {
            id: id.into(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_from_str_and_display() -> Result<()> {
        assert_eq!(Registry::from_str("npm")?, Registry::Npm);
        assert_eq!(Registry::from_str("NPM")?, Registry::Npm);
        assert_eq!(Registry::from_str("pypi")?, Registry::PyPi);
        assert_eq!(Registry::from_str("PyPI")?, Registry::PyPi);
        assert_eq!(Registry::from_str("cargo")?, Registry::Cargo);
        assert_eq!(Registry::from_str("CARGO")?, Registry::Cargo);

        assert_eq!(Registry::Npm.as_str(), "npm");
        assert_eq!(Registry::PyPi.as_str(), "pypi");
        assert_eq!(Registry::Cargo.as_str(), "cargo");

        assert_eq!(Registry::Npm.to_string(), "npm");
        assert_eq!(Registry::PyPi.to_string(), "pypi");
        assert_eq!(Registry::Cargo.to_string(), "cargo");

        assert_eq!(Registry::Npm.token_env_var(), "NPM_TOKEN");
        assert_eq!(Registry::PyPi.token_env_var(), "TWINE_PASSWORD");
        assert_eq!(Registry::Cargo.token_env_var(), "CARGO_REGISTRY_TOKEN");

        let invalid = Registry::from_str("maven");
        assert_eq!(
            invalid.as_ref().map_err(ToString::to_string),
            Err("Unsupported registry: maven".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_registry_serde() -> Result<()> {
        let json = serde_json::to_string(&Registry::PyPi)?;
        assert_eq!(json, r#""pypi""#);
        let parsed: Registry = serde_json::from_str(r#""cargo""#)?;
        assert_eq!(parsed, Registry::Cargo);
        Ok(())
    }

    #[test]
    fn test_secret_token_redaction_and_formatting() {
        let token = SecretToken::from("super-secret-xyz");
        assert_eq!(token.expose_secret(), "super-secret-xyz");
        assert_eq!(format!("{token}"), "[REDACTED]");
        assert_eq!(format!("{token:?}"), "SecretToken([REDACTED])");

        let token_owned = SecretToken::from("super-secret-xyz".to_string());
        assert_eq!(token, token_owned);

        let log = "Error authenticating with super-secret-xyz against registry";
        assert_eq!(
            token.redact_from(log),
            "Error authenticating with *** against registry"
        );

        let empty_token = SecretToken::new("");
        assert_eq!(empty_token.redact_from("abc"), "abc");
    }

    #[test]
    fn test_archive_format_detection() -> Result<()> {
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.zip")?,
            ArchiveFormat::Zip
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.ZIP?token=123#frag")?,
            ArchiveFormat::Zip
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.tar.gz")?,
            ArchiveFormat::TarGz
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.TAR.GZ?sig=abc")?,
            ArchiveFormat::TarGz
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.tgz")?,
            ArchiveFormat::TarGz
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.TGZ")?,
            ArchiveFormat::TarGz
        );
        assert_eq!(ArchiveFormat::Zip.to_string(), "zip");
        assert_eq!(ArchiveFormat::TarGz.to_string(), "tar.gz");

        let unsupported = ArchiveFormat::from_url_or_path("https://example.com/pkg.rar");
        assert_eq!(
            unsupported.as_ref().map_err(ToString::to_string),
            Err("Unsupported artifact format: https://example.com/pkg.rar".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_artifact_url_and_identifiers() -> Result<()> {
        let url = ArtifactUrl::from("https://example.com/sdk.tar.gz");
        assert_eq!(url.as_str(), "https://example.com/sdk.tar.gz");
        assert_eq!(url.as_ref(), "https://example.com/sdk.tar.gz");
        assert_eq!(url.archive_format()?, ArchiveFormat::TarGz);
        let raw_url: String = url.into();
        assert_eq!(raw_url, "https://example.com/sdk.tar.gz");

        let stream = StreamName::from("jobs");
        assert_eq!(stream.as_str(), "jobs");
        assert_eq!(stream.as_ref(), "jobs");
        assert_eq!(String::from(stream), "jobs");

        let group = GroupName::from("grp");
        assert_eq!(group.as_str(), "grp");
        assert_eq!(group.as_ref(), "grp");
        assert_eq!(String::from(group), "grp");

        let consumer = ConsumerName::from("c1");
        assert_eq!(consumer.as_str(), "c1");
        assert_eq!(consumer.as_ref(), "c1");
        assert_eq!(String::from(consumer), "c1");

        let msg_id = MessageId::from("1-0");
        assert_eq!(msg_id.as_str(), "1-0");
        assert_eq!(msg_id.as_ref(), "1-0");
        assert_eq!(String::from(msg_id.clone()), "1-0");

        let raw_msg = RawQueueMessage::new(msg_id, vec![1, 2, 3]);
        assert_eq!(raw_msg.id.as_str(), "1-0");
        assert_eq!(raw_msg.payload, vec![1, 2, 3]);

        let cfg = StreamConfig::default();
        assert_eq!(cfg.stream_name.as_str(), "publisher_jobs");
        assert_eq!(cfg.group_name.as_str(), "publisher_group");
        assert_eq!(cfg.consumer_name.as_str(), "worker_1");

        let cp = ControlPlaneUrl::from("http://127.0.0.1:8080///");
        assert_eq!(cp.as_str(), "http://127.0.0.1:8080");
        assert_eq!(cp.as_ref(), "http://127.0.0.1:8080");
        assert_eq!(
            cp.audit_publish_endpoint(),
            "http://127.0.0.1:8080/api/v1/audit/publish"
        );
        assert_eq!(String::from(cp), "http://127.0.0.1:8080");
        Ok(())
    }
}
