//! Multi-target documentation publishing engine (`cdd-publisher` Requirement 7).
//!
//! Defines the `DocsPublisher` trait and `HttpAndGitDocsPublisher` implementation
//! supporting the Official CDD Hosted Doc Site (via `cdd-storage` + `cdd-docs-ui`),
//! GitHub Pages (`gh-pages`), `ReadTheDocs`, ecosystem documentation hosts (`docs.rs`,
//! `pkg.go.dev`, `javadoc.io`, `rubydoc.info`, Swift Package Index `DocC`), and custom URLs.

use crate::domain::{DocHostTarget, DocsPublishJob, RegistryTokens};
use crate::error::{PublisherError, Result};
use reqwest::Client;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Abstract interface for publishing generated SDK documentation to a configured [`DocHostTarget`].
pub trait DocsPublisher: Send + Sync {
    /// Publishes documentation for `job` using artifacts in `unpack_dir` and credentials in `tokens`.
    ///
    /// # Arguments
    ///
    /// * `job` - The [`DocsPublishJob`] describing the target host and package metadata.
    /// * `unpack_dir` - The root or language working directory containing generated docs/specs.
    /// * `tokens` - Configured [`RegistryTokens`] for authenticating with GitHub or `ReadTheDocs`.
    ///
    /// # Returns
    ///
    /// The canonical URL string where the published documentation can be viewed.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if required credentials or URLs are missing, or if an HTTP/Git
    /// upload operation fails.
    fn publish_docs(
        &self,
        job: &DocsPublishJob,
        unpack_dir: &Path,
        tokens: &RegistryTokens,
    ) -> impl Future<Output = Result<String>> + Send;
}

/// HTTP and Git-backed implementation of [`DocsPublisher`].
#[derive(Debug, Clone)]
pub struct HttpAndGitDocsPublisher {
    /// HTTP client for `cdd-storage`, `ReadTheDocs`, and custom webhook uploads.
    client: Client,
    /// Base URL for `cdd-storage` (e.g., `"http://127.0.0.1:8081"`).
    storage_base_url: String,
    /// Base URL for `cdd-docs-ui` (e.g., `"https://docs.cdd.dev"`).
    docs_ui_base_url: String,
    /// Base URL for `ReadTheDocs` API (e.g., `"https://readthedocs.org"`).
    readthedocs_api_url: String,
    /// Executable path for `git`.
    git_exe: String,
}

impl Default for HttpAndGitDocsPublisher {
    fn default() -> Self {
        Self::new(
            Client::new(),
            "http://127.0.0.1:8081",
            "https://docs.cdd.dev",
        )
    }
}

impl HttpAndGitDocsPublisher {
    /// Creates a new [`HttpAndGitDocsPublisher`] with custom storage and docs UI base URLs.
    ///
    /// # Arguments
    ///
    /// * `client` - The [`reqwest::Client`] instance.
    /// * `storage_base_url` - Base URL of `cdd-storage`.
    /// * `docs_ui_base_url` - Base URL of `cdd-docs-ui`.
    ///
    /// # Returns
    ///
    /// A configured [`HttpAndGitDocsPublisher`].
    #[must_use]
    pub fn new(
        client: Client,
        storage_base_url: impl Into<String>,
        docs_ui_base_url: impl Into<String>,
    ) -> Self {
        Self {
            client,
            storage_base_url: storage_base_url.into().trim_end_matches('/').to_string(),
            docs_ui_base_url: docs_ui_base_url.into().trim_end_matches('/').to_string(),
            readthedocs_api_url: "https://readthedocs.org".to_string(),
            git_exe: "git".to_string(),
        }
    }

    /// Overrides the `ReadTheDocs` API base URL and `git` executable path (useful for testing).
    ///
    /// # Arguments
    ///
    /// * `readthedocs_api_url` - Custom `ReadTheDocs` API base URL.
    /// * `git_exe` - Custom `git` executable path.
    ///
    /// # Returns
    ///
    /// The updated [`HttpAndGitDocsPublisher`].
    #[must_use]
    pub fn with_endpoints_and_git(
        mut self,
        readthedocs_api_url: impl Into<String>,
        git_exe: impl Into<String>,
    ) -> Self {
        self.readthedocs_api_url = readthedocs_api_url.into().trim_end_matches('/').to_string();
        self.git_exe = git_exe.into();
        self
    }

