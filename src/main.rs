#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
//! Main entry point and application bootstrap for `cdd-publisher`.

/// Audit client for telemetry reporting to `cdd-control-plane`.
pub mod audit;
/// Multi-target documentation publishing engine.
pub mod docs_publish;
/// Strong domain primitives and value objects.
pub mod domain;
/// Unified error enum and `Result` alias.
pub mod error;
/// Registry publishing CLI abstractions and implementations.
pub mod publish;
/// Ephemeral PGP/GPG cryptographic signing engine.
pub mod signing;
/// Artifact downloading and archive unpacking.
pub mod storage;
/// Background worker processing loop and queue trait.
pub mod worker;

use audit::AuditClient;
use domain::{ControlPlaneUrl, RegistryTokens, SecretToken, StreamConfig};
use error::Result;
use worker::{EventQueue, Worker};

/// Runtime configuration for the `cdd-publisher` worker node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    /// Redis connection URL (defaults to `"redis://127.0.0.1/"`).
    pub redis_url: String,
    /// Base URL for `cdd-control-plane` (defaults to `"http://127.0.0.1:8080"`).
    pub control_plane_url: ControlPlaneUrl,
    /// Redis stream, consumer group, and consumer names.
    pub stream_config: StreamConfig,
    /// Optional authentication tokens for all 13 supported registries and doc hosts.
    pub tokens: RegistryTokens,
}

impl AppConfig {
    /// Constructs an [`AppConfig`] using a lookup function for environment variables.
    ///
    /// Accepting a lookup closure allows deterministic unit testing without mutating
    /// process-global environment variables (`unsafe { std::env::set_var(...) }`).
    ///
    /// # Arguments
    ///
    /// * `get_env` - A function or closure taking an environment variable key and returning `Option<String>`.
    ///
    /// # Returns
    ///
    /// A populated [`AppConfig`].
    #[must_use]
    pub fn from_env_fn<F>(mut get_env: F) -> Self
    where
        F: FnMut(&str) -> Option<String>,
    {
        let redis_url = get_env("REDIS_URL").unwrap_or_else(|| "redis://127.0.0.1/".to_string());
        let control_plane_raw =
            get_env("CONTROL_PLANE_URL").unwrap_or_else(|| "http://127.0.0.1:8080".to_string());

        let tokens = RegistryTokens {
            npm: get_env("NPM_TOKEN").map(SecretToken::new),
            pypi: get_env("TWINE_PASSWORD").map(SecretToken::new),
            cargo: get_env("CARGO_REGISTRY_TOKEN").map(SecretToken::new),
            maven_username: get_env("MAVEN_USERNAME").map(SecretToken::new),
            maven_password: get_env("MAVEN_PASSWORD").map(SecretToken::new),
            nuget: get_env("NUGET_API_KEY").map(SecretToken::new),
            rubygems: get_env("GEM_HOST_API_KEY").map(SecretToken::new),
            packagist: get_env("PACKAGIST_TOKEN").map(SecretToken::new),
            github: get_env("GITHUB_TOKEN").map(SecretToken::new),
            conan: get_env("CONAN_PASSWORD").map(SecretToken::new),
            vcpkg_repo_url: get_env("VCPKG_REPO_URL").map(SecretToken::new),
            homebrew_tap_repo: get_env("HOMEBREW_TAP_REPO").map(SecretToken::new),
            readthedocs: get_env("READTHEDOCS_TOKEN").map(SecretToken::new),
        };

        Self {
            redis_url,
            control_plane_url: ControlPlaneUrl::new(control_plane_raw),
            stream_config: StreamConfig::default(),
            tokens,
        }
    }

    /// Loads [`AppConfig`] from the current process environment (`std::env::var`).
    ///
    /// # Returns
    ///
    /// An [`AppConfig`] populated from process environment variables.
    #[must_use]
    pub fn from_process_env() -> Self {
        Self::from_env_fn(|key| std::env::var(key).ok())
    }
}

