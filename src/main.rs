#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
//! Main entry point for the `cdd-publisher` application.

/// Audit client for telemetry.
pub mod audit;
/// Error types and handling.
pub mod error;
/// Registry publishing logic.
pub mod publish;
/// Artifact fetching and extraction.
pub mod storage;
/// Background worker processing loop.
pub mod worker;

use error::Result;
use worker::Worker;

/// Main function for the `cdd-publisher` application.
#[tokio::main]
async fn main() -> Result<()> {
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
    let stream_name = "publisher_jobs";
    let group_name = "publisher_group";
    let consumer_name = "worker_1";

    let control_plane_url =
        std::env::var("CONTROL_PLANE_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

    let tokens = worker::RegistryTokens {
        npm: std::env::var("NPM_TOKEN").ok(),
        pypi: std::env::var("TWINE_PASSWORD").ok(),
        cargo: std::env::var("CARGO_REGISTRY_TOKEN").ok(),
    };

    let client = redis::Client::open(redis_url)?;
    let connection = client.get_multiplexed_async_connection().await?;
    let http_client = reqwest::Client::new();
    let audit_client = audit::AuditClient::new(http_client, control_plane_url);

    println!("Starting cdd-publisher worker...");
    let mut worker = Worker::new(
        connection,
        stream_name,
        group_name,
        consumer_name,
        tokens,
        audit_client,
    )
    .await?;
    loop {
        worker.run_once().await?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_redis_error() {
        unsafe { std::env::set_var("REDIS_URL", "redis://127.0.0.1:1/invalid") }; //("REDIS_URL", "redis://127.0.0.1:1/invalid");
        let result = main();
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn test_main_loop_exit_on_redis_error() {
    // Start a redis server on a specific port
    let mut redis_proc = std::process::Command::new("redis-server")
        .arg("--port")
        .arg("63800")
        .spawn()
        .unwrap();

    // Give it a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    unsafe { std::env::set_var("REDIS_URL", "redis://127.0.0.1:63800/") };

    let handle = tokio::task::spawn_blocking(|| main());

    // Let main connect and enter the loop
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Kill redis
    redis_proc.kill().unwrap();
    redis_proc.wait().unwrap();

    // Main should now exit with an error because the connection was lost
    let result = handle.await.unwrap();
    assert!(result.is_err());
}