    /// Collects doc files (`openapi.json`, `docs.json`, and files inside `docs/`) to upload to `cdd-storage`.
    ///
    /// # Arguments
    ///
    /// * `unpack_dir` - Directory to scan.
    ///
    /// # Returns
    ///
    /// A list of `(relative_name, file_bytes)` pairs.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::Io`] if reading a discovered file fails.
    fn collect_cdd_doc_assets(unpack_dir: &Path) -> Result<Vec<(String, Vec<u8>)>> {
        let mut assets = Vec::new();
        for candidate in ["openapi.json", "docs.json"] {
            let file_path = unpack_dir.join(candidate);
            if file_path.is_file() {
                assets.push((candidate.to_string(), fs::read(&file_path)?));
            }
        }

        let docs_subdir = unpack_dir.join("docs");
        if docs_subdir.is_dir() {
            let mut entries: Vec<(String, PathBuf)> = fs::read_dir(&docs_subdir)?
                .filter_map(std::result::Result::ok)
                .filter(|e| e.path().is_file())
                .filter_map(|e| {
                    e.file_name()
                        .into_string()
                        .ok()
                        .map(|name| (name, e.path()))
                })
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, path) in entries {
                assets.push((format!("docs/{name}"), fs::read(&path)?));
            }
        }

        if assets.is_empty() {
            assets.push((
                "openapi.json".to_string(),
                br#"{"openapi":"3.1.0"}"#.to_vec(),
            ));
        }
        Ok(assets)
    }

    /// Publishes documentation assets to `cdd-storage` and returns the `cdd-docs-ui` viewer URL.
    ///
    /// # Arguments
    ///
    /// * `job` - Documentation publish job metadata.
    /// * `unpack_dir` - Directory containing generated doc files.
    ///
    /// # Returns
    ///
    /// The `cdd-docs-ui` URL pointing to the uploaded specification.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if reading local files or uploading to `cdd-storage` fails.
    async fn publish_official_cdd_site(
        &self,
        job: &DocsPublishJob,
        unpack_dir: &Path,
    ) -> Result<String> {
        let assets = Self::collect_cdd_doc_assets(unpack_dir)?;
        for (rel_path, bytes) in assets {
            let upload_url = format!(
                "{}/upload/{}/{}/{}/{}",
                self.storage_base_url, job.org_id, job.repo_id, job.version, rel_path
            );
            let resp = self.client.post(&upload_url).body(bytes).send().await?;
            resp.error_for_status()?;
        }

        let spec_url = format!(
            "{}/artifacts/{}/{}/{}/openapi.json",
            self.storage_base_url, job.org_id, job.repo_id, job.version
        );
        Ok(format!("{}/?spec={spec_url}", self.docs_ui_base_url))
    }

    /// Pushes generated static documentation to the `gh-pages` branch on GitHub Pages.
    ///
    /// # Arguments
    ///
    /// * `job` - Documentation publish job metadata.
    /// * `unpack_dir` - Unpacked repository or docs directory.
    /// * `tokens` - Configured registry tokens (requires `github`).
    ///
    /// # Returns
    ///
    /// The published GitHub Pages URL (`https://{org}.github.io/{repo}/`).
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if `tokens.github` is missing or `git push` fails.
    async fn publish_github_pages(
        &self,
        job: &DocsPublishJob,
        unpack_dir: &Path,
        tokens: &RegistryTokens,
    ) -> Result<String> {
        let gh_token = tokens.github.as_ref().ok_or_else(|| {
            PublisherError::DocsPublish("GITHUB_TOKEN is required for GitHub Pages".to_string())
        })?;
        let org = job
            .github_org_name
            .as_ref()
            .map_or(job.org_id.as_str(), |o| o.as_str())
            .to_string();
        let repo = job
            .repo_name
            .as_ref()
            .map_or(job.repo_id.as_str(), |r| r.as_str())
            .to_string();
        let branch = job
            .github_pages_branch
            .as_deref()
            .unwrap_or("gh-pages")
            .to_string();

        let git_exe = self.git_exe.clone();
        let dir_buf = unpack_dir.to_path_buf();
        let tok_clone = gh_token.clone();

        tokio::task::spawn_blocking(move || {
            let output = Command::new(&git_exe)
                .arg("push")
                .arg("--force")
                .arg("origin")
                .arg(format!("HEAD:{branch}"))
                .env("GITHUB_TOKEN", tok_clone.expose_secret())
                .current_dir(&dir_buf)
                .output()?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let safe = tok_clone.redact_from(&stderr);
                return Err(PublisherError::DocsPublish(format!(
                    "github pages push failed: {safe}"
                )));
            }
            Ok(())
        })
        .await??;

        Ok(format!("https://{org}.github.io/{repo}/"))
    }

    /// Triggers a `ReadTheDocs` v3 webhook build for `job`.
    ///
    /// # Arguments
    ///
    /// * `job` - Documentation publish job metadata.
    /// * `tokens` - Configured registry tokens (requires `readthedocs`).
    ///
    /// # Returns
    ///
    /// The `ReadTheDocs` documentation URL.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if `tokens.readthedocs` is missing or the webhook request fails.
    async fn publish_readthedocs(
        &self,
        job: &DocsPublishJob,
        tokens: &RegistryTokens,
    ) -> Result<String> {
        let rtd_token = tokens.readthedocs.as_ref().ok_or_else(|| {
            PublisherError::DocsPublish("READTHEDOCS_TOKEN is required for ReadTheDocs".to_string())
        })?;
        let slug = job
            .repo_name
            .as_ref()
            .map_or(job.repo_id.as_str(), |r| r.as_str());
        let endpoint = format!(
            "{}/api/v3/projects/{slug}/versions/{}/builds/",
            self.readthedocs_api_url, job.version
        );

        let auth = format!("Token {}", rtd_token.expose_secret());
        let resp = self
            .client
            .post(&endpoint)
            .header("Authorization", auth)
            .send()
            .await?;
        resp.error_for_status()?;

        Ok(format!("https://{slug}.readthedocs.io/en/{}/", job.version))
    }
}

