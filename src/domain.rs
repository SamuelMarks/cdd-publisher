//! Strong domain primitives and value objects for `cdd-publisher`.
//!
//! This module eliminates stringly-typed primitive obsession by encapsulating
//! registries, secret tokens, archive formats, artifact URLs, repository topologies,
//! PGP signing configurations, documentation hosting targets, and stream configuration
//! in validated, strongly-typed domain structures.

use crate::error::{PublisherError, Result};
use derive_more::{AsRef, Display, From, Into};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

/// Validates that a relative path inside an archive or repository subdirectory does not contain
/// parent directory (`..`), root (`/`), or prefix components (Zip-Slip / traversal protection).
///
/// # Arguments
///
/// * `entry_path` - The relative path to validate.
///
/// # Returns
///
/// Returns `Ok(())` if `entry_path` contains at least one normal path component and no traversal components.
///
/// # Errors
///
/// Returns [`PublisherError::UnsafeArchivePath`] if the path is empty, `.` only, or attempts traversal.
pub fn validate_safe_relative_path(entry_path: &Path) -> Result<()> {
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

/// Supported external package registries across all 13 CDD target language ecosystems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "lowercase")]
pub enum Registry {
    /// The Node Package Manager (`npm`) registry (TypeScript / JavaScript).
    #[display("npm")]
    Npm,
    /// The Python Package Index (`pypi`) registry (Python).
    #[display("pypi")]
    PyPi,
    /// The Rust Crates (`cargo` / `crates.io`) registry (Rust).
    #[serde(alias = "crates")]
    #[display("cargo")]
    Cargo,
    /// Maven Central / Sonatype (`maven`) registry (Java & Kotlin).
    #[display("maven")]
    Maven,
    /// The `NuGet` (`nuget.org`) registry (C#).
    #[display("nuget")]
    NuGet,
    /// The `RubyGems` (`rubygems.org`) registry (Ruby).
    #[serde(alias = "gems")]
    #[display("rubygems")]
    RubyGems,
    /// The `Packagist` (`packagist.org`) registry (PHP).
    #[serde(alias = "composer")]
    #[display("packagist")]
    Packagist,
    /// The Go Package Index (`pkg.go.dev` / `proxy.golang.org`) registry (Go).
    #[serde(alias = "go")]
    #[display("pkggodev")]
    PkgGoDev,
    /// The Swift Package Index (`swift` / `spm`) registry (Swift).
    #[serde(alias = "spm")]
    #[display("swift")]
    SwiftPackageIndex,
    /// The Microsoft `vcpkg` port registry (C & C++).
    #[display("vcpkg")]
    Vcpkg,
    /// The `Conan` Center / Artifactory remote registry (C & C++).
    #[display("conan")]
    Conan,
    /// The `Homebrew` formula tap (`homebrew` / `brew`) registry (C, C++, Shell, Go, Rust, Swift).
    #[serde(alias = "brew")]
    #[display("homebrew")]
    Homebrew,
    /// Universal GitHub Releases (`github_releases`) binary/tarball + `.asc` publisher.
    #[serde(rename = "github_releases", alias = "github")]
    #[display("github_releases")]
    GithubReleases,
}

impl Registry {
    /// Returns the canonical lowercase string slice representation of the registry.
    ///
    /// # Returns
    ///
    /// A `'static` string slice identifying the registry.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::PyPi => "pypi",
            Self::Cargo => "cargo",
            Self::Maven => "maven",
            Self::NuGet => "nuget",
            Self::RubyGems => "rubygems",
            Self::Packagist => "packagist",
            Self::PkgGoDev => "pkggodev",
            Self::SwiftPackageIndex => "swift",
            Self::Vcpkg => "vcpkg",
            Self::Conan => "conan",
            Self::Homebrew => "homebrew",
            Self::GithubReleases => "github_releases",
        }
    }

    /// Returns the environment variable name used for this registry's primary authentication token.
    ///
    /// # Returns
    ///
    /// The canonical environment variable name expected for authenticating against `self`.
    #[must_use]
    pub const fn token_env_var(self) -> &'static str {
        match self {
            Self::Npm => "NPM_TOKEN",
            Self::PyPi => "TWINE_PASSWORD",
            Self::Cargo => "CARGO_REGISTRY_TOKEN",
            Self::Maven => "MAVEN_PASSWORD",
            Self::NuGet => "NUGET_API_KEY",
            Self::RubyGems => "GEM_HOST_API_KEY",
            Self::Packagist => "PACKAGIST_TOKEN",
            Self::PkgGoDev | Self::SwiftPackageIndex | Self::GithubReleases => "GITHUB_TOKEN",
            Self::Vcpkg => "VCPKG_REPO_URL",
            Self::Conan => "CONAN_PASSWORD",
            Self::Homebrew => "HOMEBREW_TAP_REPO",
        }
    }
}

impl FromStr for Registry {
    type Err = PublisherError;

    /// Parses a string slice into a [`Registry`].
    ///
    /// # Arguments
    ///
    /// * `s` - The string identifier of the registry (case-insensitive, supporting ecosystem aliases).
    ///
    /// # Returns
    ///
    /// The parsed [`Registry`] variant.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::UnsupportedRegistry`] if `s` does not match a known registry.
    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "npm" => Ok(Self::Npm),
            "pypi" => Ok(Self::PyPi),
            "cargo" | "crates" => Ok(Self::Cargo),
            "maven" => Ok(Self::Maven),
            "nuget" => Ok(Self::NuGet),
            "rubygems" | "gems" => Ok(Self::RubyGems),
            "packagist" | "composer" => Ok(Self::Packagist),
            "pkggodev" | "go" => Ok(Self::PkgGoDev),
            "swift" | "spm" => Ok(Self::SwiftPackageIndex),
            "vcpkg" => Ok(Self::Vcpkg),
            "conan" => Ok(Self::Conan),
            "homebrew" | "brew" => Ok(Self::Homebrew),
            "github_releases" | "github" => Ok(Self::GithubReleases),
            _ => Err(PublisherError::UnsupportedRegistry {
                registry: s.to_string(),
            }),
        }
    }
}

