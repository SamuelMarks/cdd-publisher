//! Module for sending audit telemetry back to the control plane.

use crate::error::Result;
use reqwest::Client;
use serde::Serialize;

/// Payload for reporting publishing status.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AuditPayload {
    /// The ID of the artifact that was published.
    pub artifact_id: String,
    /// The registry that was targeted (e.g., "npm", "cargo").
    pub registry: String,
    /// Whether the publishing process was successful.
    pub success: bool,
    /// Error message, populated if `success` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// An API client that POSTs the success/failure state back to `cdd-control-plane`.
#[derive(Debug, Clone)]
pub struct AuditClient {
    /// The HTTP client used to send audit events.
    client: Client,
    /// The URL of the control plane to which events are sent.
    control_plane_url: String,
}

impl AuditClient {
    /// Creates a new `AuditClient`.
    #[must_use]
    pub const fn new(client: Client, control_plane_url: String) -> Self {
        Self {
            client,
            control_plane_url,
        }
    }

    /// Reports the status of a publish job to the control plane.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP POST request fails or returns a non-success status code.
    pub async fn report_status(&self, payload: &AuditPayload) -> Result<()> {
        let endpoint = format!("{}/api/v1/audit/publish", self.control_plane_url);
        self.client
            .clone()
            .post(&endpoint)
            .json(payload)
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, method, path},
    };

    #[tokio::test]
    async fn test_report_status_success() {
        let mock_server = MockServer::start().await;

        let payload = AuditPayload {
            artifact_id: "art-123".to_string(),
            registry: "npm".to_string(),
            success: true,
            error_message: None,
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/audit/publish"))
            .and(body_json(&payload))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let audit_client = AuditClient::new(client, mock_server.uri());

        let result = audit_client.report_status(&payload).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_report_status_failure() {
        let mock_server = MockServer::start().await;

        let payload = AuditPayload {
            artifact_id: "art-123".to_string(),
            registry: "npm".to_string(),
            success: false,
            error_message: Some("publish failed".to_string()),
        };

        Mock::given(method("POST"))
            .and(path("/api/v1/audit/publish"))
            .and(body_json(&payload))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let audit_client = AuditClient::new(client, mock_server.uri());

        let result = audit_client.report_status(&payload).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_report_status_http_error() {
        let mock_server = MockServer::start().await;

        let payload = AuditPayload {
            artifact_id: "art-123".to_string(),
            registry: "npm".to_string(),
            success: true,
            error_message: None,
        };

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
