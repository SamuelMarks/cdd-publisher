//! Module for fetching and unpacking generated artifacts.
//!
//! Provides the [`ArtifactStorage`] trait for dependency injection and
//! [`HttpArtifactStorage`] for downloading and safely extracting `.zip` and `.tar.gz`/`.tgz`
//! bundles without blocking the async runtime or permitting Zip-Slip path traversal.

use crate::domain::{ArchiveFormat, ArtifactUrl};
use crate::error::{PublisherError, Result};
use reqwest::Client;
use std::fs;
use std::future::Future;
use std::io::Cursor;
use std::path::{Component, Path};

/// Abstract interface for fetching and unpacking artifact bundles.
pub trait ArtifactStorage: Send + Sync {
    /// Downloads the artifact from `url` and extracts its contents into `dest_dir`.
    ///
    /// # Arguments
    ///
    /// * `url` - The strongly-typed [`ArtifactUrl`] pointing to a `.zip`, `.tar.gz`, or `.tgz` archive.
    /// * `dest_dir` - The destination directory where archive entries will be unpacked.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if the archive format is unsupported, the HTTP download fails,
    /// an archive entry contains a path traversal (`..` / absolute path), or extraction fails.
    fn fetch_and_unpack(
        &self,
        url: &ArtifactUrl,
        dest_dir: &Path,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// HTTP-backed implementation of [`ArtifactStorage`].
#[derive(Debug, Clone, Default)]
pub struct HttpArtifactStorage {
    /// Underlying HTTP client used to fetch archives.
    client: Client,
}

impl HttpArtifactStorage {
    /// Creates a new [`HttpArtifactStorage`] with the given [`reqwest::Client`].
    ///
    /// # Arguments
    ///
    /// * `client` - The HTTP client to use for downloading artifacts.
    #[must_use]
    pub const fn new(client: Client) -> Self {
        Self { client }
    }
}

impl ArtifactStorage for HttpArtifactStorage {
    async fn fetch_and_unpack(&self, url: &ArtifactUrl, dest_dir: &Path) -> Result<()> {
        fetch_and_unpack(&self.client, url.as_str(), dest_dir).await
    }
}

/// Validates that a relative path inside an archive does not contain parent directory (`..`),
/// root (`/`), or prefix components (Zip-Slip protection).
///
/// # Arguments
///
/// * `entry_path` - The relative path of the archive entry.
///
/// # Errors
///
/// Returns [`PublisherError::UnsafeArchivePath`] if the path is empty or attempts traversal.
fn validate_safe_relative_path(entry_path: &Path) -> Result<()> {
    let mut has_normal = false;
    for component in entry_path.components() {
        match component {
            Component::Normal(_) => has_normal = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(PublisherError::UnsafeArchivePath {
                    entry_path: entry_path.to_string_lossy().into_owned(),
                });
            }
        }
    }
    if !has_normal {
        return Err(PublisherError::UnsafeArchivePath {
            entry_path: entry_path.to_string_lossy().into_owned(),
        });
    }
    Ok(())
}

/// Unpacks archive bytes in memory into `dest_dir` according to `format`.
///
/// Runs synchronously and is designed to be invoked inside `tokio::task::spawn_blocking`.
///
/// # Arguments
///
/// * `bytes` - Raw archive bytes.
/// * `format` - The detected [`ArchiveFormat`].
/// * `dest_dir` - Target directory on disk.
///
/// # Errors
///
/// Returns [`PublisherError`] if archive parsing, path validation, or file extraction fails.
fn unpack_bytes(bytes: &[u8], format: ArchiveFormat, dest_dir: &Path) -> Result<()> {
    match format {
        ArchiveFormat::Zip => {
            let reader = Cursor::new(bytes);
            let mut archive = zip::ZipArchive::new(reader)?;
            for i in 0..archive.len() {
                let mut file = archive.by_index(i)?;
                let raw_name = file.name()?.into_owned();
                let entry_path = Path::new(&raw_name);
                validate_safe_relative_path(entry_path)?;

                let outpath = dest_dir.join(entry_path);
                if raw_name.ends_with('/') || file.is_dir() {
                    fs::create_dir_all(&outpath)?;
                } else {
                    fs::create_dir_all(outpath.parent().unwrap_or(dest_dir))?;
                    let mut outfile = fs::File::create(&outpath)?;
                    std::io::copy(&mut file, &mut outfile)?;
                }
            }
            Ok(())
        }
        ArchiveFormat::TarGz => {
            let reader = Cursor::new(bytes);
            let tar = flate2::read::GzDecoder::new(reader);
            let mut archive = tar::Archive::new(tar);
            for entry_result in archive.entries()? {
                let mut entry = entry_result?;
                let path_cow = entry.path()?;
                validate_safe_relative_path(&path_cow)?;
                entry.unpack_in(dest_dir)?;
            }
            Ok(())
        }
    }
}

/// Fetches an artifact from the given URL and unpacks it to the specified destination directory.
///
/// Supports both `.zip` and `.tar.gz`/`.tgz` formats (case-insensitively) and validates all
/// archive entry paths against Zip-Slip traversal before extracting on a blocking thread pool.
///
/// # Arguments
///
/// * `client` - The [`reqwest::Client`] used to download the archive.
/// * `url` - The URL string of the artifact archive.
/// * `dest_dir` - The destination directory where the archive will be extracted.
///
/// # Errors
///
/// Returns an error if the archive format is unsupported, the HTTP request fails,
/// an archive path is unsafe, or extraction fails.
pub async fn fetch_and_unpack(client: &Client, url: &str, dest_dir: &Path) -> Result<()> {
    let format = ArchiveFormat::from_url_or_path(url)?;

    let response = client.get(url).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;
    let dest_buf = dest_dir.to_path_buf();

    tokio::task::spawn_blocking(move || unpack_bytes(&bytes, format, &dest_buf)).await?
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
    async fn test_fetch_and_unpack_unsupported_format() -> std::io::Result<()> {
        let client = Client::new();
        let dest = TempDir::new()?;

        let result = fetch_and_unpack(&client, "http://localhost/file.txt", dest.path()).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Unsupported artifact format"));
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_and_unpack_tar_gz_and_http_storage_trait() -> Result<()> {
        let mock_server = MockServer::start().await;

        let mut tar_gz_data = Vec::new();
        {
            let encoder =
                flate2::write::GzEncoder::new(&mut tar_gz_data, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);

            let mut header = tar::Header::new_gnu();
            header.set_path("nested/hello.txt")?;
            header.set_size(11);
            header.set_mode(0o644);
            header.set_cksum();

            builder.append(&header, &b"hello world"[..])?;
            let encoder = builder.into_inner()?;
            let _ = encoder.finish()?;
        }

        Mock::given(method("GET"))
            .and(path("/artifact.TAR.GZ"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(tar_gz_data))
            .mount(&mock_server)
            .await;

        let storage = HttpArtifactStorage::new(Client::new());
        let dest = TempDir::new()?;
        let url = ArtifactUrl::new(format!("{}/artifact.TAR.GZ", mock_server.uri()));

        storage.fetch_and_unpack(&url, dest.path()).await?;

        let unpacked_file = fs::read_to_string(dest.path().join("nested/hello.txt"))?;
        assert_eq!(unpacked_file, "hello world");
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_and_unpack_zip_with_dir_and_nested_files() -> Result<()> {
        let mock_server = MockServer::start().await;

        let mut zip_data = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut zip_data));
            let options = zip::write::SimpleFileOptions::default();
            zip.add_directory("emptydir/", options)?;
            zip.start_file("sub/hello.txt", options)?;
            zip.write_all(b"hello world")?;
            zip.finish()?;
        }

        Mock::given(method("GET"))
            .and(path("/artifact.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(zip_data))
            .mount(&mock_server)
            .await;

        let storage = HttpArtifactStorage::default();
        let dest = TempDir::new()?;
        let url = ArtifactUrl::new(format!("{}/artifact.zip", mock_server.uri()));

        storage.fetch_and_unpack(&url, dest.path()).await?;

        assert!(dest.path().join("emptydir").is_dir());
        let unpacked_file = fs::read_to_string(dest.path().join("sub/hello.txt"))?;
        assert_eq!(unpacked_file, "hello world");
        Ok(())
    }

    #[test]
    fn test_validate_safe_relative_path_rejects_traversal_and_root() {
        assert!(validate_safe_relative_path(Path::new("./valid/file.txt")).is_ok());
        assert!(matches!(
            validate_safe_relative_path(Path::new("../evil.txt")),
            Err(PublisherError::UnsafeArchivePath { .. })
        ));
        assert!(matches!(
            validate_safe_relative_path(Path::new("sub/../../evil.txt")),
            Err(PublisherError::UnsafeArchivePath { .. })
        ));
        assert!(matches!(
            validate_safe_relative_path(Path::new("/etc/passwd")),
            Err(PublisherError::UnsafeArchivePath { .. })
        ));
        assert!(matches!(
            validate_safe_relative_path(Path::new(".")),
            Err(PublisherError::UnsafeArchivePath { .. })
        ));
    }

    #[test]
    fn test_unpack_bytes_zip_rejects_traversal() -> std::io::Result<()> {
        let mut zip_data = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut zip_data));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("../escape.txt", options)?;
            zip.write_all(b"malicious")?;
            zip.finish()?;
        }
        let dest = TempDir::new()?;
        let res = unpack_bytes(&zip_data, ArchiveFormat::Zip, dest.path());
        assert!(matches!(res, Err(PublisherError::UnsafeArchivePath { .. })));
        Ok(())
    }

    #[tokio::test]
    async fn test_fetch_and_unpack_http_error() -> std::io::Result<()> {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/artifact.zip"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = Client::new();
        let dest = TempDir::new()?;
        let url = format!("{}/artifact.zip", mock_server.uri());

        let result = fetch_and_unpack(&client, &url, dest.path()).await;
        assert!(result.is_err());
        Ok(())
    }
}