/// A secret authentication token, private key, or passphrase that redacts itself in `Debug` and `Display` output.
///
/// Wrapping registry tokens and PGP secrets in `SecretToken` guarantees that secrets are never
/// accidentally written to standard output or error logs via formatting traits, and zeroes
/// the underlying buffer when dropped.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretToken(String);

impl SecretToken {
    /// Replacement placeholder used when redacting secrets from logs or output streams.
    pub const REDACTED_PLACEHOLDER: &'static str = "***";

    /// Creates a new [`SecretToken`] from any string-like value.
    ///
    /// # Arguments
    ///
    /// * `value` - The raw secret token string.
    ///
    /// # Returns
    ///
    /// A new [`SecretToken`] wrapping `value`.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Explicitly exposes the underlying secret string slice for authentication operations.
    ///
    /// # Returns
    ///
    /// The unredacted secret string slice.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    /// Scrubs all occurrences of the secret token from the provided text,
    /// replacing them with [`SecretToken::REDACTED_PLACEHOLDER`].
    ///
    /// If the secret token is empty, returns the input text unchanged.
    ///
    /// # Arguments
    ///
    /// * `input` - The diagnostic or stderr text that may contain the raw secret.
    ///
    /// # Returns
    ///
    /// A sanitized `String` with all occurrences of the secret replaced by `"***"`.
    #[must_use]
    pub fn redact_from(&self, input: &str) -> String {
        if self.0.is_empty() {
            return input.to_string();
        }
        input.replace(&self.0, Self::REDACTED_PLACEHOLDER)
    }
}

impl fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretToken([REDACTED])")
    }
}

