//! Worker loop and queue abstraction for `cdd-publisher`.
//!
//! Coordinates reading publish jobs from the `EventQueue`, fetching and unpacking
//! artifacts via `ArtifactStorage`, resolving monorepo/polyrepo working directories,
//! publishing via `RegistryPublisher` (with optional PGP signing), deploying documentation
//! via `DocsPublisher`, and emitting completion telemetry via `AuditReporter`.

use crate::audit::{AuditClient, AuditPayload, AuditReporter};
use crate::docs_publish::{DocsPublisher, HttpAndGitDocsPublisher};
use crate::domain::{
    ArtifactUrl, ConsumerName, GroupName, MessageId, RawQueueMessage, Registry, StreamConfig,
    StreamName,
};
pub use crate::domain::{PublishJob, RegistryTokens};
use crate::error::{PublisherError, Result};
use crate::publish::{CliRegistryPublisher, ExecutablePaths, RegistryPublisher};
use crate::storage::{ArtifactStorage, HttpArtifactStorage};
use std::future::Future;

/// Abstract interface for the event queue, allowing deterministic mocking in unit tests.
pub trait EventQueue: Send + Sync {
    /// Creates a consumer group and the stream if it does not already exist.
    ///
    /// # Arguments
    ///
    /// * `stream` - The stream name.
    /// * `group` - The consumer group name.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the group is created or already exists.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if creating the consumer group fails with an error other than `BUSYGROUP`.
    fn create_group(
        &mut self,
        stream: &StreamName,
        group: &GroupName,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Reads a single message from the stream for the specified consumer group.
    ///
    /// # Arguments
    ///
    /// * `stream` - The stream name.
    /// * `group` - The consumer group name.
    /// * `consumer` - The consumer identifier within the group.
    ///
    /// # Returns
    ///
    /// `Ok(Some(RawQueueMessage))` if a message was read, or `Ok(None)` on timeout/empty stream.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if the stream read operation fails.
    fn read_one(
        &mut self,
        stream: &StreamName,
        group: &GroupName,
        consumer: &ConsumerName,
    ) -> impl Future<Output = Result<Option<RawQueueMessage>>> + Send;

    /// Acknowledges a processed message by its ID (`XACK`).
    ///
    /// # Arguments
    ///
    /// * `stream` - The stream name.
    /// * `group` - The consumer group name.
    /// * `id` - The message ID to acknowledge.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the message is acknowledged.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if acknowledging the message fails.
    fn ack(
        &mut self,
        stream: &StreamName,
        group: &GroupName,
        id: &MessageId,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// Interprets the result of an `XGROUP CREATE ... MKSTREAM` command, ignoring `BUSYGROUP`
/// errors which indicate the consumer group already exists.
///
/// # Arguments
///
/// * `result` - The raw [`redis::RedisResult`] returned by `xgroup_create_mkstream`.
///
/// # Returns
///
/// `Ok(())` if `result` succeeded or returned `"BUSYGROUP"`.
///
/// # Errors
///
/// Returns [`PublisherError::Redis`] if `result` is an error that does not contain `"BUSYGROUP"`.
pub(crate) fn handle_create_group_result(result: redis::RedisResult<()>) -> Result<()> {
    if let Err(e) = result
        && !e.to_string().contains("BUSYGROUP")
    {
        return Err(PublisherError::Redis(e));
    }
    Ok(())
}

/// Extracts a [`RawQueueMessage`] from a [`redis::streams::StreamReadReply`].
///
/// # Arguments
///
/// * `reply` - The stream read reply returned by `XREADGROUP`.
///
/// # Returns
///
/// `Ok(Some(RawQueueMessage))` if an entry is present, or `Ok(None)` if empty.
///
/// # Errors
///
/// Returns [`PublisherError::RedisParsing`] if a present `"payload"` field cannot be converted into bytes.
pub(crate) fn extract_message_from_reply(
    reply: &redis::streams::StreamReadReply,
) -> Result<Option<RawQueueMessage>> {
    let Some(key) = reply.keys.first() else {
        return Ok(None);
    };
    let Some(entry) = key.ids.first() else {
        return Ok(None);
    };
    let msg_id = MessageId::new(entry.id.clone());
    let Some(payload_value) = entry.map.get("payload") else {
        return Ok(Some(RawQueueMessage::new(msg_id, Vec::new())));
    };
    let payload_bytes: Vec<u8> = redis::FromRedisValue::from_redis_value(payload_value.clone())?;
    Ok(Some(RawQueueMessage::new(msg_id, payload_bytes)))
}

/// Implementation of [`EventQueue`] for any type implementing [`redis::aio::ConnectionLike`].
impl<C> EventQueue for C
where
    C: redis::aio::ConnectionLike + Send + Sync,
{
    async fn create_group(&mut self, stream: &StreamName, group: &GroupName) -> Result<()> {
        let s = stream.as_str();
        let g = group.as_str();
        let result: redis::RedisResult<()> =
            redis::AsyncCommands::xgroup_create_mkstream(self, s, g, "$").await;
        handle_create_group_result(result)
    }

    async fn read_one(
        &mut self,
        stream: &StreamName,
        group: &GroupName,
        consumer: &ConsumerName,
    ) -> Result<Option<RawQueueMessage>> {
        let opts = redis::streams::StreamReadOptions::default()
            .group(group.as_str(), consumer.as_str())
            .block(5000)
            .count(1);
        let keys = [stream.as_str()];
        let ids = [">"];
        let reply: redis::streams::StreamReadReply =
            redis::AsyncCommands::xread_options(self, &keys, &ids, &opts).await?;
        extract_message_from_reply(&reply)
    }

    async fn ack(&mut self, stream: &StreamName, group: &GroupName, id: &MessageId) -> Result<()> {
        let s = stream.as_str();
        let g = group.as_str();
        let ids = [id.as_str()];
        let (): () = redis::AsyncCommands::xack(self, s, g, &ids).await?;
        Ok(())
    }
}

/// The worker that processes publish jobs from an [`EventQueue`].
pub struct Worker<
    Q: EventQueue,
    S: ArtifactStorage = HttpArtifactStorage,
    P: RegistryPublisher = CliRegistryPublisher,
    A: AuditReporter = AuditClient,
    D: DocsPublisher = HttpAndGitDocsPublisher,
> {
    /// The event queue connection (e.g. Redis).
    connection: Q,
    /// Strongly-typed stream, group, and consumer configuration.
    stream_config: StreamConfig,
    /// Artifact storage service used to download and unpack bundles.
    storage: S,
    /// Registry publisher service used to execute package manager CLIs.
    publisher: P,
    /// Multi-target documentation publisher service.
    docs_publisher: D,
    /// Registry authentication tokens.
    tokens: RegistryTokens,
    /// Audit reporter used to notify the control plane of job outcomes.
    audit_client: A,
}

impl<Q: EventQueue>
    Worker<Q, HttpArtifactStorage, CliRegistryPublisher, AuditClient, HttpAndGitDocsPublisher>
{
    /// Creates a new [`Worker`] backed by [`HttpArtifactStorage`], [`CliRegistryPublisher`],
    /// [`HttpAndGitDocsPublisher`], and [`AuditClient`].
    ///
    /// # Arguments
    ///
    /// * `connection` - The [`EventQueue`] implementation.
    /// * `stream_config` - Stream, group, and consumer names.
    /// * `tokens` - Configured [`RegistryTokens`].
    /// * `audit_client` - The [`AuditClient`] for telemetry reporting.
    ///
    /// # Returns
    ///
    /// An initialized [`Worker`].
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if the consumer group cannot be created on `connection`.
    pub async fn new(
        connection: Q,
        stream_config: StreamConfig,
        tokens: RegistryTokens,
        audit_client: AuditClient,
    ) -> Result<Self> {
        Self::with_dependencies(
            connection,
            stream_config,
            HttpArtifactStorage::default(),
            CliRegistryPublisher::default(),
            HttpAndGitDocsPublisher::default(),
            tokens,
            audit_client,
        )
        .await
    }

    /// Replaces the [`ExecutablePaths`] used by the worker's [`CliRegistryPublisher`].
    ///
    /// # Arguments
    ///
    /// * `exe_paths` - Custom executable paths for supported registry toolchains.
    pub fn set_exe_paths(&mut self, exe_paths: ExecutablePaths) {
        self.publisher = CliRegistryPublisher::with_paths(exe_paths);
    }

    /// Replaces the [`HttpAndGitDocsPublisher`] instance used by the worker.
    ///
    /// # Arguments
    ///
    /// * `docs_publisher` - Configured [`HttpAndGitDocsPublisher`].
    pub fn set_docs_publisher(&mut self, docs_publisher: HttpAndGitDocsPublisher) {
        self.docs_publisher = docs_publisher;
    }
}

impl<Q, S, P, A, D> Worker<Q, S, P, A, D>
where
    Q: EventQueue,
    S: ArtifactStorage,
    P: RegistryPublisher,
    A: AuditReporter,
    D: DocsPublisher,
{
    /// Creates a new [`Worker`] with explicitly injected storage, publisher, docs, and audit dependencies.
    ///
    /// # Arguments
    ///
    /// * `connection` - The [`EventQueue`] implementation.
    /// * `stream_config` - Stream, group, and consumer names.
    /// * `storage` - The [`ArtifactStorage`] implementation.
    /// * `publisher` - The [`RegistryPublisher`] implementation.
    /// * `docs_publisher` - The [`DocsPublisher`] implementation.
    /// * `tokens` - Configured [`RegistryTokens`].
    /// * `audit_client` - The [`AuditReporter`] implementation.
    ///
    /// # Returns
    ///
    /// An initialized [`Worker`].
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if the consumer group cannot be created on `connection`.
    pub async fn with_dependencies(
        mut connection: Q,
        stream_config: StreamConfig,
        storage: S,
        publisher: P,
        docs_publisher: D,
        tokens: RegistryTokens,
        audit_client: A,
    ) -> Result<Self> {
        connection
            .create_group(&stream_config.stream_name, &stream_config.group_name)
            .await?;

        Ok(Self {
            connection,
            stream_config,
            storage,
            publisher,
            docs_publisher,
            tokens,
            audit_client,
        })
    }

    /// Runs a single iteration of the worker loop (reads and processes at most one message).
    ///
    /// # Returns
    ///
    /// `Ok(())` after processing or skipping a single message.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if reading from the stream fails or if acknowledging
    /// a processed or malformed message fails.
    pub async fn run_once(&mut self) -> Result<()> {
        let s = &self.stream_config.stream_name;
        let g = &self.stream_config.group_name;
        let c = &self.stream_config.consumer_name;
        let Some(raw_msg) = self.connection.read_one(s, g, c).await? else {
            return Ok(());
        };

        if raw_msg.payload.is_empty() {
            return self.acknowledge(&raw_msg.id).await;
        }

        match serde_json::from_slice::<PublishJob>(&raw_msg.payload) {
            Ok(job) => {
                let process_result = self.process_job(&job).await;

                if let Ok(registry) = job.parsed_registry() {
                    self.report_audit(&job.artifact_id, registry, &process_result)
                        .await;
                }

                if let Err(e) = process_result {
                    eprintln!("Failed to process job: {e}");
                    Ok(())
                } else {
                    self.acknowledge(&raw_msg.id).await
                }
            }
            Err(e) => {
                eprintln!("Failed to parse job payload: {e}");
                self.acknowledge(&raw_msg.id).await
            }
        }
    }

    /// Reports the audit status of a processed job to the control plane.
    ///
    /// # Arguments
    ///
    /// * `artifact_id` - The artifact URL/ID of the job.
    /// * `registry` - The validated [`Registry`] of the job.
    /// * `process_result` - The outcome of [`Self::process_job`].
    async fn report_audit(
        &self,
        artifact_id: &ArtifactUrl,
        registry: Registry,
        process_result: &Result<()>,
    ) {
        let payload = match process_result {
            Ok(()) => AuditPayload::success(artifact_id.clone(), registry),
            Err(e) => AuditPayload::failure(artifact_id.clone(), registry, e.to_string()),
        };
        if let Err(e) = self.audit_client.report_status(&payload).await {
            eprintln!("Failed to report audit status: {e}");
        }
    }

    /// Processes a single [`PublishJob`].
    ///
    /// Validates the target registry and token before downloading/unpacking the archive,
    /// resolving the monorepo/polyrepo working directory, invoking the [`RegistryPublisher`]
    /// (with optional PGP signing), and optionally executing [`DocsPublisher`].
    ///
    /// # Arguments
    ///
    /// * `job` - The deserialized [`PublishJob`] to execute.
    ///
    /// # Returns
    ///
    /// `Ok(())` when package and optional documentation publishing succeed.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if the registry is unsupported, the registry token is missing,
    /// downloading/unpacking fails, the monorepo subdirectory is unsafe, or publishing fails.
    pub async fn process_job(&self, job: &PublishJob) -> Result<()> {
        let registry = job.parsed_registry()?;
        let token = self.tokens.token_for(registry)?;

        let dest_dir = tempfile::TempDir::new()?;
        self.storage
            .fetch_and_unpack(&job.artifact_id, dest_dir.path())
            .await?;

        let working_dir = job.resolve_working_dir(dest_dir.path())?;
        std::fs::create_dir_all(&working_dir)?;

        self.publisher
            .publish_with_options(
                registry,
                &working_dir,
                token,
                job.topology_mode,
                job.pgp_config.as_ref(),
            )
            .await?;

        if let Some(docs_job) = &job.docs_job {
            let _published_url = self
                .docs_publisher
                .publish_docs(docs_job, &working_dir, &self.tokens)
                .await?;
        }

        Ok(())
    }

    /// Acknowledges a message on the underlying [`EventQueue`].
    ///
    /// # Arguments
    ///
    /// * `id` - The [`MessageId`] to acknowledge.
    ///
    /// # Returns
    ///
    /// `Ok(())` when acknowledged.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if the `XACK` operation fails.
    async fn acknowledge(&mut self, id: &MessageId) -> Result<()> {
        let s = &self.stream_config.stream_name;
        let g = &self.stream_config.group_name;
        self.connection.ack(s, g, id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DocHostTarget, DocsPublishJob, RepoTopologyMode, SecretToken};
    use std::collections::{HashMap, VecDeque};
    use std::future::ready;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, path_regex},
    };

    /// Creates a mock executable script with the given exit code.
    fn create_dummy_exe(dir: &std::path::Path, name: &str, exit_code: i32) -> Result<String> {
        use std::os::unix::fs::PermissionsExt;
        let exe_path = dir.join(name);
        let script = format!("#!/bin/sh\nexit {exit_code}\n");
        std::fs::write(&exe_path, script)?;
        std::fs::set_permissions(&exe_path, std::fs::Permissions::from_mode(0o755))?;
        Ok(exe_path.to_string_lossy().to_string())
    }

    /// Creates an [`AuditClient`] pointing at `url`.
    fn get_dummy_audit_client(url: &str) -> AuditClient {
        AuditClient::new(reqwest::Client::new(), url)
    }

    /// Creates an in-memory ZIP archive containing a single `hello.txt` file.
    fn make_valid_zip_bytes() -> Result<Vec<u8>> {
        let mut zip_data = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_data));
            zip.start_file("hello.txt", zip::write::SimpleFileOptions::default())?;
            std::io::Write::write_all(&mut zip, b"hello world")?;
            zip.finish()?;
        }
        Ok(zip_data)
    }

    /// Deterministic in-memory [`EventQueue`] implementation for unit testing [`Worker`].
    #[derive(Default)]
    struct FakeQueue {
        /// Result to return from `create_group`.
        create_group_error: Option<String>,
        /// Canned messages to pop on `read_one`.
        messages: VecDeque<Option<RawQueueMessage>>,
        /// Recorded message IDs acknowledged via `ack`.
        acked_ids: Vec<MessageId>,
    }

    impl FakeQueue {
        /// Creates a new [`FakeQueue`] with a single queued message.
        fn with_message(msg: RawQueueMessage) -> Self {
            let mut messages = VecDeque::new();
            messages.push_back(Some(msg));
            Self {
                create_group_error: None,
                messages,
                acked_ids: Vec::new(),
            }
        }
    }

    impl EventQueue for FakeQueue {
        fn create_group(
            &mut self,
            _stream: &StreamName,
            _group: &GroupName,
        ) -> impl Future<Output = Result<()>> + Send {
            let res = self
                .create_group_error
                .as_ref()
                .map_or(Ok(()), |msg| Err(PublisherError::Publish(msg.clone())));
            ready(res)
        }

        fn read_one(
            &mut self,
            _stream: &StreamName,
            _group: &GroupName,
            _consumer: &ConsumerName,
        ) -> impl Future<Output = Result<Option<RawQueueMessage>>> + Send {
            let next = self.messages.pop_front().flatten();
            ready(Ok(next))
        }

        fn ack(
            &mut self,
            _stream: &StreamName,
            _group: &GroupName,
            id: &MessageId,
        ) -> impl Future<Output = Result<()>> + Send {
            self.acked_ids.push(id.clone());
            ready(Ok(()))
        }
    }

    /// Deterministic in-memory implementation of [`redis::aio::ConnectionLike`] for testing
    /// the [`EventQueue`] blanket implementation without external network dependencies.
    struct FakeRedisConnection {
        /// Canned response value returned for each packed command.
        next_response: redis::Value,
    }

    impl redis::aio::ConnectionLike for FakeRedisConnection {
        fn req_packed_command<'a>(
            &'a mut self,
            _cmd: &'a redis::Cmd,
        ) -> redis::RedisFuture<'a, redis::Value> {
            let val = self.next_response.clone();
            Box::pin(async move { Ok(val) })
        }

        fn req_packed_commands<'a>(
            &'a mut self,
            _cmd: &'a redis::Pipeline,
            _offset: usize,
            _count: usize,
        ) -> redis::RedisFuture<'a, Vec<redis::Value>> {
            let val = self.next_response.clone();
            Box::pin(async move { Ok(vec![val]) })
        }

        fn get_db(&self) -> i64 {
            0
        }
    }

    #[test]
    fn test_publish_job_serialization_and_registry_parsing() -> Result<()> {
        let job = PublishJob::new("test-artifact-123.zip", "npm");
        assert_eq!(job.parsed_registry()?, Registry::Npm);

        let json = serde_json::to_string(&job)?;
        let deserialized = serde_json::from_str::<PublishJob>(&json)?;
        assert_eq!(job, deserialized);
        Ok(())
    }

    #[tokio::test]
    async fn test_worker_new_success_and_error() -> Result<()> {
        let mock_queue = FakeQueue::default();
        let worker = Worker::new(
            mock_queue,
            StreamConfig::default(),
            RegistryTokens::default(),
            get_dummy_audit_client("http://localhost"),
        )
        .await;
        assert!(worker.is_ok());

        let err_queue = FakeQueue {
            create_group_error: Some("group err".to_string()),
            ..FakeQueue::default()
        };
        let err_worker = Worker::new(
            err_queue,
            StreamConfig::default(),
            RegistryTokens::default(),
            get_dummy_audit_client("http://localhost"),
        )
        .await;
        assert!(err_worker.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn test_worker_run_once_no_messages_empty_and_invalid_json() -> Result<()> {
        let mock_queue = FakeQueue::default();
        let mut worker = Worker::new(
            mock_queue,
            StreamConfig::default(),
            RegistryTokens::default(),
            get_dummy_audit_client("http://localhost"),
        )
        .await?;
        worker.run_once().await?;
        assert_eq!(worker.connection.acked_ids, Vec::<MessageId>::new());

        let empty_queue = FakeQueue::with_message(RawQueueMessage::new("1-0", Vec::new()));
        let mut worker_empty = Worker::new(
            empty_queue,
            StreamConfig::default(),
            RegistryTokens::default(),
            get_dummy_audit_client("http://localhost"),
        )
        .await?;
        worker_empty.run_once().await?;
        assert_eq!(
            worker_empty.connection.acked_ids,
            vec![MessageId::new("1-0")]
        );

        let invalid_queue =
            FakeQueue::with_message(RawQueueMessage::new("2-0", b"invalid".to_vec()));
        let mut worker_invalid = Worker::new(
            invalid_queue,
            StreamConfig::default(),
            RegistryTokens::default(),
            get_dummy_audit_client("http://localhost"),
        )
        .await?;
        worker_invalid.run_once().await?;
        assert_eq!(
            worker_invalid.connection.acked_ids,
            vec![MessageId::new("2-0")]
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_process_job_monorepo_docs_and_failure_branches() -> Result<()> {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/artifact.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(make_valid_zip_bytes()?))
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/upload/.*"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let dest = tempfile::TempDir::new()?;
        let ok_exe = create_dummy_exe(dest.path(), "ok_cli", 0)?;
        let fail_exe = create_dummy_exe(dest.path(), "fail_cli", 1)?;

        let tokens = RegistryTokens {
            npm: Some(SecretToken::new("tok")),
            pypi: Some(SecretToken::new("tok")),
            cargo: Some(SecretToken::new("tok")),
            ..RegistryTokens::default()
        };
        let mut worker = Worker::new(
            FakeQueue::default(),
            StreamConfig::default(),
            tokens,
            get_dummy_audit_client("http://localhost"),
        )
        .await?;

        worker.set_exe_paths(ExecutablePaths::with_all(&ok_exe));
        worker.set_docs_publisher(HttpAndGitDocsPublisher::new(
            reqwest::Client::new(),
            mock_server.uri(),
            "https://docs.cdd.dev",
        ));

        let artifact_url = ArtifactUrl::new(format!("{}/artifact.zip", mock_server.uri()));
        let mut job = PublishJob::new(artifact_url, "pypi");
        job.topology_mode = RepoTopologyMode::Monorepo;
        job.sub_directory = Some("python".to_string());
        job.docs_job = Some(DocsPublishJob {
            target: DocHostTarget::OfficialCddSite,
            org_id: "acme".to_string(),
            repo_id: "acme-sdk".to_string(),
            version: "1.0.0".to_string(),
            github_org_name: None,
            repo_name: None,
            github_pages_branch: None,
            custom_url: None,
        });

        worker.process_job(&job).await?;

        // Test docs_job failure branch (CustomUrl without custom_url set)
        job.docs_job = Some(DocsPublishJob {
            target: DocHostTarget::CustomUrl,
            org_id: "acme".to_string(),
            repo_id: "acme-sdk".to_string(),
            version: "1.0.0".to_string(),
            github_org_name: None,
            repo_name: None,
            github_pages_branch: None,
            custom_url: None,
        });
        assert!(worker.process_job(&job).await.is_err());

        // Test unsafe monorepo sub_directory branch
        job.sub_directory = Some("../evil".to_string());
        assert!(worker.process_job(&job).await.is_err());

        // Test CLI failure branch
        job.sub_directory = None;
        worker.set_exe_paths(ExecutablePaths::with_all(&fail_exe));
        assert!(worker.process_job(&job).await.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_worker_run_once_process_error_and_unsupported_registry() -> Result<()> {
        let valid_json_missing_token =
            br#"{"artifact_id":"http://localhost/a.zip","registry":"npm"}"#.to_vec();
        let mock_queue_err =
            FakeQueue::with_message(RawQueueMessage::new("1-0", valid_json_missing_token));

        let mut worker_err = Worker::new(
            mock_queue_err,
            StreamConfig::default(),
            RegistryTokens::default(),
            get_dummy_audit_client("http://127.0.0.1:1"),
        )
        .await?;
        worker_err.run_once().await?;
        assert_eq!(worker_err.connection.acked_ids, Vec::<MessageId>::new());

        let valid_json_unsupported = br#"{"artifact_id":"123","registry":"unsupported"}"#.to_vec();
        let mock_queue_unsup =
            FakeQueue::with_message(RawQueueMessage::new("1-0", valid_json_unsupported));
        let mut worker_unsup = Worker::new(
            mock_queue_unsup,
            StreamConfig::default(),
            RegistryTokens::default(),
            get_dummy_audit_client("http://localhost"),
        )
        .await?;
        worker_unsup.run_once().await?;
        assert_eq!(worker_unsup.connection.acked_ids, Vec::<MessageId>::new());
        Ok(())
    }

    #[tokio::test]
    async fn test_worker_run_once_success() -> Result<()> {
        let dest = tempfile::TempDir::new()?;
        let ok_exe = create_dummy_exe(dest.path(), "ok_cli", 0)?;

        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/artifact.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(make_valid_zip_bytes()?))
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/audit/publish"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let job_url = format!("{}/artifact.zip", mock_server.uri());
        let valid_json = format!(r#"{{"artifact_id":"{job_url}","registry":"npm"}}"#).into_bytes();
        let mock_queue = FakeQueue::with_message(RawQueueMessage::new("1-0", valid_json));

        let tokens = RegistryTokens {
            npm: Some(SecretToken::new("token")),
            ..RegistryTokens::default()
        };
        let mut worker = Worker::new(
            mock_queue,
            StreamConfig::default(),
            tokens,
            get_dummy_audit_client(&mock_server.uri()),
        )
        .await?;
        worker.set_exe_paths(ExecutablePaths::with_all(&ok_exe));
        worker.run_once().await?;
        assert_eq!(worker.connection.acked_ids, vec![MessageId::new("1-0")]);
        Ok(())
    }

    #[test]
    fn test_handle_create_group_result_and_extract_message() -> Result<()> {
        assert!(handle_create_group_result(Ok(())).is_ok());

        let busy_err = redis::RedisError::from(std::io::Error::other(
            "BUSYGROUP Consumer Group name already exists",
        ));
        assert!(handle_create_group_result(Err(busy_err)).is_ok());

        let other_err = redis::RedisError::from(std::io::Error::other("connection refused"));
        assert!(handle_create_group_result(Err(other_err)).is_err());

        let empty_reply = redis::streams::StreamReadReply { keys: vec![] };
        assert_eq!(extract_message_from_reply(&empty_reply)?, None);

        let empty_ids_reply = redis::streams::StreamReadReply {
            keys: vec![redis::streams::StreamKey {
                key: "stream".to_string(),
                ids: vec![],
            }],
        };
        assert_eq!(extract_message_from_reply(&empty_ids_reply)?, None);

        let no_payload_reply = redis::streams::StreamReadReply {
            keys: vec![redis::streams::StreamKey {
                key: "stream".to_string(),
                ids: vec![redis::streams::StreamId {
                    id: "1-0".to_string(),
                    map: HashMap::new(),
                    milliseconds_elapsed_from_delivery: None,
                    delivered_count: None,
                }],
            }],
        };
        assert_eq!(
            extract_message_from_reply(&no_payload_reply)?,
            Some(RawQueueMessage::new("1-0", Vec::new()))
        );

        let mut valid_map = HashMap::new();
        valid_map.insert(
            "payload".to_string(),
            redis::Value::BulkString(b"hello".to_vec()),
        );
        let valid_reply = redis::streams::StreamReadReply {
            keys: vec![redis::streams::StreamKey {
                key: "stream".to_string(),
                ids: vec![redis::streams::StreamId {
                    id: "2-0".to_string(),
                    map: valid_map,
                    milliseconds_elapsed_from_delivery: None,
                    delivered_count: None,
                }],
            }],
        };
        assert_eq!(
            extract_message_from_reply(&valid_reply)?,
            Some(RawQueueMessage::new("2-0", b"hello".to_vec()))
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_redis_event_queue_implementation_via_connection_like() -> Result<()> {
        use redis::aio::ConnectionLike;

        let mut conn = FakeRedisConnection {
            next_response: redis::Value::Okay,
        };
        assert_eq!(conn.get_db(), 0);
        let mut pipe = redis::Pipeline::new();
        pipe.cmd("PING");
        let pipe_res = conn.req_packed_commands(&pipe, 0, 1).await?;
        assert_eq!(pipe_res.len(), 1);

        conn.create_group(&StreamName::new("s"), &GroupName::new("g"))
            .await?;

        conn.next_response = redis::Value::Array(vec![]);
        let msg = conn
            .read_one(
                &StreamName::new("s"),
                &GroupName::new("g"),
                &ConsumerName::new("c"),
            )
            .await?;
        assert!(msg.is_none());

        conn.next_response = redis::Value::Int(1);
        conn.ack(
            &StreamName::new("s"),
            &GroupName::new("g"),
            &MessageId::new("1-0"),
        )
        .await?;

        Ok(())
    }
}
