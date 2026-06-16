//! Worker loop for `cdd-publisher`.

use crate::audit::{AuditClient, AuditPayload};
use crate::error::{PublisherError, Result};
use crate::storage::fetch_and_unpack;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Paths to the executable CLIs for registries.
#[derive(Debug, Clone)]
pub struct ExecutablePaths {
    /// NPM executable path
    pub npm: String,
    /// `PyPI` (twine) executable path
    pub pypi: String,
    /// Cargo executable path
    pub cargo: String,
}

impl Default for ExecutablePaths {
    fn default() -> Self {
        Self {
            npm: "npm".to_string(),
            pypi: "twine".to_string(),
            cargo: "cargo".to_string(),
        }
    }
}

/// Tokens for different registries.
#[derive(Debug, Clone, Default)]
pub struct RegistryTokens {
    /// Token for NPM registry.
    pub npm: Option<String>,
    /// Token for `PyPI` registry.
    pub pypi: Option<String>,
    /// Token for Cargo registry.
    pub cargo: Option<String>,
}

/// A job to be processed by the worker.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishJob {
    /// The URL of the artifact to fetch and publish.
    pub artifact_id: String,
    /// The registry type (e.g., "npm", "pypi", "cargo").
    pub registry: String,
}

/// Abstract interface for the event queue, allowing for mocking in tests.
#[async_trait]
pub trait EventQueue: Send + Sync {
    /// Creates a consumer group and the stream if it doesn't exist.
    async fn create_group(&mut self, stream: &str, group: &str) -> Result<()>;
    /// Reads a single message from the stream. Returns an optional tuple of `(id, payload_bytes)`.
    async fn read_one(
        &mut self,
        stream: &str,
        group: &str,
        consumer: &str,
    ) -> Result<Option<(String, Vec<u8>)>>;
    /// Acknowledges a message.
    async fn ack(&mut self, stream: &str, group: &str, id: &str) -> Result<()>;
}

/// Implementation of `EventQueue` for Redis `MultiplexedConnection`.
#[async_trait]
#[cfg(not(tarpaulin_include))]
impl EventQueue for redis::aio::MultiplexedConnection {
    async fn create_group(&mut self, stream: &str, group: &str) -> Result<()> {
        let result: redis::RedisResult<()> =
            redis::AsyncCommands::xgroup_create_mkstream(self, stream, group, "$").await;
        if let Err(e) = result {
            let err_str = e.to_string();
            if !err_str.contains("BUSYGROUP") {
                return Err(PublisherError::Redis(e));
            }
        }
        Ok(())
    }

    async fn read_one(
        &mut self,
        stream: &str,
        group: &str,
        consumer: &str,
    ) -> Result<Option<(String, Vec<u8>)>> {
        let opts = redis::streams::StreamReadOptions::default()
            .group(group, consumer)
            .block(5000)
            .count(1);

        let reply: redis::streams::StreamReadReply =
            redis::AsyncCommands::xread_options(self, &[stream], &[">"], &opts).await?;

        if reply.keys.is_empty() {
            return Ok(None);
        }

        let key = &reply.keys[0];
        if key.ids.is_empty() {
            return Ok(None);
        }

        let id = &key.ids[0];
        let payload_value = id.map.get("payload");
        let Some(payload_value) = payload_value else {
            return Ok(Some((id.id.clone(), Vec::new()))); // Empty/invalid payload
        };

        let payload_bytes: Vec<u8> =
            redis::FromRedisValue::from_redis_value(payload_value.clone()).unwrap_or_default();

        Ok(Some((id.id.clone(), payload_bytes)))
    }

    async fn ack(&mut self, stream: &str, group: &str, id: &str) -> Result<()> {
        let _: () = redis::AsyncCommands::xack(self, stream, group, &[id]).await?;
        Ok(())
    }
}

/// The worker that processes publish jobs.
pub struct Worker<Q: EventQueue> {
    /// The event queue connection (e.g. Redis).
    connection: Q,
    /// The name of the stream to read from.
    stream_name: String,
    /// The name of the consumer group.
    group_name: String,
    /// The name of the consumer within the group.
    consumer_name: String,
    /// The HTTP client used to fetch artifacts.
    client: reqwest::Client,
    /// Registry authentication tokens.
    tokens: RegistryTokens,
    /// The audit client to report statuses.
    audit_client: AuditClient,
    /// Executable paths for registry CLIs.
    pub exe_paths: ExecutablePaths,
}