impl fmt::Display for SecretToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl From<String> for SecretToken {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SecretToken {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl Drop for SecretToken {
    fn drop(&mut self) {
        // Overwrite the secret bytes in memory prior to deallocation.
        // SAFETY: Replacing valid UTF-8 bytes with ASCII 0x00 ('\0') preserves valid UTF-8.
        unsafe {
            let bytes = self.0.as_bytes_mut();
            std::ptr::write_bytes(bytes.as_mut_ptr(), 0, bytes.len());
        }
    }
}

/// Authentication tokens configured for all 13 supported registries and documentation hosts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegistryTokens {
    /// Token for the NPM registry (`NPM_TOKEN`).
    pub npm: Option<SecretToken>,
    /// Token for the `PyPI` registry (`TWINE_PASSWORD`).
    pub pypi: Option<SecretToken>,
    /// Token for the Cargo registry (`CARGO_REGISTRY_TOKEN`).
    pub cargo: Option<SecretToken>,
    /// Username for Maven Central / Sonatype (`MAVEN_USERNAME`).
    pub maven_username: Option<SecretToken>,
    /// Password or token for Maven Central / Sonatype (`MAVEN_PASSWORD`).
    pub maven_password: Option<SecretToken>,
    /// API key for `NuGet.org` (`NUGET_API_KEY`).
    pub nuget: Option<SecretToken>,
    /// API key for `RubyGems.org` (`GEM_HOST_API_KEY`).
    pub rubygems: Option<SecretToken>,
    /// API token for `Packagist.org` (`PACKAGIST_TOKEN`).
    pub packagist: Option<SecretToken>,
    /// Token for GitHub API, Releases, Go/Swift tag pushes, and GitHub Pages (`GITHUB_TOKEN`).
    pub github: Option<SecretToken>,
    /// Password or token for Conan remote (`CONAN_PASSWORD`).
    pub conan: Option<SecretToken>,
    /// Repository URL or token for `vcpkg` port registry (`VCPKG_REPO_URL`).
    pub vcpkg_repo_url: Option<SecretToken>,
    /// Tap repository identifier or token for `Homebrew` (`HOMEBREW_TAP_REPO`).
    pub homebrew_tap_repo: Option<SecretToken>,
    /// Webhook API token for `ReadTheDocs` (`READTHEDOCS_TOKEN`).
    pub readthedocs: Option<SecretToken>,
}

impl RegistryTokens {
    /// Retrieves the configured [`SecretToken`] for the given [`Registry`].
    ///
    /// # Arguments
    ///
    /// * `registry` - The target registry requiring authentication.
    ///
    /// # Returns
    ///
    /// A reference to the [`SecretToken`] for `registry`.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::MissingRegistryToken`] if the token for `registry` is `None`.
    pub fn token_for(&self, registry: Registry) -> Result<&SecretToken> {
        let maybe_token = match registry {
            Registry::Npm => self.npm.as_ref(),
            Registry::PyPi => self.pypi.as_ref(),
            Registry::Cargo => self.cargo.as_ref(),
            Registry::Maven => self.maven_password.as_ref(),
            Registry::NuGet => self.nuget.as_ref(),
            Registry::RubyGems => self.rubygems.as_ref(),
            Registry::Packagist => self.packagist.as_ref(),
            Registry::PkgGoDev | Registry::SwiftPackageIndex | Registry::GithubReleases => {
                self.github.as_ref()
            }
            Registry::Vcpkg => self.vcpkg_repo_url.as_ref().or(self.github.as_ref()),
            Registry::Conan => self.conan.as_ref(),
            Registry::Homebrew => self.homebrew_tap_repo.as_ref().or(self.github.as_ref()),
        };
        maybe_token.ok_or(PublisherError::MissingRegistryToken { registry })
    }
}

/// Repository layout topology mode (Requirement 2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "lowercase")]
pub enum RepoTopologyMode {
    /// Single repository (`${GITHUB_ORG_NAME}-sdk`) containing `<lang>/` subdirectories.
    #[display("monorepo")]
    Monorepo,
    /// Standalone language repository (`${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`).
    #[default]
    #[display("polyrepo")]
    Polyrepo,
}

/// Validated GitHub organization or owner name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct GitHubOrgName(String);

impl GitHubOrgName {
    /// Creates a new [`GitHubOrgName`] without validation.
    ///
    /// # Arguments
    ///
    /// * `name` - The GitHub organization name.
    ///
    /// # Returns
    ///
    /// A new [`GitHubOrgName`].
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Validates and constructs a [`GitHubOrgName`].
    ///
    /// # Arguments
    ///
    /// * `name` - Candidate GitHub organization name.
    ///
    /// # Returns
    ///
    /// A validated [`GitHubOrgName`].
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::InvalidDomainValue`] if `name` is empty or contains path separators.
    pub fn try_new(name: impl Into<String>) -> Result<Self> {
        let raw = name.into();
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains("..") {
            return Err(PublisherError::InvalidDomainValue {
                field: "GitHubOrgName",
                reason: "must be a non-empty identifier without path separators".to_string(),
            });
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Returns the organization name as a string slice.
    ///
    /// # Returns
    ///
    /// The underlying `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for GitHubOrgName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Validated GitHub repository name (`${GITHUB_ORG_NAME}-sdk` or `${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct RepoName(String);

impl RepoName {
    /// Creates a new [`RepoName`].
    ///
    /// # Arguments
    ///
    /// * `name` - The repository name.
    ///
    /// # Returns
    ///
    /// A new [`RepoName`].
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Validates and constructs a [`RepoName`].
    ///
    /// # Arguments
    ///
    /// * `name` - Candidate repository name.
    ///
    /// # Returns
    ///
    /// A validated [`RepoName`].
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::InvalidDomainValue`] if `name` is empty or contains path separators.
    pub fn try_new(name: impl Into<String>) -> Result<Self> {
        let raw = name.into();
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.contains('/') || trimmed.contains("..") {
            return Err(PublisherError::InvalidDomainValue {
                field: "RepoName",
                reason: "must be a non-empty repository identifier without path separators"
                    .to_string(),
            });
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Constructs the default monorepo name `${GITHUB_ORG_NAME}-sdk`.
    ///
    /// # Arguments
    ///
    /// * `org` - The [`GitHubOrgName`].
    ///
    /// # Returns
    ///
    /// A [`RepoName`] formatted as `${org}-sdk`.
    #[must_use]
    pub fn default_monorepo(org: &GitHubOrgName) -> Self {
        Self(format!("{}-sdk", org.as_str()))
    }

    /// Constructs the default polyrepo name `${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`.
    ///
    /// # Arguments
    ///
    /// * `org` - The [`GitHubOrgName`].
    /// * `lang` - The target language identifier (e.g., `"rust"`, `"python"`).
    ///
    /// # Returns
    ///
    /// A [`RepoName`] formatted as `${org}-sdk-${lang}`.
    #[must_use]
    pub fn default_polyrepo(org: &GitHubOrgName, lang: &str) -> Self {
        Self(format!(
            "{}-sdk-{}",
            org.as_str(),
            lang.to_ascii_lowercase()
        ))
    }

    /// Returns the repository name as a string slice.
    ///
    /// # Returns
    ///
    /// The underlying `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for RepoName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Validated hexadecimal PGP key fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, Into)]
#[serde(transparent)]
pub struct PgpKeyFingerprint(String);

impl PgpKeyFingerprint {
    /// Validates and constructs a [`PgpKeyFingerprint`] from a hex string.
    ///
    /// Spaces are stripped and characters are normalized to uppercase hexadecimal.
    /// The normalized fingerprint must be between 8 and 64 hex characters.
    ///
    /// # Arguments
    ///
    /// * `fingerprint` - Candidate hexadecimal fingerprint string.
    ///
    /// # Returns
    ///
    /// A validated [`PgpKeyFingerprint`].
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::InvalidDomainValue`] if the string is not valid hexadecimal
    /// or has an invalid length.
    pub fn try_new(fingerprint: impl Into<String>) -> Result<Self> {
        let owned = fingerprint.into();
        Self::from_raw_str(&owned)
    }

    /// Validates a string slice as a [`PgpKeyFingerprint`].
    ///
    /// # Arguments
    ///
    /// * `raw` - Candidate hexadecimal fingerprint string slice.
    ///
    /// # Returns
    ///
    /// A validated [`PgpKeyFingerprint`].
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::InvalidDomainValue`] if the string is not 8..=64 hex chars.
    fn from_raw_str(raw: &str) -> Result<Self> {
        let normalized: String = raw
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let valid_len = (8..=64).contains(&normalized.len());
        if !valid_len || !normalized.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(PublisherError::InvalidDomainValue {
                field: "PgpKeyFingerprint",
                reason: "must be 8..=64 hexadecimal characters".to_string(),
            });
        }
        Ok(Self(normalized))
    }

    /// Returns the normalized uppercase hex fingerprint as a string slice.
    ///
    /// # Returns
    ///
    /// The underlying `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Ephemeral PGP/GPG signing configuration for package artifacts and Git tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PgpSigningConfig {
    /// ASCII-armored PGP private key block (`-----BEGIN PGP PRIVATE KEY BLOCK-----...`).
    pub armored_private_key: SecretToken,
    /// Optional passphrase unlocking the private key.
    #[serde(default)]
    pub passphrase: Option<SecretToken>,
    /// Optional specific key fingerprint to select when signing.
    #[serde(default)]
    pub key_fingerprint: Option<PgpKeyFingerprint>,
}

/// Supported documentation hosting targets (Requirement 7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
#[serde(rename_all = "snake_case")]
pub enum DocHostTarget {
    /// Official CDD Hosted Doc Site backed by `cdd-storage` and `cdd-docs-ui`.
    #[display("official_cdd_site")]
    OfficialCddSite,
    /// GitHub Pages (`gh-pages` branch deployment).
    #[display("github_pages")]
    GithubPages,
    /// `ReadTheDocs` webhook build trigger.
    #[display("read_the_docs")]
    ReadTheDocs,
    /// Rust `docs.rs` automatic crate documentation.
    #[display("docs_rs")]
    DocsRs,
    /// Go `pkg.go.dev` automatic module documentation.
    #[display("pkg_go_dev")]
    PkgGoDev,
    /// Java/Kotlin `javadoc.io` automatic Maven documentation.
    #[display("javadoc_io")]
    JavadocIo,
    /// Ruby `rubydoc.info` automatic gem documentation.
    #[display("ruby_doc")]
    RubyDoc,
    /// Swift Package Index hosted `DocC` documentation.
    #[display("swift_package_index_docs")]
    SwiftPackageIndexDocs,
    /// User-supplied custom documentation webhook or S3 URL.
    #[display("custom_url")]
    CustomUrl,
    /// Documentation publishing disabled for this job.
    #[default]
    #[display("none")]
    None,
}

/// Configuration for publishing SDK documentation alongside or independent of package release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsPublishJob {
    /// Target documentation hosting provider.
    pub target: DocHostTarget,
    /// Organization identifier for storage/URL routing.
    pub org_id: String,
    /// Repository or package identifier for storage/URL routing.
    pub repo_id: String,
    /// Semantic version string of the release (e.g., `"1.2.0"`).
    pub version: String,
    /// Optional GitHub organization name for GitHub Pages / ecosystem doc links.
    #[serde(default)]
    pub github_org_name: Option<GitHubOrgName>,
    /// Optional repository name for GitHub Pages / ecosystem doc links.
    #[serde(default)]
    pub repo_name: Option<RepoName>,
    /// Optional target branch for GitHub Pages (defaults to `"gh-pages"`).
    #[serde(default)]
    pub github_pages_branch: Option<String>,
    /// Optional custom webhook or base URL when `target` is [`DocHostTarget::CustomUrl`].
    #[serde(default)]
    pub custom_url: Option<String>,
}

/// Wire payload of a publish job read from the Redis stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishJob {
    /// The URL of the artifact archive to fetch and publish.
    #[serde(alias = "artifact_url")]
    pub artifact_id: ArtifactUrl,
    /// The target registry identifier (e.g., `"npm"`, `"pypi"`, `"cargo"`, `"maven"`, etc.).
    pub registry: String,
    /// Optional GitHub organization name (`${GITHUB_ORG_NAME}`).
    #[serde(default)]
    pub github_org_name: Option<GitHubOrgName>,
    /// Optional explicit repository name (`${GITHUB_ORG_NAME}-sdk` or `${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`).
    #[serde(default)]
    pub repo_name: Option<RepoName>,
    /// Repository topology mode (`monorepo` vs `polyrepo`, defaults to `polyrepo`).
    #[serde(default)]
    pub topology_mode: RepoTopologyMode,
    /// Optional subdirectory inside the unpacked archive when operating in `Monorepo` mode (e.g., `"rust"`).
    #[serde(default)]
    pub sub_directory: Option<String>,
    /// Optional PGP/GPG signing configuration for artifact and Git tag signing.
    #[serde(default)]
    pub pgp_config: Option<PgpSigningConfig>,
    /// Optional documentation publishing job to execute after package publishing.
    #[serde(default)]
    pub docs_job: Option<DocsPublishJob>,
}

