//! Main entry point for the `cdd-publisher` application.

pub mod audit;
pub mod error;
pub mod publish;
pub mod storage;
pub mod worker;

use error::Result;
use worker::Worker;

/// Main function for the `cdd-publisher` application.
#[tokio::main]
#[cfg(not(tarpaulin_include))]
#[cfg(not(tarpaulin_include))]
#[cfg(not(tarpaulin_include))]
async fn main() -> Result<()> {
    let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
    let stream_name = "publisher_jobs";
    let group_name = "publisher_group";
    let consumer_name = "worker_1";

    let control_plane_url = std::env::var("CONTROL_PLANE_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
    
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
    let mut worker = Worker::new(connection, stream_name, group_name, consumer_name, tokens, audit_client).await?;
    loop {
        worker.run_once().await?;
    }
}
