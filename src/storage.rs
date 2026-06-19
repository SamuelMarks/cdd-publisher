//! Module for fetching and unpacking generated artifacts.

use crate::error::Result;
use reqwest::Client;

use std::path::Path;

/// Fetches an artifact from the given URL and unpacks it to the specified destination directory.
///
/// Supports both `.zip` and `.tar.gz` formats.
///
/// # Errors
///
/// Returns an error if the HTTP request fails, if I/O operations fail, or if
/// the archive format is unsupported or corrupt.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
pub async fn fetch_and_unpack(client: &Client, url: &str, dest_dir: &Path) -> Result<()> {
    let is_tar_gz = url.ends_with(".tar.gz") || url.ends_with(".tgz");
    let is_zip = url.ends_with(".zip");

    if !is_tar_gz && !is_zip {
        return Err(crate::error::PublisherError::Publish(
            "Unsupported artifact format".to_string(),
        ));
    }
    {
        let response = client.get(url).send().await?.error_for_status()?;
        let bytes = response.bytes().await?;

        let temp_dir = tempfile::tempdir()?;
        let temp_file_path = temp_dir.path().join("archive");
        std::fs::write(&temp_file_path, &bytes)?;
        let temp_file = std::fs::File::open(&temp_file_path)?;

        if is_zip {
            let mut archive = zip::ZipArchive::new(temp_file)?;
            archive.extract(dest_dir)?;
        } else {
            let tar = flate2::read::GzDecoder::new(temp_file);
            let mut archive = tar::Archive::new(tar);
            archive.unpack(dest_dir)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[tokio::test]
    async fn test_fetch_and_unpack_unsupported_format() {
        let client = Client::new();
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));

        let result = fetch_and_unpack(&client, "http://localhost/file.txt", dest.path()).await;
        assert!(result.is_err());
        match result {
            Err(e) => {
                println!("ERROR IS: {e}");
                assert!(e.to_string().contains("Unsupported artifact format"));
            }
            Ok(()) => panic!("Expected error"),
        }
    }

    #[tokio::test]
    async fn test_fetch_and_unpack_tar_gz() {
        let mock_server = MockServer::start().await;

        // Create a valid tar.gz file in memory
        let mut tar_gz_data = Vec::new();
        {
            let encoder =
                flate2::write::GzEncoder::new(&mut tar_gz_data, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);

            let mut header = tar::Header::new_gnu();
            header
                .set_path("hello.txt")
                .unwrap_or_else(|_| panic!("set_path failed"));
            header.set_size(11);
            header.set_cksum();

            builder
                .append(&header, &b"hello world"[..])
                .unwrap_or_else(|_| panic!("append failed"));
            builder.finish().unwrap_or_else(|_| panic!("finish failed"));
        }

        Mock::given(method("GET"))
            .and(path("/artifact.tar.gz"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(tar_gz_data))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));
        let url = format!("{}/artifact.tar.gz", mock_server.uri());

        let result = fetch_and_unpack(&client, &url, dest.path()).await;
        assert!(result.is_ok());

        let unpacked_file = std::fs::read_to_string(dest.path().join("hello.txt"))
            .unwrap_or_else(|_| panic!("read_to_string failed"));
        assert_eq!(unpacked_file, "hello world");
    }

    #[tokio::test]
    async fn test_fetch_and_unpack_zip() {
        let mock_server = MockServer::start().await;

        // Create a valid zip file in memory
        let mut zip_data = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_data));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("hello.txt", options)
                .unwrap_or_else(|_| panic!("start_file failed"));
            zip.write_all(b"hello world")
                .unwrap_or_else(|_| panic!("write_all failed"));
            zip.finish().unwrap_or_else(|_| panic!("finish failed"));
        }

        Mock::given(method("GET"))
            .and(path("/artifact.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(zip_data))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));
        let url = format!("{}/artifact.zip", mock_server.uri());

        let result = fetch_and_unpack(&client, &url, dest.path()).await;
        assert!(result.is_ok());

        let unpacked_file = std::fs::read_to_string(dest.path().join("hello.txt"))
            .unwrap_or_else(|_| panic!("read_to_string failed"));
        assert_eq!(unpacked_file, "hello world");
    }

    #[tokio::test]
    async fn test_fetch_and_unpack_http_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/artifact.zip"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let dest = TempDir::new().unwrap();
        let url = format!("{}/artifact.zip", mock_server.uri());

        let result = fetch_and_unpack(&client, &url, dest.path()).await;
        assert!(result.is_err());
    }
}