impl PublishJob {
    /// Creates a minimal [`PublishJob`] for the given artifact URL and registry string.
    ///
    /// # Arguments
    ///
    /// * `artifact_id` - The artifact archive URL.
    /// * `registry` - The target registry identifier string.
    ///
    /// # Returns
    ///
    /// A [`PublishJob`] with default topology (`Polyrepo`) and no optional extensions.
    #[must_use]
    pub fn new(artifact_id: impl Into<ArtifactUrl>, registry: impl Into<String>) -> Self {
        Self {
            artifact_id: artifact_id.into(),
            registry: registry.into(),
            github_org_name: None,
            repo_name: None,
            topology_mode: RepoTopologyMode::Polyrepo,
            sub_directory: None,
            pgp_config: None,
            docs_job: None,
        }
    }

    /// Parses the job's `registry` string into a strongly-typed [`Registry`] enum.
    ///
    /// # Returns
    ///
    /// The validated [`Registry`] enum variant.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::UnsupportedRegistry`] if `self.registry` is not recognized.
    pub fn parsed_registry(&self) -> Result<Registry> {
        Registry::from_str(&self.registry)
    }

    /// Computes the effective [`RepoName`] for this job based on explicit `repo_name`,
    /// `github_org_name`, and `topology_mode`.
    ///
    /// # Returns
    ///
    /// `Some(RepoName)` if either `repo_name` or `github_org_name` is present, or `None` otherwise.
    #[must_use]
    pub fn effective_repo_name(&self) -> Option<RepoName> {
        if let Some(explicit) = &self.repo_name {
            return Some(explicit.clone());
        }
        let org = self.github_org_name.as_ref()?;
        match self.topology_mode {
            RepoTopologyMode::Monorepo => Some(RepoName::default_monorepo(org)),
            RepoTopologyMode::Polyrepo => {
                let lang = self.sub_directory.as_deref().unwrap_or(&self.registry);
                Some(RepoName::default_polyrepo(org, lang))
            }
        }
    }