/// Runs the worker loop on the provided [`EventQueue`] connection until `keep_running`
/// returns `false` or an unrecoverable queue error occurs.
///
/// # Arguments
///
/// * `connection` - The event queue connection to consume jobs from.
/// * `config` - The application configuration.
/// * `keep_running` - A predicate evaluated before each iteration of the worker loop.
///
/// # Returns
///
/// `Ok(())` when `keep_running` returns `false`.
///
/// # Errors
///
/// Returns [`error::PublisherError`] if initializing the consumer group or executing
/// an iteration of the worker loop fails.
pub async fn run_worker_with_queue<Q, F>(
    connection: Q,
    config: AppConfig,
    mut keep_running: F,
) -> Result<()>
where
    Q: EventQueue,
    F: FnMut() -> bool,
{
    let http_client = reqwest::Client::new();
    let audit_client = AuditClient::new(http_client, config.control_plane_url);
    let stream_cfg = config.stream_config;
    let tokens = config.tokens;
    let mut worker = Worker::new(connection, stream_cfg, tokens, audit_client).await?;

    while keep_running() {
        worker.run_once().await?;
    }
    Ok(())
}

/// Connects to Redis using `config` and runs the worker loop until `keep_running` returns `false`.
///
/// # Arguments
///
/// * `config` - The application configuration containing the Redis URL and tokens.
/// * `keep_running` - A predicate evaluated before each iteration of the worker loop.
///
/// # Returns
///
/// `Ok(())` when `keep_running` returns `false`.
///
/// # Errors
///
/// Returns [`error::PublisherError`] if opening the Redis client, establishing the async
/// connection, or running the worker fails.
pub async fn run_with_config<F>(config: AppConfig, keep_running: F) -> Result<()>
where
    F: FnMut() -> bool,
{
    let client = redis::Client::open(config.redis_url.as_str())?;
    let connection = client.get_multiplexed_async_connection().await?;
    run_worker_with_queue(connection, config, keep_running).await
}

