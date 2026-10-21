//! Module for sending audit telemetry back to the control plane.
//!
//! Defines [`AuditPayload`], the [`AuditReporter`] trait for dependency injection,
//! and [`AuditClient`] for HTTP status reporting with URL normalization.

use crate::domain::{ArtifactUrl, ControlPlaneUrl, Registry};
use crate::error::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::future::Future;

/// Payload for reporting publishing status to `cdd-control-plane`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditPayload {
    /// The URL/ID of the artifact that was published.
    pub artifact_id: ArtifactUrl,
    /// The registry that was targeted (`npm`, `pypi`, `cargo`).
    pub registry: Registry,
    /// Whether the publishing process was successful.
    pub success: bool,
    /// Error message, populated if `success` is `false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl AuditPayload {
    /// Creates a new successful [`AuditPayload`].
    ///
    /// # Arguments
    ///
    /// * `artifact_id` - The artifact URL/ID that succeeded.
    /// * `registry` - The target registry.
    #[must_use]
    pub const fn success(artifact_id: ArtifactUrl, registry: Registry) -> Self {
        Self {
            artifact_id,
            registry,
            success: true,
            error_message: None,
        }
    }

    /// Creates a new failed [`AuditPayload`].
    ///
    /// # Arguments
    ///
    /// * `artifact_id` - The artifact URL/ID that failed.
    /// * `registry` - The target registry.
    /// * `error_message` - The sanitized error diagnostic string.
    #[must_use]
    pub const fn failure(
        artifact_id: ArtifactUrl,
        registry: Registry,
        error_message: String,
    ) -> Self {
        Self {
            artifact_id,
            registry,
            success: false,
            error_message: Some(error_message),
        }
    }
}

/// Abstract interface for reporting job completion telemetry to the control plane.
pub trait AuditReporter: Send + Sync {
    /// Reports the status of a publish job to the control plane.
    ///
    /// # Arguments
    ///
    /// * `payload` - The [`AuditPayload`] describing the job result.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::PublisherError`] if the telemetry report cannot be delivered.
    fn report_status(&self, payload: &AuditPayload) -> impl Future<Output = Result<()>> + Send;
}

/// An HTTP API client that POSTs the success/failure state back to `cdd-control-plane`.
#[derive(Debug, Clone)]
pub struct AuditClient {
    /// The HTTP client used to send audit events.
    client: Client,
    /// The normalized base URL of the control plane to which events are sent.
    control_plane_url: ControlPlaneUrl,
}

impl AuditClient {
    /// Creates a new [`AuditClient`].
    ///
    /// Any trailing slashes on `control_plane_url` are normalized automatically.
    ///
    /// # Arguments
    ///
    /// * `client` - The [`reqwest::Client`] instance.
    /// * `control_plane_url` - The base URL of `cdd-control-plane`.
    #[must_use]
    pub fn new(client: Client, control_plane_url: impl Into<ControlPlaneUrl>) -> Self {
        Self {
            client,
            control_plane_url: control_plane_url.into(),
        }
    }

    /// Returns a reference to the configured [`ControlPlaneUrl`].
    #[must_use]
    pub const fn control_plane_url(&self) -> &ControlPlaneUrl {
        &self.control_plane_url
    }

    /// Reports the status of a publish job to the control plane.
    ///
    /// # Arguments
    ///
    /// * `payload` - The [`AuditPayload`] containing the job outcome.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP POST request fails or returns a non-success status code.
    pub async fn report_status(&self, payload: &AuditPayload) -> Result<()> {
        let endpoint = self.control_plane_url.audit_publish_endpoint();
        self.client
            .post(&endpoint)
            .json(payload)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }
}

impl AuditReporter for AuditClient {
    async fn report_status(&self, payload: &AuditPayload) -> Result<()> {
        Self::report_status(self, payload).await
    }
}

#[cfg(test)]
/// Tests for the `audit` module.
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, method, path},
    };

    /// Helper that invokes [`AuditReporter::report_status`] generically.
    async fn invoke_reporter<R: AuditReporter>(reporter: &R, payload: &AuditPayload) -> Result<()> {
        reporter.report_status(payload).await
    }

    /// Tests reporting a successful publishing job via both inherent method and trait.
    #[tokio::test]
    async fn test_report_status_success() -> Result<()> {
        let mock_server = MockServer::start().await;

        let payload = AuditPayload::success(ArtifactUrl::new("art-123"), Registry::Npm);

        Mock::given(method("POST"))
            .and(path("/api/v1/audit/publish"))
            .and(body_json(&payload))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        // Verify trailing slash normalization works cleanly
        let uri_with_slash = format!("{}/", mock_server.uri());
        let audit_client = AuditClient::new(client, uri_with_slash);
        assert_eq!(audit_client.control_plane_url().as_str(), mock_server.uri());

        invoke_reporter(&audit_client, &payload).await?;
        Ok(())
    }

    /// Tests reporting a failed publishing job.
    #[tokio::test]
    async fn test_report_status_failure() -> Result<()> {
        let mock_server = MockServer::start().await;

        let payload = AuditPayload::failure(
            ArtifactUrl::new("art-123"),
            Registry::Cargo,
            "publish failed".to_string(),
        );

        Mock::given(method("POST"))
            .and(path("/api/v1/audit/publish"))
            .and(body_json(&payload))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let audit_client = AuditClient::new(client, mock_server.uri());

        audit_client.report_status(&payload).await?;
        Ok(())
    }

    /// Tests handling HTTP errors during audit report.
    #[tokio::test]
    async fn test_report_status_http_error() {
        let mock_server = MockServer::start().await;

        let payload = AuditPayload::success(ArtifactUrl::new("art-123"), Registry::PyPi);

        Mock::given(method("POST"))
            .and(path("/api/v1/audit/publish"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let audit_client = AuditClient::new(client, mock_server.uri());

        let result = audit_client.report_status(&payload).await;
        assert!(result.is_err());
    }
}