    /// Resolves the working directory inside `unpack_root` where package manager commands should run.
    ///
    /// In [`RepoTopologyMode::Polyrepo`] mode (or when `sub_directory` is `None`), returns `unpack_root`.
    /// In [`RepoTopologyMode::Monorepo`] mode with `Some(sub_dir)`, validates that `sub_dir` is a safe
    /// relative path (rejecting `..` or `/`) and returns `unpack_root.join(sub_dir)`.
    ///
    /// # Arguments
    ///
    /// * `unpack_root` - The root directory where the artifact bundle was extracted.
    ///
    /// # Returns
    ///
    /// The resolved [`PathBuf`] for executing registry CLI commands.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::UnsafeArchivePath`] if `sub_directory` attempts path traversal.
    pub fn resolve_working_dir(&self, unpack_root: &Path) -> Result<PathBuf> {
        if self.topology_mode == RepoTopologyMode::Monorepo
            && let Some(sub_dir) = &self.sub_directory
        {
            let sub_path = Path::new(sub_dir);
            validate_safe_relative_path(sub_path)?;
            return Ok(unpack_root.join(sub_path));
        }
        Ok(unpack_root.to_path_buf())
    }
}

/// Supported archive compression and packaging formats for artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
pub enum ArchiveFormat {
    /// A ZIP archive (`.zip`).
    #[display("zip")]
    Zip,
    /// A Gzip-compressed TAR archive (`.tar.gz` or `.tgz`).
    #[display("tar.gz")]
    TarGz,
}

impl ArchiveFormat {
    /// Determines the [`ArchiveFormat`] from a URL or file path string.
    ///
    /// Query strings (`?...`) and fragments (`#...`) are stripped prior to
    /// performing a case-insensitive extension check.
    ///
    /// # Arguments
    ///
    /// * `url_or_path` - The URL or path string to inspect.
    ///
    /// # Returns
    ///
    /// The detected [`ArchiveFormat`].
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::UnsupportedArchiveFormat`] if the path does not end with
    /// `.zip`, `.tar.gz`, or `.tgz` (case-insensitively).
    pub fn from_url_or_path(url_or_path: &str) -> Result<Self> {
        let without_fragment = url_or_path.split('#').next().unwrap_or(url_or_path);
        let clean_path = without_fragment
            .split('?')
            .next()
            .unwrap_or(without_fragment);
        let path = Path::new(clean_path);
        let ext = path.extension();

        let is_tgz = ext.is_some_and(|e| e.eq_ignore_ascii_case("tgz"));
        let is_tar_gz = ext.is_some_and(|e| e.eq_ignore_ascii_case("gz"))
            && path
                .file_stem()
                .and_then(|stem| Path::new(stem).extension())
                .is_some_and(|e| e.eq_ignore_ascii_case("tar"));
        let is_zip = ext.is_some_and(|e| e.eq_ignore_ascii_case("zip"));

        if is_tar_gz || is_tgz {
            Ok(Self::TarGz)
        } else if is_zip {
            Ok(Self::Zip)
        } else {
            Err(PublisherError::UnsupportedArchiveFormat {
                url: url_or_path.to_string(),
            })
        }
    }
}

/// Strongly-typed identifier/URL for an artifact bundle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct ArtifactUrl(String);

impl ArtifactUrl {
    /// Creates a new [`ArtifactUrl`] from a string-like value.
    ///
    /// # Arguments
    ///
    /// * `url` - The URL or identifier of the artifact.
    ///
    /// # Returns
    ///
    /// A new [`ArtifactUrl`].
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self(url.into())
    }

    /// Returns the underlying string slice of the artifact URL.
    ///
    /// # Returns
    ///
    /// The artifact URL as `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Determines the [`ArchiveFormat`] of this artifact URL.
    ///
    /// # Returns
    ///
    /// The detected [`ArchiveFormat`].
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::UnsupportedArchiveFormat`] if the URL extension is unsupported.
    pub fn archive_format(&self) -> Result<ArchiveFormat> {
        ArchiveFormat::from_url_or_path(&self.0)
    }
}

impl From<&str> for ArtifactUrl {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed Redis stream name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct StreamName(String);

impl StreamName {
    /// Creates a new [`StreamName`].
    ///
    /// # Arguments
    ///
    /// * `name` - The stream identifier.
    ///
    /// # Returns
    ///
    /// A new [`StreamName`].
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the stream name as a string slice.
    ///
    /// # Returns
    ///
    /// The underlying `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for StreamName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed Redis consumer group name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct GroupName(String);

impl GroupName {
    /// Creates a new [`GroupName`].
    ///
    /// # Arguments
    ///
    /// * `name` - The consumer group identifier.
    ///
    /// # Returns
    ///
    /// A new [`GroupName`].
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the group name as a string slice.
    ///
    /// # Returns
    ///
    /// The underlying `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for GroupName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed Redis consumer name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct ConsumerName(String);

impl ConsumerName {
    /// Creates a new [`ConsumerName`].
    ///
    /// # Arguments
    ///
    /// * `name` - The consumer instance identifier.
    ///
    /// # Returns
    ///
    /// A new [`ConsumerName`].
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Returns the consumer name as a string slice.
    ///
    /// # Returns
    ///
    /// The underlying `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ConsumerName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed queue message ID (e.g. `"1700000000000-0"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, From, Into)]
#[serde(transparent)]
pub struct MessageId(String);

impl MessageId {
    /// Creates a new [`MessageId`].
    ///
    /// # Arguments
    ///
    /// * `id` - The broker-assigned message identifier.
    ///
    /// # Returns
    ///
    /// A new [`MessageId`].
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the message ID as a string slice.
    ///
    /// # Returns
    ///
    /// The underlying `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MessageId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Strongly-typed URL for the `cdd-control-plane` service.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, AsRef, Into)]
#[serde(transparent)]
pub struct ControlPlaneUrl(String);

impl ControlPlaneUrl {
    /// Creates a new [`ControlPlaneUrl`], normalizing any trailing slashes.
    ///
    /// # Arguments
    ///
    /// * `url` - The base URL of the control plane (e.g. `"http://127.0.0.1:8080"`).
    ///
    /// # Returns
    ///
    /// A normalized [`ControlPlaneUrl`].
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        let raw = url.into();
        let normalized = raw.trim_end_matches('/').to_string();
        Self(normalized)
    }