impl DocsPublisher for HttpAndGitDocsPublisher {
    async fn publish_docs(
        &self,
        job: &DocsPublishJob,
        unpack_dir: &Path,
        tokens: &RegistryTokens,
    ) -> Result<String> {
        let org = job
            .github_org_name
            .as_ref()
            .map_or(job.org_id.as_str(), |o| o.as_str());
        let repo = job
            .repo_name
            .as_ref()
            .map_or(job.repo_id.as_str(), |r| r.as_str());

        match job.target {
            DocHostTarget::OfficialCddSite => self.publish_official_cdd_site(job, unpack_dir).await,
            DocHostTarget::GithubPages => self.publish_github_pages(job, unpack_dir, tokens).await,
            DocHostTarget::ReadTheDocs => self.publish_readthedocs(job, tokens).await,
            DocHostTarget::DocsRs => Ok(format!("https://docs.rs/{repo}/{}", job.version)),
            DocHostTarget::PkgGoDev => Ok(format!(
                "https://pkg.go.dev/github.com/{org}/{repo}@v{}",
                job.version
            )),
            DocHostTarget::JavadocIo => Ok(format!(
                "https://javadoc.io/doc/{org}/{repo}/{}",
                job.version
            )),
            DocHostTarget::RubyDoc => Ok(format!(
                "https://www.rubydoc.info/gems/{repo}/{}",
                job.version
            )),
            DocHostTarget::SwiftPackageIndexDocs => Ok(format!(
                "https://swiftpackageindex.com/{org}/{repo}/{}/documentation",
                job.version
            )),
            DocHostTarget::CustomUrl => {
                let url = job.custom_url.as_ref().ok_or_else(|| {
                    PublisherError::DocsPublish(
                        "custom_url is required for DocHostTarget::CustomUrl".to_string(),
                    )
                })?;
                let resp = self.client.post(url).json(job).send().await?;
                resp.error_for_status()?;
                Ok(url.clone())
            }
            DocHostTarget::None => Ok(String::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{GitHubOrgName, RepoName, SecretToken};
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path_regex},
    };

    /// Creates a sample [`DocsPublishJob`] for `target`.
    fn make_docs_job(target: DocHostTarget) -> DocsPublishJob {
        DocsPublishJob {
            target,
            org_id: "acme".to_string(),
            repo_id: "acme-sdk".to_string(),
            version: "1.2.0".to_string(),
            github_org_name: Some(GitHubOrgName::new("acme")),
            repo_name: Some(RepoName::new("acme-sdk")),
            github_pages_branch: None,
            custom_url: None,
        }
    }

    #[tokio::test]
    async fn test_official_cdd_site_and_ecosystem_targets() -> Result<()> {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/upload/.*"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let default_pub = HttpAndGitDocsPublisher::default();
        assert_eq!(default_pub.docs_ui_base_url, "https://docs.cdd.dev");

        let publisher = HttpAndGitDocsPublisher::new(
            Client::new(),
            format!("{}/", mock_server.uri()),
            "https://docs.cdd.dev/",
        );

        let temp = TempDir::new()?;
        let tokens = RegistryTokens::default();

        // 1. OfficialCddSite with fallback openapi.json when unpack_dir is empty
        let job_official = make_docs_job(DocHostTarget::OfficialCddSite);
        let url1 = publisher
            .publish_docs(&job_official, temp.path(), &tokens)
            .await?;
        assert!(url1.starts_with("https://docs.cdd.dev/?spec="));

        // 2. OfficialCddSite with explicit openapi.json, docs.json, and docs/index.html
        fs::write(temp.path().join("openapi.json"), b"{}")?;
        fs::write(temp.path().join("docs.json"), b"{}")?;
        let docs_dir = temp.path().join("docs");
        fs::create_dir_all(&docs_dir)?;
        fs::write(docs_dir.join("index.html"), b"<html></html>")?;
        fs::write(docs_dir.join("api.html"), b"<html>api</html>")?;
        fs::create_dir_all(docs_dir.join("nested_dir"))?;
        let url2 = publisher
            .publish_docs(&job_official, temp.path(), &tokens)
            .await?;
        assert!(url2.contains("/artifacts/acme/acme-sdk/1.2.0/openapi.json"));

        // 3. Ecosystem canonical URLs & None
        let cases = [
            (DocHostTarget::DocsRs, "https://docs.rs/acme-sdk/1.2.0"),
            (
                DocHostTarget::PkgGoDev,
                "https://pkg.go.dev/github.com/acme/acme-sdk@v1.2.0",
            ),
            (
                DocHostTarget::JavadocIo,
                "https://javadoc.io/doc/acme/acme-sdk/1.2.0",
            ),
            (
                DocHostTarget::RubyDoc,
                "https://www.rubydoc.info/gems/acme-sdk/1.2.0",
            ),
            (
                DocHostTarget::SwiftPackageIndexDocs,
                "https://swiftpackageindex.com/acme/acme-sdk/1.2.0/documentation",
            ),
            (DocHostTarget::None, ""),
        ];

        for (target, expected_url) in cases {
            let mut job = make_docs_job(target);
            job.github_org_name = None;
            job.repo_name = None;
            let res = publisher.publish_docs(&job, temp.path(), &tokens).await?;
            assert_eq!(res, expected_url);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_github_pages_readthedocs_and_custom_url() -> Result<()> {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let temp = TempDir::new()?;
        let git_ok = temp.path().join("git_ok");
        fs::write(&git_ok, "#!/bin/sh\nexit 0\n")?;
        fs::set_permissions(&git_ok, fs::Permissions::from_mode(0o755))?;

        let git_fail = temp.path().join("git_fail");
        let fail_script = "#!/bin/sh\necho 'Push failed gh-secret-1' >&2\nexit 1\n";
        fs::write(&git_fail, fail_script)?;
        fs::set_permissions(&git_fail, fs::Permissions::from_mode(0o755))?;

        let publisher =
            HttpAndGitDocsPublisher::new(Client::new(), mock_server.uri(), "https://docs.cdd.dev")
                .with_endpoints_and_git(mock_server.uri(), git_ok.to_string_lossy());

        let empty_tokens = RegistryTokens::default();
        let tokens = RegistryTokens {
            github: Some(SecretToken::new("gh-secret-1")),
            readthedocs: Some(SecretToken::new("rtd-secret-2")),
            ..RegistryTokens::default()
        };

        // 1. GithubPages missing token error vs success vs git failure with redaction
        let gh_job = make_docs_job(DocHostTarget::GithubPages);
        assert!(
            publisher
                .publish_docs(&gh_job, temp.path(), &empty_tokens)
                .await
                .is_err()
        );
        let gh_url = publisher
            .publish_docs(&gh_job, temp.path(), &tokens)
            .await?;
        assert_eq!(gh_url, "https://acme.github.io/acme-sdk/");

        let fail_pub = publisher
            .clone()
            .with_endpoints_and_git(mock_server.uri(), git_fail.to_string_lossy());
        let gh_err = fail_pub
            .publish_docs(&gh_job, temp.path(), &tokens)
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(gh_err.contains("github pages push failed"));
        assert!(!gh_err.contains("gh-secret-1"));

        // 2. ReadTheDocs missing token error vs success
        let rtd_job = make_docs_job(DocHostTarget::ReadTheDocs);
        assert!(
            publisher
                .publish_docs(&rtd_job, temp.path(), &empty_tokens)
                .await
                .is_err()
        );
        let rtd_url = publisher
            .publish_docs(&rtd_job, temp.path(), &tokens)
            .await?;
        assert_eq!(rtd_url, "https://acme-sdk.readthedocs.io/en/1.2.0/");

        // 3. CustomUrl missing custom_url error vs success
        let mut custom_job = make_docs_job(DocHostTarget::CustomUrl);
        assert!(
            publisher
                .publish_docs(&custom_job, temp.path(), &tokens)
                .await
                .is_err()
        );
        let target_url = format!("{}/custom-docs-hook", mock_server.uri());
        custom_job.custom_url = Some(target_url.clone());
        let custom_res = publisher
            .publish_docs(&custom_job, temp.path(), &tokens)
            .await?;
        assert_eq!(custom_res, target_url);

        Ok(())
    }
}