/// Main entry point for the `cdd-publisher` binary.
#[cfg(not(tarpaulin_include))]
#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_process_env();
    run_with_config(config, || true).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ConsumerName, GroupName, MessageId, RawQueueMessage, StreamName};
    use std::collections::HashMap;
    use std::future::{Future, ready};

    /// Deterministic in-memory queue for testing [`run_worker_with_queue`].
    #[derive(Default)]
    struct TestAppQueue {
        /// Number of times `read_one` was invoked.
        reads: usize,
    }

    impl EventQueue for TestAppQueue {
        fn create_group(
            &mut self,
            _stream: &StreamName,
            _group: &GroupName,
        ) -> impl Future<Output = Result<()>> + Send {
            ready(Ok(()))
        }

        fn read_one(
            &mut self,
            _stream: &StreamName,
            _group: &GroupName,
            _consumer: &ConsumerName,
        ) -> impl Future<Output = Result<Option<RawQueueMessage>>> + Send {
            self.reads += 1;
            ready(Ok(None))
        }

        fn ack(
            &mut self,
            _stream: &StreamName,
            _group: &GroupName,
            _id: &MessageId,
        ) -> impl Future<Output = Result<()>> + Send {
            ready(Ok(()))
        }
    }

    #[test]
    fn test_app_config_from_env_fn_defaults_and_overrides() {
        let default_cfg = AppConfig::from_env_fn(|_| None);
        assert_eq!(default_cfg.redis_url, "redis://127.0.0.1/");
        assert_eq!(
            default_cfg.control_plane_url.as_str(),
            "http://127.0.0.1:8080"
        );
        assert!(default_cfg.tokens.npm.is_none());
        assert!(default_cfg.tokens.pypi.is_none());
        assert!(default_cfg.tokens.cargo.is_none());

        let mut env_map = HashMap::new();
        env_map.insert("REDIS_URL", "redis://localhost:6380/".to_string());
        env_map.insert("CONTROL_PLANE_URL", "https://cp.example.com/".to_string());
        env_map.insert("NPM_TOKEN", "npm-1".to_string());
        env_map.insert("TWINE_PASSWORD", "pypi-2".to_string());
        env_map.insert("CARGO_REGISTRY_TOKEN", "cargo-3".to_string());
        env_map.insert("MAVEN_USERNAME", "mvn-u".to_string());
        env_map.insert("MAVEN_PASSWORD", "mvn-p".to_string());
        env_map.insert("NUGET_API_KEY", "nuget-k".to_string());
        env_map.insert("GEM_HOST_API_KEY", "gem-k".to_string());
        env_map.insert("PACKAGIST_TOKEN", "php-k".to_string());
        env_map.insert("GITHUB_TOKEN", "gh-k".to_string());
        env_map.insert("CONAN_PASSWORD", "conan-k".to_string());
        env_map.insert("VCPKG_REPO_URL", "vcpkg-u".to_string());
        env_map.insert("HOMEBREW_TAP_REPO", "brew-t".to_string());
        env_map.insert("READTHEDOCS_TOKEN", "rtd-t".to_string());

        let custom_cfg = AppConfig::from_env_fn(|k| env_map.get(k).cloned());
        assert_eq!(custom_cfg.redis_url, "redis://localhost:6380/");
        assert_eq!(
            custom_cfg.control_plane_url.as_str(),
            "https://cp.example.com"
        );
        assert_eq!(
            custom_cfg
                .tokens
                .npm
                .as_ref()
                .map(SecretToken::expose_secret),
            Some("npm-1")
        );
        assert_eq!(
            custom_cfg
                .tokens
                .readthedocs
                .as_ref()
                .map(SecretToken::expose_secret),
            Some("rtd-t")
        );

        let proc_cfg = AppConfig::from_process_env();
        assert_ne!(proc_cfg.redis_url, "");
    }

    #[tokio::test]
    async fn test_run_worker_with_queue_executes_bounded_iterations() -> Result<()> {
        let mut queue = TestAppQueue::default();
        queue
            .ack(
                &StreamName::new("s"),
                &GroupName::new("g"),
                &MessageId::new("1-0"),
            )
            .await?;

        let config = AppConfig::from_env_fn(|_| None);
        let mut remaining = 2usize;
        run_worker_with_queue(queue, config, || {
            if remaining == 0 {
                false
            } else {
                remaining -= 1;
                true
            }
        })
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_run_with_config_invalid_redis_url_and_unreachable_port() -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let mut bad_scheme_cfg = AppConfig::from_env_fn(|_| None);
        bad_scheme_cfg.redis_url = "not_a_valid_redis_url".to_string();
        assert!(run_with_config(bad_scheme_cfg, || false).await.is_err());

        let mut unreachable_cfg = AppConfig::from_env_fn(|_| None);
        unreachable_cfg.redis_url = "redis://127.0.0.1:1/".to_string();
        assert!(run_with_config(unreachable_cfg, || false).await.is_err());

        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let server_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut buf = [0u8; 512];
            loop {
                let n = stream.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                let req = String::from_utf8_lossy(&buf[..n]);
                let replies = req.matches('*').count().max(1);
                for _ in 0..replies {
                    let _ = stream.write_all(b"+OK\r\n").await;
                }
            }
            Ok::<(), std::io::Error>(())
        });

        let mut ok_cfg = AppConfig::from_env_fn(|_| None);
        ok_cfg.redis_url = format!("redis://{addr}/");
        run_with_config(ok_cfg, || false).await?;
        server_task.await??;

        // SAFETY: `test_app_config_from_env_fn_defaults_and_overrides` only asserts `redis_url != ""`.
        unsafe {
            std::env::set_var("REDIS_URL", "invalid_redis_scheme://127.0.0.1/");
        }
        let main_res = std::thread::spawn(main).join();
        assert!(matches!(main_res, Ok(Err(_))));
        Ok(())
    }
}