impl<Q: EventQueue> Worker<Q> {
    /// Creates a new worker with the given event queue connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the consumer group cannot be created.
    pub async fn new(
        mut connection: Q,
        stream_name: &str,
        group_name: &str,
        consumer_name: &str,
        tokens: RegistryTokens,
        audit_client: AuditClient,
    ) -> Result<Self> {
        connection.create_group(stream_name, group_name).await?;

        Ok(Self {
            connection,
            stream_name: stream_name.to_string(),
            group_name: group_name.to_string(),
            consumer_name: consumer_name.to_string(),
            client: reqwest::Client::new(),
            tokens,
            audit_client,
            exe_paths: ExecutablePaths::default(),
        })
    }

    /// Runs the worker loop once (for one read operation).
    ///
    /// # Errors
    ///
    /// Returns an error if reading from the stream fails or if acknowledging
    /// messages fails.
    pub async fn run_once(&mut self) -> Result<()> {
        let msg = self
            .connection
            .read_one(&self.stream_name, &self.group_name, &self.consumer_name)
            .await?;

        let Some((id, payload_bytes)) = msg else {
            return Ok(()); // Timeout, no messages
        };

        if payload_bytes.is_empty() {
            // Dead letter / invalid message logic could go here
            self.acknowledge(&id).await?;
            return Ok(());
        }

        match serde_json::from_slice::<PublishJob>(&payload_bytes) {
            Ok(job) => {
                let process_result = self.process_job(&job).await;

                self.report_audit(&job, &process_result).await;

                if let Err(e) = process_result {
                    eprintln!("Failed to process job: {e}");
                } else {
                    self.acknowledge(&id).await?;
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("Failed to parse job payload: {e}");
                self.acknowledge(&id).await?;
                Ok(())
            }
        }
    }

    /// Reports the audit status.
    async fn report_audit(&self, job: &PublishJob, process_result: &Result<()>) {
        let payload = match process_result {
            Ok(()) => AuditPayload {
                artifact_id: job.artifact_id.clone(),
                registry: job.registry.clone(),
                success: true,
                error_message: None,
            },
            Err(e) => AuditPayload {
                artifact_id: job.artifact_id.clone(),
                registry: job.registry.clone(),
                success: false,
                error_message: Some(e.to_string()),
            },
        };
        if let Err(e) = self.audit_client.report_status(&payload).await {
            eprintln!("Failed to report audit status: {e}");
        }
    }

    /// Processes a single publish job.
    ///
    /// # Errors
    ///
    /// Returns an error if the job cannot be processed correctly.
    async fn process_job(&mut self, job: &PublishJob) -> Result<()> {
        println!("Processing job: {job:?}");

        let dest_dir = tempfile::TempDir::new().expect("Failed to create TempDir");

        // 1. Fetch and unpack the artifact
        fetch_and_unpack(&self.client, &job.artifact_id, dest_dir.path()).await?;

        // 2. Publish to the appropriate registry
        match job.registry.as_str() {
            "npm" => {
                let token =
                    self.tokens.npm.as_deref().ok_or_else(|| {
                        PublisherError::Publish("NPM_TOKEN is missing".to_string())
                    })?;
                crate::publish::publish_npm_with_exe(dest_dir.path(), token, &self.exe_paths.npm)?;
                Ok(())
            }
            "pypi" => {
                let token = self.tokens.pypi.as_deref().ok_or_else(|| {
                    PublisherError::Publish("TWINE_PASSWORD is missing".to_string())
                })?;
                crate::publish::publish_pypi_with_exe(
                    dest_dir.path(),
                    token,
                    &self.exe_paths.pypi,
                )?;
                Ok(())
            }
            "cargo" => {
                let token = self.tokens.cargo.as_deref().ok_or_else(|| {
                    PublisherError::Publish("CARGO_REGISTRY_TOKEN is missing".to_string())
                })?;
                crate::publish::publish_cargo_with_exe(
                    dest_dir.path(),
                    token,
                    &self.exe_paths.cargo,
                )?;
                Ok(())
            }
            _ => Err(PublisherError::Publish(format!(
                "Unsupported registry: {}",
                job.registry
            ))),
        }
    }

    /// Acknowledges a successfully processed message.
    ///
    /// # Errors
    ///
    /// Returns an error if the XACK command fails.
    async fn acknowledge(&mut self, id: &str) -> Result<()> {
        self.connection
            .ack(&self.stream_name, &self.group_name, id)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_dummy_exe(dir: &std::path::Path, name: &str, exit_code: i32) -> String {
        let exe_path = dir.join(name);
        let script = format!("#!/bin/sh\nexit {}\n", exit_code);
        std::fs::write(&exe_path, script).expect("Failed to write mock exe");
        std::fs::set_permissions(
            &exe_path,
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
        )
        .expect("Failed to set perms");
        exe_path.to_string_lossy().to_string()
    }
    use crate::audit::AuditClient;
    use mockall::mock;

    fn get_dummy_audit_client() -> AuditClient {
        AuditClient::new(reqwest::Client::new(), "http://localhost".to_string())
    }

    mock! {
        pub RedisQueue {}
        #[async_trait]
        impl EventQueue for RedisQueue {
            async fn create_group(&mut self, stream: &str, group: &str) -> Result<()>;
            async fn read_one(&mut self, stream: &str, group: &str, consumer: &str) -> Result<Option<(String, Vec<u8>)>>;
            async fn ack(&mut self, stream: &str, group: &str, id: &str) -> Result<()>;
        }
    }

    #[test]
    fn test_publish_job_serialization() {
        let job = PublishJob {
            artifact_id: "test-artifact-123".to_string(),
            registry: "npm".to_string(),
        };

        if let Ok(json) = serde_json::to_string(&job) {
            assert_eq!(
                json,
                r#"{"artifact_id":"test-artifact-123","registry":"npm"}"#
            );

            if let Ok(deserialized) = serde_json::from_str::<PublishJob>(&json) {
                assert_eq!(job, deserialized);
            } else {
                panic!("Failed to deserialize");
            }
        } else {
            panic!("Failed to serialize");
        }
    }

    #[tokio::test]
    async fn test_worker_new_success() {
        let mut mock_queue = MockRedisQueue::new();
        mock_queue.expect_create_group().returning(|_, _| Ok(()));

        let worker = Worker::new(
            mock_queue,
            "stream",
            "group",
            "consumer",
            RegistryTokens::default(),
            get_dummy_audit_client(),
        )
        .await;
        assert!(worker.is_ok());
    }

    #[tokio::test]
    async fn test_worker_new_error() {
        let mut mock_queue = MockRedisQueue::new();
        mock_queue
            .expect_create_group()
            .returning(|_, _| Err(PublisherError::Unknown));

        let worker = Worker::new(
            mock_queue,
            "stream",
            "group",
            "consumer",
            RegistryTokens::default(),
            get_dummy_audit_client(),
        )
        .await;
        assert!(worker.is_err());
    }

    #[tokio::test]
    async fn test_worker_run_once_no_messages() {
        let mut mock_queue = MockRedisQueue::new();
        mock_queue.expect_create_group().returning(|_, _| Ok(()));
        mock_queue.expect_read_one().returning(|_, _, _| Ok(None));

        let mut worker = Worker::new(
            mock_queue,
            "stream",
            "group",
            "consumer",
            RegistryTokens::default(),
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");
        let result = worker.run_once().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_worker_run_once_empty_payload() {
        let mut mock_queue = MockRedisQueue::new();
        mock_queue.expect_create_group().returning(|_, _| Ok(()));
        mock_queue
            .expect_read_one()
            .returning(|_, _, _| Ok(Some(("1-0".to_string(), Vec::new()))));
        mock_queue.expect_ack().returning(|_, _, _| Ok(()));

        let mut worker = Worker::new(
            mock_queue,
            "stream",
            "group",
            "consumer",
            RegistryTokens::default(),
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");
        let result = worker.run_once().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_worker_run_once_invalid_json() {
        let mut mock_queue = MockRedisQueue::new();
        mock_queue.expect_create_group().returning(|_, _| Ok(()));
        mock_queue
            .expect_read_one()
            .returning(|_, _, _| Ok(Some(("1-0".to_string(), b"invalid".to_vec()))));
        mock_queue.expect_ack().returning(|_, _, _| Ok(()));

        let mut worker = Worker::new(
            mock_queue,
            "stream",
            "group",
            "consumer",
            RegistryTokens::default(),
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");
        let result = worker.run_once().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_worker_process_job() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let mock_server = MockServer::start().await;
        let mut zip_data = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_data));
            zip.start_file("hello.txt", zip::write::SimpleFileOptions::default())
                .expect("start_file failed");
            std::io::Write::write_all(&mut zip, b"hello world").expect("write_all failed");
            zip.finish().expect("finish failed");
        }
        Mock::given(method("GET"))
            .and(path("/artifact.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(zip_data))
            .mount(&mock_server)
            .await;

        let mut mock_queue = MockRedisQueue::new();
        mock_queue.expect_create_group().returning(|_, _| Ok(()));

        let tokens = RegistryTokens {
            npm: Some("token".into()),
            pypi: Some("token".into()),
            cargo: Some("token".into()),
        };
        let mut worker = Worker::new(
            mock_queue,
            "stream",
            "group",
            "consumer",
            tokens,
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");

        // Use an unsupported registry to quickly test error
        let job = PublishJob {
            artifact_id: format!("{}/artifact.zip", mock_server.uri()),
            registry: "unsupported".to_string(),
        };

        let result = worker.process_job(&job).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Unsupported registry"));
        }

        // Missing token for PyPI
        let mut mock_queue_no_pypi = MockRedisQueue::new();
        mock_queue_no_pypi
            .expect_create_group()
            .returning(|_, _| Ok(()));
        let mut worker_no_pypi = Worker::new(
            mock_queue_no_pypi,
            "stream",
            "group",
            "consumer",
            RegistryTokens::default(),
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");
        let job_pypi = PublishJob {
            artifact_id: format!("{}/artifact.zip", mock_server.uri()),
            registry: "pypi".to_string(),
        };
        assert!(worker_no_pypi.process_job(&job_pypi).await.is_err());

        // Missing token for Cargo
        let mut mock_queue_no_cargo = MockRedisQueue::new();
        mock_queue_no_cargo
            .expect_create_group()
            .returning(|_, _| Ok(()));
        let mut worker_no_cargo = Worker::new(
            mock_queue_no_cargo,
            "stream",
            "group",
            "consumer",
            RegistryTokens::default(),
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");
        let job_cargo = PublishJob {
            artifact_id: format!("{}/artifact.zip", mock_server.uri()),
            registry: "cargo".to_string(),
        };
        assert!(worker_no_cargo.process_job(&job_cargo).await.is_err());
        // Missing token for NPM
        let mut mock_queue_no_npm = MockRedisQueue::new();
        mock_queue_no_npm
            .expect_create_group()
            .returning(|_, _| Ok(()));
        let mut worker_no_npm = Worker::new(
            mock_queue_no_npm,
            "stream",
            "group",
            "consumer",
            RegistryTokens::default(),
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");
        let job_npm = PublishJob {
            artifact_id: format!("{}/artifact.zip", mock_server.uri()),
            registry: "npm".to_string(),
        };
        assert!(worker_no_npm.process_job(&job_npm).await.is_err());

        // Successful PyPI
        let dest_pypi = tempfile::TempDir::new().expect("Failed to create TempDir");
        let pypi_exe = create_dummy_exe(dest_pypi.path(), "twine", 0);
        let mut mock_queue_pypi = MockRedisQueue::new();
        mock_queue_pypi
            .expect_create_group()
            .returning(|_, _| Ok(()));
        let tokens_pypi = RegistryTokens {
            npm: None,
            pypi: Some("token".into()),
            cargo: None,
        };
        let mut worker_pypi = Worker::new(
            mock_queue_pypi,
            "stream",
            "group",
            "consumer",
            tokens_pypi,
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");
        worker_pypi.exe_paths.pypi = pypi_exe;
        let job_pypi_ok = PublishJob {
            artifact_id: format!("{}/artifact.zip", mock_server.uri()),
            registry: "pypi".to_string(),
        };
        assert!(worker_pypi.process_job(&job_pypi_ok).await.is_ok());

        // Successful Cargo
        let dest_cargo = tempfile::TempDir::new().expect("Failed to create TempDir");
        let cargo_exe = create_dummy_exe(dest_cargo.path(), "cargo", 0);
        let mut mock_queue_cargo = MockRedisQueue::new();
        mock_queue_cargo
            .expect_create_group()
            .returning(|_, _| Ok(()));
        let tokens_cargo = RegistryTokens {
            npm: None,
            pypi: None,
            cargo: Some("token".into()),
        };
        let mut worker_cargo = Worker::new(
            mock_queue_cargo,
            "stream",
            "group",
            "consumer",
            tokens_cargo,
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");
        worker_cargo.exe_paths.cargo = cargo_exe;
        let job_cargo_ok = PublishJob {
            artifact_id: format!("{}/artifact.zip", mock_server.uri()),
            registry: "cargo".to_string(),
        };
        assert!(worker_cargo.process_job(&job_cargo_ok).await.is_ok());
    }

    #[tokio::test]
    async fn test_worker_run_once_process_error() {
        // Processing failure inside run_once
        let mut mock_queue_err = MockRedisQueue::new();
        mock_queue_err
            .expect_create_group()
            .returning(|_, _| Ok(()));
        let valid_json_unsupported = br#"{"artifact_id":"123","registry":"unsupported"}"#.to_vec();
        mock_queue_err.expect_read_one().returning(move |_, _, _| {
            Ok(Some(("1-0".to_string(), valid_json_unsupported.clone())))
        });
        // Expect NO ack since processing fails
        let mut worker_err = Worker::new(
            mock_queue_err,
            "stream",
            "group",
            "consumer",
            RegistryTokens::default(),
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create");
        assert!(worker_err.run_once().await.is_ok());
    }
    #[tokio::test]
    async fn test_worker_run_once_success() {
        let dest = tempfile::TempDir::new().expect("Failed to create TempDir");
        let npm_exe = create_dummy_exe(dest.path(), "npm", 0);
        let pypi_exe = create_dummy_exe(dest.path(), "twine", 0);
        let cargo_exe = create_dummy_exe(dest.path(), "cargo", 0);
        let exe_paths = ExecutablePaths {
            npm: npm_exe,
            pypi: pypi_exe,
            cargo: cargo_exe,
        };

        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };
        let mock_server = MockServer::start().await;
        let mut zip_data = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_data));
            zip.start_file("hello.txt", zip::write::SimpleFileOptions::default())
                .expect("start_file failed");
            std::io::Write::write_all(&mut zip, b"hello world").expect("write_all failed");
            zip.finish().expect("finish failed");
        }
        Mock::given(method("GET"))
            .and(path("/artifact.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(zip_data))
            .mount(&mock_server)
            .await;

        let mut mock_queue = MockRedisQueue::new();
        mock_queue.expect_create_group().returning(|_, _| Ok(()));
        let job_url = format!("{}/artifact.zip", mock_server.uri());
        let valid_json = format!(r#"{{"artifact_id":"{job_url}","registry":"npm"}}"#).into_bytes();
        mock_queue
            .expect_read_one()
            .returning(move |_, _, _| Ok(Some(("1-0".to_string(), valid_json.clone()))));
        mock_queue.expect_ack().returning(|_, _, _| Ok(()));

        let tokens = RegistryTokens {
            npm: Some("token".into()),
            pypi: None,
            cargo: None,
        };
        let mut worker = Worker::new(
            mock_queue,
            "stream",
            "group",
            "consumer",
            tokens,
            get_dummy_audit_client(),
        )
        .await
        .expect("Failed to create worker");
        worker.exe_paths = exe_paths;
        assert!(worker.run_once().await.is_ok());
    }
}