    /// Returns the normalized base URL as a string slice.
    ///
    /// # Returns
    ///
    /// The underlying `&str`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Constructs the full audit publish endpoint URL (`<base>/api/v1/audit/publish`).
    ///
    /// # Returns
    ///
    /// The full HTTP endpoint URL string.
    #[must_use]
    pub fn audit_publish_endpoint(&self) -> String {
        format!("{}/api/v1/audit/publish", self.0)
    }
}

impl From<String> for ControlPlaneUrl {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for ControlPlaneUrl {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// Configuration for the event stream consumer group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamConfig {
    /// The name of the stream to read jobs from.
    pub stream_name: StreamName,
    /// The name of the consumer group.
    pub group_name: GroupName,
    /// The unique name of the worker consumer within the group.
    pub consumer_name: ConsumerName,
}

impl StreamConfig {
    /// Creates a new [`StreamConfig`].
    ///
    /// # Arguments
    ///
    /// * `stream_name` - The stream name.
    /// * `group_name` - The consumer group name.
    /// * `consumer_name` - The consumer name.
    ///
    /// # Returns
    ///
    /// A new [`StreamConfig`].
    #[must_use]
    pub fn new(
        stream_name: impl Into<StreamName>,
        group_name: impl Into<GroupName>,
        consumer_name: impl Into<ConsumerName>,
    ) -> Self {
        Self {
            stream_name: stream_name.into(),
            group_name: group_name.into(),
            consumer_name: consumer_name.into(),
        }
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self::new("publisher_jobs", "publisher_group", "worker_1")
    }
}

/// A raw unparsed message read from the event queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawQueueMessage {
    /// The broker message ID used for acknowledgement.
    pub id: MessageId,
    /// The raw byte payload of the message.
    pub payload: Vec<u8>,
}

impl RawQueueMessage {
    /// Creates a new [`RawQueueMessage`].
    ///
    /// # Arguments
    ///
    /// * `id` - The broker message identifier.
    /// * `payload` - The raw payload bytes.
    ///
    /// # Returns
    ///
    /// A new [`RawQueueMessage`].
    #[must_use]
    pub fn new(id: impl Into<MessageId>, payload: Vec<u8>) -> Self {
        Self {
            id: id.into(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_13_registries_from_str_display_and_env_vars() -> Result<()> {
        let cases = [
            ("npm", Registry::Npm, "npm", "NPM_TOKEN"),
            ("NPM", Registry::Npm, "npm", "NPM_TOKEN"),
            ("pypi", Registry::PyPi, "pypi", "TWINE_PASSWORD"),
            ("PyPI", Registry::PyPi, "pypi", "TWINE_PASSWORD"),
            ("cargo", Registry::Cargo, "cargo", "CARGO_REGISTRY_TOKEN"),
            ("crates", Registry::Cargo, "cargo", "CARGO_REGISTRY_TOKEN"),
            ("maven", Registry::Maven, "maven", "MAVEN_PASSWORD"),
            ("nuget", Registry::NuGet, "nuget", "NUGET_API_KEY"),
            (
                "rubygems",
                Registry::RubyGems,
                "rubygems",
                "GEM_HOST_API_KEY",
            ),
            ("gems", Registry::RubyGems, "rubygems", "GEM_HOST_API_KEY"),
            (
                "packagist",
                Registry::Packagist,
                "packagist",
                "PACKAGIST_TOKEN",
            ),
            (
                "composer",
                Registry::Packagist,
                "packagist",
                "PACKAGIST_TOKEN",
            ),
            ("pkggodev", Registry::PkgGoDev, "pkggodev", "GITHUB_TOKEN"),
            ("go", Registry::PkgGoDev, "pkggodev", "GITHUB_TOKEN"),
            (
                "swift",
                Registry::SwiftPackageIndex,
                "swift",
                "GITHUB_TOKEN",
            ),
            ("spm", Registry::SwiftPackageIndex, "swift", "GITHUB_TOKEN"),
            ("vcpkg", Registry::Vcpkg, "vcpkg", "VCPKG_REPO_URL"),
            ("conan", Registry::Conan, "conan", "CONAN_PASSWORD"),
            (
                "homebrew",
                Registry::Homebrew,
                "homebrew",
                "HOMEBREW_TAP_REPO",
            ),
            ("brew", Registry::Homebrew, "homebrew", "HOMEBREW_TAP_REPO"),
            (
                "github_releases",
                Registry::GithubReleases,
                "github_releases",
                "GITHUB_TOKEN",
            ),
            (
                "github",
                Registry::GithubReleases,
                "github_releases",
                "GITHUB_TOKEN",
            ),
        ];

        for (input, expected, canonical, env_var) in cases {
            let parsed = Registry::from_str(input)?;
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), canonical);
            assert_eq!(parsed.to_string(), canonical);
            assert_eq!(parsed.token_env_var(), env_var);
        }

        let invalid = Registry::from_str("unknown_ecosystem");
        assert_eq!(
            invalid.as_ref().map_err(ToString::to_string),
            Err("Unsupported registry: unknown_ecosystem".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_registry_serde_and_tokens_lookup() -> Result<()> {
        let json = serde_json::to_string(&Registry::GithubReleases)?;
        assert_eq!(json, r#""github_releases""#);
        let parsed: Registry = serde_json::from_str(r#""crates""#)?;
        assert_eq!(parsed, Registry::Cargo);

        let all_registries = [
            Registry::Npm,
            Registry::PyPi,
            Registry::Cargo,
            Registry::Maven,
            Registry::NuGet,
            Registry::RubyGems,
            Registry::Packagist,
            Registry::PkgGoDev,
            Registry::SwiftPackageIndex,
            Registry::Vcpkg,
            Registry::Conan,
            Registry::Homebrew,
            Registry::GithubReleases,
        ];

        let empty = RegistryTokens::default();
        for reg in all_registries {
            assert_eq!(
                empty.token_for(reg).err().map(|e| e.to_string()),
                Some(format!("{} is missing", reg.token_env_var()))
            );
        }

        let populated = RegistryTokens {
            npm: Some(SecretToken::new("t-npm")),
            pypi: Some(SecretToken::new("t-pypi")),
            cargo: Some(SecretToken::new("t-cargo")),
            maven_username: Some(SecretToken::new("u-maven")),
            maven_password: Some(SecretToken::new("t-maven")),
            nuget: Some(SecretToken::new("t-nuget")),
            rubygems: Some(SecretToken::new("t-gems")),
            packagist: Some(SecretToken::new("t-php")),
            github: Some(SecretToken::new("t-gh")),
            conan: Some(SecretToken::new("t-conan")),
            vcpkg_repo_url: Some(SecretToken::new("t-vcpkg")),
            homebrew_tap_repo: Some(SecretToken::new("t-brew")),
            readthedocs: Some(SecretToken::new("t-rtd")),
        };

        for reg in all_registries {
            assert_ne!(populated.token_for(reg)?.expose_secret(), "");
        }

        // Verify fallback of Vcpkg and Homebrew to github token when dedicated token is None
        let gh_fallback = RegistryTokens {
            github: Some(SecretToken::new("t-gh-fallback")),
            ..RegistryTokens::default()
        };
        assert_eq!(
            gh_fallback.token_for(Registry::Vcpkg)?.expose_secret(),
            "t-gh-fallback"
        );
        assert_eq!(
            gh_fallback.token_for(Registry::Homebrew)?.expose_secret(),
            "t-gh-fallback"
        );
        Ok(())
    }

    #[test]
    fn test_topology_org_repo_and_working_dir() -> Result<()> {
        let org = GitHubOrgName::try_new("  acme-corp  ")?;
        assert_eq!(org.as_str(), "acme-corp");
        assert_eq!(GitHubOrgName::from("acme").as_str(), "acme");
        assert!(GitHubOrgName::try_new("").is_err());
        assert!(GitHubOrgName::try_new("bad/org").is_err());
        assert!(GitHubOrgName::try_new("bad..org").is_err());

        let repo = RepoName::try_new("acme-sdk")?;
        assert_eq!(repo.as_str(), "acme-sdk");
        assert_eq!(RepoName::from("acme-sdk").as_str(), "acme-sdk");
        assert!(RepoName::try_new("").is_err());
        assert!(RepoName::try_new("bad/repo").is_err());
        assert!(RepoName::try_new("bad..repo").is_err());

        assert_eq!(RepoName::default_monorepo(&org).as_str(), "acme-corp-sdk");
        assert_eq!(
            RepoName::default_polyrepo(&org, "Rust").as_str(),
            "acme-corp-sdk-rust"
        );
        assert_eq!(RepoTopologyMode::Monorepo.to_string(), "monorepo");
        assert_eq!(RepoTopologyMode::Polyrepo.to_string(), "polyrepo");

        let root = Path::new("/tmp/unpack");
        let mut job = PublishJob::new("https://example.com/a.zip", "cargo");
        assert_eq!(job.effective_repo_name(), None);
        assert_eq!(job.resolve_working_dir(root)?, PathBuf::from("/tmp/unpack"));

        job.github_org_name = Some(org);
        assert_eq!(
            job.effective_repo_name().as_ref().map(RepoName::as_str),
            Some("acme-corp-sdk-cargo")
        );

        job.topology_mode = RepoTopologyMode::Monorepo;
        assert_eq!(
            job.effective_repo_name().as_ref().map(RepoName::as_str),
            Some("acme-corp-sdk")
        );
        assert_eq!(job.resolve_working_dir(root)?, PathBuf::from("/tmp/unpack"));

        job.sub_directory = Some("rust".to_string());
        assert_eq!(
            job.resolve_working_dir(root)?,
            PathBuf::from("/tmp/unpack/rust")
        );

        job.sub_directory = Some("../escape".to_string());
        assert!(matches!(
            job.resolve_working_dir(root),
            Err(PublisherError::UnsafeArchivePath { .. })
        ));

        job.repo_name = Some(RepoName::new("custom-repo"));
        assert_eq!(
            job.effective_repo_name().as_ref().map(RepoName::as_str),
            Some("custom-repo")
        );
        Ok(())
    }

    #[test]
    fn test_pgp_and_docs_domain_types() -> Result<()> {
        let fp = PgpKeyFingerprint::try_new("abcd 1234 ef00 9988")?;
        assert_eq!(fp.as_str(), "ABCD1234EF009988");
        assert!(PgpKeyFingerprint::try_new("short").is_err());
        assert!(PgpKeyFingerprint::try_new("not_hex_12345678").is_err());
        assert!(PgpKeyFingerprint::try_new("A".repeat(65)).is_err());

        let pgp_cfg = PgpSigningConfig {
            armored_private_key: SecretToken::new("-----BEGIN PGP PRIVATE KEY BLOCK-----"),
            passphrase: Some(SecretToken::new("pass123")),
            key_fingerprint: Some(fp),
        };
        let serialized = serde_json::to_string(&pgp_cfg)?;
        let deserialized: PgpSigningConfig = serde_json::from_str(&serialized)?;
        assert_eq!(pgp_cfg, deserialized);

        assert_eq!(DocHostTarget::default(), DocHostTarget::None);
        assert_eq!(
            DocHostTarget::OfficialCddSite.to_string(),
            "official_cdd_site"
        );
        assert_eq!(DocHostTarget::GithubPages.to_string(), "github_pages");
        assert_eq!(DocHostTarget::ReadTheDocs.to_string(), "read_the_docs");
        assert_eq!(DocHostTarget::DocsRs.to_string(), "docs_rs");
        assert_eq!(DocHostTarget::PkgGoDev.to_string(), "pkg_go_dev");
        assert_eq!(DocHostTarget::JavadocIo.to_string(), "javadoc_io");
        assert_eq!(DocHostTarget::RubyDoc.to_string(), "ruby_doc");
        assert_eq!(
            DocHostTarget::SwiftPackageIndexDocs.to_string(),
            "swift_package_index_docs"
        );
        assert_eq!(DocHostTarget::CustomUrl.to_string(), "custom_url");
        assert_eq!(DocHostTarget::None.to_string(), "none");

        let docs_job = DocsPublishJob {
            target: DocHostTarget::OfficialCddSite,
            org_id: "org1".to_string(),
            repo_id: "repo1".to_string(),
            version: "1.0.0".to_string(),
            github_org_name: Some(GitHubOrgName::new("org1")),
            repo_name: Some(RepoName::new("org1-sdk")),
            github_pages_branch: Some("gh-pages".to_string()),
            custom_url: None,
        };
        let docs_json = serde_json::to_string(&docs_job)?;
        let parsed_docs: DocsPublishJob = serde_json::from_str(&docs_json)?;
        assert_eq!(docs_job, parsed_docs);
        Ok(())
    }

    #[test]
    fn test_secret_token_redaction_and_formatting() {
        let token = SecretToken::from("super-secret-xyz");
        assert_eq!(token.expose_secret(), "super-secret-xyz");
        assert_eq!(format!("{token}"), "[REDACTED]");
        assert_eq!(format!("{token:?}"), "SecretToken([REDACTED])");

        let token_owned = SecretToken::from("super-secret-xyz".to_string());
        assert_eq!(token, token_owned);

        let log = "Error authenticating with super-secret-xyz against registry";
        assert_eq!(
            token.redact_from(log),
            "Error authenticating with *** against registry"
        );

        let empty_token = SecretToken::new("");
        assert_eq!(empty_token.redact_from("abc"), "abc");
    }

    #[test]
    fn test_archive_format_detection() -> Result<()> {
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.zip")?,
            ArchiveFormat::Zip
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.ZIP?token=123#frag")?,
            ArchiveFormat::Zip
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.tar.gz")?,
            ArchiveFormat::TarGz
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.TAR.GZ?sig=abc")?,
            ArchiveFormat::TarGz
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.tgz")?,
            ArchiveFormat::TarGz
        );
        assert_eq!(
            ArchiveFormat::from_url_or_path("https://example.com/pkg.TGZ")?,
            ArchiveFormat::TarGz
        );
        assert_eq!(ArchiveFormat::Zip.to_string(), "zip");
        assert_eq!(ArchiveFormat::TarGz.to_string(), "tar.gz");

        let unsupported = ArchiveFormat::from_url_or_path("https://example.com/pkg.rar");
        assert_eq!(
            unsupported.as_ref().map_err(ToString::to_string),
            Err("Unsupported artifact format: https://example.com/pkg.rar".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_artifact_url_and_identifiers() -> Result<()> {
        let url = ArtifactUrl::from("https://example.com/sdk.tar.gz");
        assert_eq!(url.as_str(), "https://example.com/sdk.tar.gz");
        assert_eq!(url.as_ref(), "https://example.com/sdk.tar.gz");
        assert_eq!(url.archive_format()?, ArchiveFormat::TarGz);
        let raw_url: String = url.into();
        assert_eq!(raw_url, "https://example.com/sdk.tar.gz");

        let stream = StreamName::from("jobs");
        assert_eq!(stream.as_str(), "jobs");
        assert_eq!(stream.as_ref(), "jobs");
        assert_eq!(String::from(stream), "jobs");

        let group = GroupName::from("grp");
        assert_eq!(group.as_str(), "grp");
        assert_eq!(group.as_ref(), "grp");
        assert_eq!(String::from(group), "grp");

        let consumer = ConsumerName::from("c1");
        assert_eq!(consumer.as_str(), "c1");
        assert_eq!(consumer.as_ref(), "c1");
        assert_eq!(String::from(consumer), "c1");

        let msg_id = MessageId::from("1-0");
        assert_eq!(msg_id.as_str(), "1-0");
        assert_eq!(msg_id.as_ref(), "1-0");
        assert_eq!(String::from(msg_id.clone()), "1-0");

        let raw_msg = RawQueueMessage::new(msg_id, vec![1, 2, 3]);
        assert_eq!(raw_msg.id.as_str(), "1-0");
        assert_eq!(raw_msg.payload, vec![1, 2, 3]);

        let cfg = StreamConfig::default();
        assert_eq!(cfg.stream_name.as_str(), "publisher_jobs");
        assert_eq!(cfg.group_name.as_str(), "publisher_group");
        assert_eq!(cfg.consumer_name.as_str(), "worker_1");

        let cp = ControlPlaneUrl::from("http://127.0.0.1:8080///");
        assert_eq!(cp.as_str(), "http://127.0.0.1:8080");
        assert_eq!(cp.as_ref(), "http://127.0.0.1:8080");
        assert_eq!(
            cp.audit_publish_endpoint(),
            "http://127.0.0.1:8080/api/v1/audit/publish"
        );
        assert_eq!(String::from(cp), "http://127.0.0.1:8080");
        Ok(())
    }
}
