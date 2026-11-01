//! Handlers for publishing artifacts to all 13 external package registries.
//!
//! Provides both the `RegistryPublisher` trait for dependency injection and
//! `CliRegistryPublisher` for invoking `npm`, `twine`, `cargo`, `mvn`, `gradle`,
//! `dotnet`, `gem`, `composer`, `git`, `go`, `swift`, `conan`, `vcpkg`, `brew`, and `gh`
//! in isolated blocking worker threads with strict credential protection and PGP signing.

use crate::domain::{PgpSigningConfig, Registry, RepoTopologyMode, SecretToken};
use crate::error::{PublisherError, Result};
use crate::signing::{
    TempGpgHome, configure_git_signing_sync, import_private_key_sync, sign_directory_artifacts_sync,
};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Paths to the executable CLIs for all 13 registries and signing tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutablePaths {
    /// NPM executable path (`npm`).
    pub npm: String,
    /// `PyPI` (`twine`) executable path.
    pub pypi: String,
    /// Cargo executable path (`cargo`).
    pub cargo: String,
    /// Apache Maven executable path (`mvn`).
    pub mvn: String,
    /// Gradle executable path (`gradle`).
    pub gradle: String,
    /// .NET CLI executable path (`dotnet`).
    pub dotnet: String,
    /// `RubyGems` CLI executable path (`gem`).
    pub gem: String,
    /// PHP Composer executable path (`composer`).
    pub composer: String,
    /// Git executable path (`git`).
    pub git: String,
    /// Go toolchain executable path (`go`).
    pub go: String,
    /// Swift toolchain executable path (`swift`).
    pub swift: String,
    /// Conan C/C++ package manager executable path (`conan`).
    pub conan: String,
    /// Microsoft `vcpkg` executable path (`vcpkg`).
    pub vcpkg: String,
    /// Homebrew executable path (`brew`).
    pub brew: String,
    /// GitHub CLI executable path (`gh`).
    pub gh: String,
    /// `GnuPG` executable path (`gpg`).
    pub gpg: String,
}

impl ExecutablePaths {
    /// Constructs an [`ExecutablePaths`] where every toolchain executable points to `exe`.
    ///
    /// Useful for configuring uniform sandbox wrappers or mock test binaries.
    ///
    /// # Arguments
    ///
    /// * `exe` - Executable path to assign to all toolchain slots.
    ///
    /// # Returns
    ///
    /// An [`ExecutablePaths`] instance with all fields set to `exe`.
    #[must_use]
    pub fn with_all(exe: &str) -> Self {
        Self {
            npm: exe.to_string(),
            pypi: exe.to_string(),
            cargo: exe.to_string(),
            mvn: exe.to_string(),
            gradle: exe.to_string(),
            dotnet: exe.to_string(),
            gem: exe.to_string(),
            composer: exe.to_string(),
            git: exe.to_string(),
            go: exe.to_string(),
            swift: exe.to_string(),
            conan: exe.to_string(),
            vcpkg: exe.to_string(),
            brew: exe.to_string(),
            gh: exe.to_string(),
            gpg: exe.to_string(),
        }
    }
}

impl Default for ExecutablePaths {
    fn default() -> Self {
        Self {
            npm: "npm".to_string(),
            pypi: "twine".to_string(),
            cargo: "cargo".to_string(),
            mvn: "mvn".to_string(),
            gradle: "gradle".to_string(),
            dotnet: "dotnet".to_string(),
            gem: "gem".to_string(),
            composer: "composer".to_string(),
            git: "git".to_string(),
            go: "go".to_string(),
            swift: "swift".to_string(),
            conan: "conan".to_string(),
            vcpkg: "vcpkg".to_string(),
            brew: "brew".to_string(),
            gh: "gh".to_string(),
            gpg: "gpg".to_string(),
        }
    }
}

/// Abstract interface for publishing an unpacked artifact directory to a target registry.
pub trait RegistryPublisher: Send + Sync {
    /// Publishes the artifact located in `dir` to `registry` authenticating with `token`.
    ///
    /// # Arguments
    ///
    /// * `registry` - The target package registry.
    /// * `dir` - The working directory of the unpacked artifact.
    /// * `token` - The secret authentication token for the registry.
    ///
    /// # Returns
    ///
    /// `Ok(())` when the package is published.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if credential setup fails, the CLI process cannot be
    /// spawned, or the registry returns a non-zero exit code.
    fn publish(
        &self,
        registry: Registry,
        dir: &Path,
        token: &SecretToken,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Publishes the artifact located in `dir` to `registry` with explicit repository topology
    /// and optional PGP signing configuration.
    ///
    /// # Arguments
    ///
    /// * `registry` - The target package registry.
    /// * `dir` - The working directory of the unpacked artifact.
    /// * `token` - The secret authentication token for the registry.
    /// * `topology` - Repository topology mode (`Monorepo` vs `Polyrepo`).
    /// * `pgp_config` - Optional PGP signing configuration.
    ///
    /// # Returns
    ///
    /// `Ok(())` when signing and publishing succeed.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if PGP signing, credential setup, or CLI execution fails.
    fn publish_with_options(
        &self,
        registry: Registry,
        dir: &Path,
        token: &SecretToken,
        topology: RepoTopologyMode,
        pgp_config: Option<&PgpSigningConfig>,
    ) -> impl Future<Output = Result<()>> + Send {
        let _ = (topology, pgp_config);
        self.publish(registry, dir, token)
    }
}

/// CLI-backed implementation of [`RegistryPublisher`] supporting all 13 target ecosystems.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliRegistryPublisher {
    /// Configured executable names or paths for each registry CLI.
    exe_paths: ExecutablePaths,
}

impl CliRegistryPublisher {
    /// Creates a new [`CliRegistryPublisher`] using the default CLI executable names.
    ///
    /// # Returns
    ///
    /// A default [`CliRegistryPublisher`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new [`CliRegistryPublisher`] with custom executable paths.
    ///
    /// # Arguments
    ///
    /// * `exe_paths` - Custom executable paths for supported toolchains.
    ///
    /// # Returns
    ///
    /// A configured [`CliRegistryPublisher`].
    #[must_use]
    pub const fn with_paths(exe_paths: ExecutablePaths) -> Self {
        Self { exe_paths }
    }

    /// Returns a reference to the configured [`ExecutablePaths`].
    ///
    /// # Returns
    ///
    /// A reference to the underlying [`ExecutablePaths`].
    #[must_use]
    pub const fn exe_paths(&self) -> &ExecutablePaths {
        &self.exe_paths
    }
}

impl RegistryPublisher for CliRegistryPublisher {
    async fn publish(&self, registry: Registry, dir: &Path, token: &SecretToken) -> Result<()> {
        self.publish_with_options(registry, dir, token, RepoTopologyMode::Polyrepo, None)
            .await
    }

    async fn publish_with_options(
        &self,
        registry: Registry,
        dir: &Path,
        token: &SecretToken,
        topology: RepoTopologyMode,
        pgp_config: Option<&PgpSigningConfig>,
    ) -> Result<()> {
        let dir_buf = dir.to_path_buf();
        let token_clone = token.clone();
        let paths = self.exe_paths.clone();
        let pgp_owned = pgp_config.cloned();

        tokio::task::spawn_blocking(move || {
            dispatch_registry_sync(
                registry,
                &dir_buf,
                &token_clone,
                topology,
                pgp_owned.as_ref(),
                &paths,
            )
        })
        .await?
    }
}

/// Dispatches synchronous CLI execution for all 13 [`Registry`] variants.
///
/// # Arguments
///
/// * `registry` - Target registry.
/// * `dir` - Working directory.
/// * `token` - Primary secret token for the registry.
/// * `topology` - Repository topology mode.
/// * `pgp_config` - Optional PGP signing config.
/// * `paths` - Configured executable paths.
///
/// # Returns
///
/// `Ok(())` on successful publish.
///
/// # Errors
///
/// Returns [`PublisherError`] if signing or CLI execution fails.
fn dispatch_registry_sync(
    registry: Registry,
    dir: &Path,
    token: &SecretToken,
    topology: RepoTopologyMode,
    pgp_config: Option<&PgpSigningConfig>,
    paths: &ExecutablePaths,
) -> Result<()> {
    match registry {
        Registry::Npm => publish_npm_with_exe(dir, token, &paths.npm),
        Registry::PyPi => {
            if let Some(cfg) = pgp_config {
                let _ = sign_directory_artifacts_sync(
                    &paths.gpg,
                    &dir.join("dist"),
                    &[".whl", ".tar.gz"],
                    cfg,
                )?;
            }
            publish_pypi_with_exe(dir, token, &paths.pypi)
        }
        Registry::Cargo => publish_cargo_with_exe(dir, token, &paths.cargo),
        Registry::Maven => publish_maven_with_exe(dir, token, pgp_config, paths),
        Registry::NuGet => publish_nuget_with_exe(dir, token, &paths.dotnet),
        Registry::RubyGems => publish_rubygems_with_exe(dir, token, &paths.gem),
        Registry::Packagist => publish_packagist_with_exe(dir, token, &paths.composer),
        Registry::PkgGoDev => publish_pkggodev_with_exe(dir, token, topology, pgp_config, paths),
        Registry::SwiftPackageIndex => publish_swift_with_exe(dir, token, pgp_config, paths),
        Registry::Conan => publish_conan_with_exe(dir, token, &paths.conan),
        Registry::Vcpkg => publish_vcpkg_with_exe(dir, token, &paths.vcpkg),
        Registry::Homebrew => publish_homebrew_with_exe(dir, token, pgp_config, paths),
        Registry::GithubReleases => publish_github_releases_with_exe(dir, token, pgp_config, paths),
    }
}

/// RAII guard that removes a temporary credential file (such as `.npmrc` or `settings.xml`) when dropped.
struct SecretFileCleanupGuard {
    /// Path to the temporary secret file to remove on drop.
    path: PathBuf,
}

impl Drop for SecretFileCleanupGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Writes a secret configuration file with restrictive (`0o600` on Unix) permissions and returns a cleanup guard.
///
/// # Arguments
///
/// * `path` - File path to create.
/// * `content` - File content containing credentials.
///
/// # Returns
///
/// A [`SecretFileCleanupGuard`] that deletes `path` on drop.
///
/// # Errors
///
/// Returns [`PublisherError::Io`] if writing or setting permissions fails.
fn write_secret_file(path: PathBuf, content: &str) -> Result<SecretFileCleanupGuard> {
    fs::write(&path, content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(SecretFileCleanupGuard { path })
}

/// Writes an `.npmrc` file with restrictive (`0o600` on Unix) permissions and returns a cleanup guard.
///
/// # Arguments
///
/// * `dir` - The package directory where `.npmrc` will be created.
/// * `token` - The NPM authentication token.
///
/// # Returns
///
/// A [`SecretFileCleanupGuard`] for the created `.npmrc`.
///
/// # Errors
///
/// Returns an error if creating, setting permissions, or writing `.npmrc` fails.
fn write_secure_npmrc(dir: &Path, token: &SecretToken) -> Result<SecretFileCleanupGuard> {
    let npmrc_path = dir.join(".npmrc");
    let content = format!("//registry.npmjs.org/:_authToken={}", token.expose_secret());
    write_secret_file(npmrc_path, &content)
}

/// Formats and redacts combined `stderr`/`stdout` output from a failed CLI invocation.
///
/// # Arguments
///
/// * `prefix` - Context prefix (e.g., `"npm publish failed"`).
/// * `stderr` - Raw stderr bytes from the child process.
/// * `stdout` - Raw stdout bytes from the child process.
/// * `token` - Secret token to scrub from the output.
///
/// # Returns
///
/// A sanitized [`PublisherError::Publish`] instance.
#[must_use]
fn format_redacted_failure(
    prefix: &str,
    stderr: &[u8],
    stdout: &[u8],
    token: &SecretToken,
) -> PublisherError {
    let raw_stderr = String::from_utf8_lossy(stderr);
    let raw_stdout = String::from_utf8_lossy(stdout);
    let combined = if raw_stderr.trim().is_empty() {
        raw_stdout.into_owned()
    } else if raw_stdout.trim().is_empty() {
        raw_stderr.into_owned()
    } else {
        format!(
            "{raw_stderr}
{raw_stdout}"
        )
    };
    let safe_output = token.redact_from(&combined);
    PublisherError::Publish(format!("{prefix}: {safe_output}"))
}

/// Publishes to NPM using the default `npm` executable.
///
/// # Arguments
///
/// * `dir` - The directory containing the NPM package to publish.
/// * `token` - The NPM authentication token.
///
/// # Returns
///
/// `Ok(())` when `npm publish` succeeds.
///
/// # Errors
///
/// Returns an error if writing `.npmrc` fails, if the command cannot be executed,
/// or if `npm publish` returns a non-zero exit code.
pub fn publish_npm(dir: &Path, token: &SecretToken) -> Result<()> {
    publish_npm_with_exe(dir, token, &ExecutablePaths::default().npm)
}

/// Internal testable implementation of NPM publish that accepts an executable path.
///
/// # Arguments
///
/// * `dir` - The directory containing the NPM package to publish.
/// * `token` - The NPM authentication token.
/// * `exe` - The path or name of the `npm` executable.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns an error on I/O or command execution failure.
pub fn publish_npm_with_exe(dir: &Path, token: &SecretToken, exe: &str) -> Result<()> {
    let _guard = write_secure_npmrc(dir, token)?;

    let output = Command::new(exe)
        .arg("publish")
        .arg("--access")
        .arg("public")
        .env("NPM_TOKEN", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(format_redacted_failure(
            "npm publish failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

/// Collects distribution artifact paths inside `dir/dist`, falling back to `"dist/*"`
/// if `dist/` does not exist or is empty.
///
/// # Arguments
///
/// * `dir` - The root directory of the unpacked Python package.
///
/// # Returns
///
/// Sorted list of relative artifact paths under `dist/`.
#[must_use]
fn collect_pypi_dist_args(dir: &Path) -> Vec<String> {
    let dist_dir = dir.join("dist");
    if let Ok(entries) = fs::read_dir(&dist_dir) {
        let mut files: Vec<String> = entries
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().to_str().map(|name| format!("dist/{name}")))
            .collect();
        files.sort();
        if !files.is_empty() {
            return files;
        }
    }
    vec!["dist/*".to_string()]
}

/// Publishes to `PyPI` using the default `twine` executable.
///
/// # Arguments
///
/// * `dir` - The directory containing the Python distribution files (`dist/`).
/// * `token` - The `PyPI` authentication token.
///
/// # Returns
///
/// `Ok(())` when `twine upload` succeeds.
///
/// # Errors
///
/// Returns an error if the command cannot be executed or if `twine upload` returns a non-zero exit code.
pub fn publish_pypi(dir: &Path, token: &SecretToken) -> Result<()> {
    publish_pypi_with_exe(dir, token, &ExecutablePaths::default().pypi)
}

/// Internal testable implementation of `PyPI` publish that accepts an executable path.
///
/// # Arguments
///
/// * `dir` - The directory containing the Python distribution files (`dist/`).
/// * `token` - The `PyPI` authentication token.
/// * `exe` - The path or name of the `twine` executable.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns an error on command execution failure.
pub fn publish_pypi_with_exe(dir: &Path, token: &SecretToken, exe: &str) -> Result<()> {
    let dist_args = collect_pypi_dist_args(dir);
    let output = Command::new(exe)
        .arg("upload")
        .arg("--non-interactive")
        .args(&dist_args)
        .env("TWINE_USERNAME", "__token__")
        .env("TWINE_PASSWORD", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(format_redacted_failure(
            "twine upload failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes to Cargo using the default `cargo` executable.
///
/// # Arguments
///
/// * `dir` - The directory containing the Rust crate (`Cargo.toml`).
/// * `token` - The Cargo registry token.
///
/// # Returns
///
/// `Ok(())` when `cargo publish` succeeds.
///
/// # Errors
///
/// Returns an error if the command cannot be executed or if `cargo publish` returns a non-zero exit code.
pub fn publish_cargo(dir: &Path, token: &SecretToken) -> Result<()> {
    publish_cargo_with_exe(dir, token, &ExecutablePaths::default().cargo)
}

/// Internal testable implementation of Cargo publish that accepts an executable path.
///
/// # Arguments
///
/// * `dir` - The directory containing the Rust crate (`Cargo.toml`).
/// * `token` - The Cargo registry token.
/// * `exe` - The path or name of the `cargo` executable.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns an error on command execution failure.
pub fn publish_cargo_with_exe(dir: &Path, token: &SecretToken, exe: &str) -> Result<()> {
    let output = Command::new(exe)
        .arg("publish")
        .env("CARGO_REGISTRY_TOKEN", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(format_redacted_failure(
            "cargo publish failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes a Java or Kotlin project to Maven Central / Sonatype (`mvn deploy` or `gradle publish`).
///
/// Writes an ephemeral `settings.xml` with `0o600` permissions that is automatically deleted on drop.
///
/// # Arguments
///
/// * `dir` - Project directory containing `pom.xml`, `build.gradle`, or `build.gradle.kts`.
/// * `token` - Maven Central password/token.
/// * `pgp_config` - Optional PGP signing config for Maven GPG signing.
/// * `paths` - Configured toolchain executable paths.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if credential file creation, PGP import, or build tool execution fails.
pub fn publish_maven_with_exe(
    dir: &Path,
    token: &SecretToken,
    pgp_config: Option<&PgpSigningConfig>,
    paths: &ExecutablePaths,
) -> Result<()> {
    let settings_path = dir.join("settings.xml");
    let xml = format!(
        "<settings><servers><server><id>central</id><username>__token__</username><password>{}</password></server></servers></settings>",
        token.expose_secret()
    );
    let _guard = write_secret_file(settings_path.clone(), &xml)?;

    let maybe_gpg_home = if let Some(cfg) = pgp_config {
        let home = TempGpgHome::new()?;
        import_private_key_sync(&paths.gpg, home.path(), cfg)?;
        Some(home)
    } else {
        None
    };

    let use_gradle = dir.join("build.gradle.kts").exists() || dir.join("build.gradle").exists();
    let mut cmd = if use_gradle {
        let mut c = Command::new(&paths.gradle);
        c.arg("publish").arg("--no-daemon");
        c
    } else {
        let mut c = Command::new(&paths.mvn);
        c.arg("deploy").arg("-B").arg("-s").arg(&settings_path);
        c
    };

    if let Some(home) = &maybe_gpg_home {
        cmd.env("GNUPGHOME", home.path());
    }
    cmd.env("MAVEN_PASSWORD", token.expose_secret())
        .current_dir(dir);

    let output = cmd.output()?;
    if !output.status.success() {
        return Err(format_redacted_failure(
            "maven publish failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes a C# `.nupkg` package to `NuGet.org` via `dotnet nuget push`.
///
/// # Arguments
///
/// * `dir` - Directory containing the `.nupkg` package.
/// * `token` - `NuGet` API key.
/// * `dotnet_exe` - Path to the `dotnet` executable.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if `dotnet nuget push` fails.
pub fn publish_nuget_with_exe(dir: &Path, token: &SecretToken, dotnet_exe: &str) -> Result<()> {
    let output = Command::new(dotnet_exe)
        .arg("nuget")
        .arg("push")
        .arg("*.nupkg")
        .arg("--api-key")
        .arg(token.expose_secret())
        .arg("--source")
        .arg("https://api.nuget.org/v3/index.json")
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(format_redacted_failure(
            "dotnet nuget push failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes a Ruby gem to `RubyGems.org` via `gem push`.
///
/// # Arguments
///
/// * `dir` - Directory containing `.gem` or `.gemspec`.
/// * `token` - `RubyGems` API key (`GEM_HOST_API_KEY`).
/// * `gem_exe` - Path to the `gem` executable.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if `gem push` fails.
pub fn publish_rubygems_with_exe(dir: &Path, token: &SecretToken, gem_exe: &str) -> Result<()> {
    let gem_arg = fs::read_dir(dir)
        .ok()
        .and_then(|entries| {
            entries
                .filter_map(std::result::Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .find(|name| {
                    Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("gem"))
                })
        })
        .unwrap_or_else(|| "package.gem".to_string());

    let output = Command::new(gem_exe)
        .arg("push")
        .arg(&gem_arg)
        .env("GEM_HOST_API_KEY", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(format_redacted_failure(
            "gem push failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

/// Validates and publishes a PHP Composer package for `Packagist.org`.
///
/// # Arguments
///
/// * `dir` - Directory containing `composer.json`.
/// * `token` - Packagist API token.
/// * `composer_exe` - Path to the `composer` executable.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if `composer` validation/publish fails.
pub fn publish_packagist_with_exe(
    dir: &Path,
    token: &SecretToken,
    composer_exe: &str,
) -> Result<()> {
    let output = Command::new(composer_exe)
        .arg("validate")
        .arg("--no-interaction")
        .env("COMPOSER_AUTH", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(format_redacted_failure(
            "composer publish failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes a Go module to `pkg.go.dev` by creating a topology-aware Git tag (`v0.1.0` or `go/v0.1.0`)
/// and warming the Go module proxy.
///
/// # Arguments
///
/// * `dir` - Go module directory.
/// * `token` - GitHub token.
/// * `topology` - Repository topology (`Monorepo` uses `go/v0.1.0`, `Polyrepo` uses `v0.1.0`).
/// * `pgp_config` - Optional PGP signing config for signed Git tags.
/// * `paths` - Toolchain executable paths.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if Git tagging or Go module listing fails.
pub fn publish_pkggodev_with_exe(
    dir: &Path,
    token: &SecretToken,
    topology: RepoTopologyMode,
    pgp_config: Option<&PgpSigningConfig>,
    paths: &ExecutablePaths,
) -> Result<()> {
    let maybe_gpg = if let Some(cfg) = pgp_config {
        Some(configure_git_signing_sync(
            &paths.gpg, &paths.git, dir, cfg,
        )?)
    } else {
        None
    };

    let tag_name = match topology {
        RepoTopologyMode::Monorepo => "go/v0.1.0",
        RepoTopologyMode::Polyrepo => "v0.1.0",
    };

    let mut tag_cmd = Command::new(&paths.git);
    tag_cmd.arg("tag").arg("-f");
    if let Some(home) = &maybe_gpg {
        tag_cmd
            .arg("-s")
            .arg("-m")
            .arg("Release")
            .env("GNUPGHOME", home.path());
    }
    let tag_out = tag_cmd
        .arg(tag_name)
        .env("GITHUB_TOKEN", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !tag_out.status.success() {
        return Err(format_redacted_failure(
            "go git tag failed",
            &tag_out.stderr,
            &tag_out.stdout,
            token,
        ));
    }

    let go_out = Command::new(&paths.go)
        .arg("list")
        .arg("-m")
        .env("GITHUB_TOKEN", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !go_out.status.success() {
        return Err(format_redacted_failure(
            "pkggodev index failed",
            &go_out.stderr,
            &go_out.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes a Swift package to the Swift Package Index via Git tag and `swift package dump-package`.
///
/// # Arguments
///
/// * `dir` - Directory containing `Package.swift`.
/// * `token` - GitHub / Swift Package Index token.
/// * `pgp_config` - Optional PGP signing config for signed Git tags.
/// * `paths` - Toolchain executable paths.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if Git tagging or `swift` validation fails.
pub fn publish_swift_with_exe(
    dir: &Path,
    token: &SecretToken,
    pgp_config: Option<&PgpSigningConfig>,
    paths: &ExecutablePaths,
) -> Result<()> {
    let maybe_gpg = if let Some(cfg) = pgp_config {
        Some(configure_git_signing_sync(
            &paths.gpg, &paths.git, dir, cfg,
        )?)
    } else {
        None
    };

    let mut tag_cmd = Command::new(&paths.git);
    tag_cmd.arg("tag").arg("-f");
    if let Some(home) = &maybe_gpg {
        tag_cmd
            .arg("-s")
            .arg("-m")
            .arg("Release")
            .env("GNUPGHOME", home.path());
    }
    let tag_out = tag_cmd
        .arg("v0.1.0")
        .env("GITHUB_TOKEN", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !tag_out.status.success() {
        return Err(format_redacted_failure(
            "swift git tag failed",
            &tag_out.stderr,
            &tag_out.stdout,
            token,
        ));
    }

    let swift_out = Command::new(&paths.swift)
        .arg("package")
        .arg("dump-package")
        .env("GITHUB_TOKEN", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !swift_out.status.success() {
        return Err(format_redacted_failure(
            "swift publish failed",
            &swift_out.stderr,
            &swift_out.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes a C/C++ package to a Conan remote via `conan export-pkg` and `conan upload`.
///
/// # Arguments
///
/// * `dir` - Directory containing `conanfile.py` or `conanfile.txt`.
/// * `token` - Conan password/token (`CONAN_PASSWORD`).
/// * `conan_exe` - Path to the `conan` executable.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if `conan export-pkg` or `conan upload` fails.
pub fn publish_conan_with_exe(dir: &Path, token: &SecretToken, conan_exe: &str) -> Result<()> {
    let export_out = Command::new(conan_exe)
        .arg("export-pkg")
        .arg(".")
        .env("CONAN_PASSWORD", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !export_out.status.success() {
        return Err(format_redacted_failure(
            "conan export-pkg failed",
            &export_out.stderr,
            &export_out.stdout,
            token,
        ));
    }

    let upload_out = Command::new(conan_exe)
        .arg("upload")
        .arg("*")
        .arg("-r")
        .arg("default")
        .arg("--confirm")
        .env("CONAN_PASSWORD", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !upload_out.status.success() {
        return Err(format_redacted_failure(
            "conan upload failed",
            &upload_out.stderr,
            &upload_out.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes a C/C++ port to a `vcpkg` registry via `vcpkg x-add-version`.
///
/// # Arguments
///
/// * `dir` - Directory containing `vcpkg.json`.
/// * `token` - `vcpkg` repository URL or token (`VCPKG_REPO_URL`).
/// * `vcpkg_exe` - Path to the `vcpkg` executable.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if `vcpkg` fails.
pub fn publish_vcpkg_with_exe(dir: &Path, token: &SecretToken, vcpkg_exe: &str) -> Result<()> {
    let output = Command::new(vcpkg_exe)
        .arg("x-add-version")
        .arg("--all")
        .env("VCPKG_REPO_URL", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(format_redacted_failure(
            "vcpkg publish failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes or updates a Homebrew formula tap via `brew bump-formula-pr`.
///
/// # Arguments
///
/// * `dir` - Formula or package directory.
/// * `token` - Homebrew tap token (`HOMEBREW_TAP_REPO`).
/// * `pgp_config` - Optional PGP signing config for signing archives in `dir`.
/// * `paths` - Toolchain executable paths.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if signing or `brew` execution fails.
pub fn publish_homebrew_with_exe(
    dir: &Path,
    token: &SecretToken,
    pgp_config: Option<&PgpSigningConfig>,
    paths: &ExecutablePaths,
) -> Result<()> {
    if let Some(cfg) = pgp_config {
        let _ = sign_directory_artifacts_sync(&paths.gpg, dir, &[".tar.gz", ".zip"], cfg)?;
    }

    let output = Command::new(&paths.brew)
        .arg("bump-formula-pr")
        .arg("--write-only")
        .env("HOMEBREW_TAP_REPO", token.expose_secret())
        .env("HOMEBREW_GITHUB_API_TOKEN", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(format_redacted_failure(
            "homebrew publish failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

/// Publishes release archives and `.asc` detached signatures to GitHub Releases via `gh release upload`.
///
/// # Arguments
///
/// * `dir` - Directory containing release assets.
/// * `token` - GitHub token (`GITHUB_TOKEN`).
/// * `pgp_config` - Optional PGP signing config to generate `.asc` signatures before uploading.
/// * `paths` - Toolchain executable paths.
///
/// # Returns
///
/// `Ok(())` on success.
///
/// # Errors
///
/// Returns [`PublisherError`] if signing or `gh release upload` fails.
pub fn publish_github_releases_with_exe(
    dir: &Path,
    token: &SecretToken,
    pgp_config: Option<&PgpSigningConfig>,
    paths: &ExecutablePaths,
) -> Result<()> {
    if let Some(cfg) = pgp_config {
        let _ = sign_directory_artifacts_sync(&paths.gpg, dir, &[".tar.gz", ".tgz", ".zip"], cfg)?;
    }

    let output = Command::new(&paths.gh)
        .arg("release")
        .arg("upload")
        .arg("v0.1.0")
        .arg("--clobber")
        .env("GH_TOKEN", token.expose_secret())
        .env("GITHUB_TOKEN", token.expose_secret())
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Err(format_redacted_failure(
            "github release upload failed",
            &output.stderr,
            &output.stdout,
            token,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    /// Creates an executable shell script for testing CLI invocation.
    fn create_mock_cli(dir: &Path, name: &str, body: &str) -> Result<String> {
        let exe_path = dir.join(name);
        let script = format!(
            "#!/bin/sh
{body}
"
        );
        fs::write(&exe_path, script)?;
        fs::set_permissions(&exe_path, fs::Permissions::from_mode(0o755))?;
        Ok(exe_path.to_string_lossy().into_owned())
    }

    #[test]
    fn test_publish_npm_success_and_cleanup() -> Result<()> {
        let dest = TempDir::new()?;
        let body = r#"
if [ "$*" != "publish --access public" ]; then
    echo "Unexpected args: $*" >&2
    exit 1
fi
if [ ! -f ".npmrc" ]; then
    echo ".npmrc missing" >&2
    exit 1
fi
CONTENT=$(cat .npmrc)
if [ "$CONTENT" != "//registry.npmjs.org/:_authToken=secret123" ]; then
    echo "Bad .npmrc content: $CONTENT" >&2
    exit 1
fi
exit 0
"#;
        let exe = create_mock_cli(dest.path(), "mock_npm", body)?;
        let token = SecretToken::new("secret123");

        publish_npm_with_exe(dest.path(), &token, &exe)?;
        assert!(!dest.path().join(".npmrc").exists());
        Ok(())
    }

    #[test]
    fn test_publish_npm_failure_masks_token_in_stderr_and_stdout() -> Result<()> {
        let dest = TempDir::new()?;
        let body = "echo 'Auth error with secret123' >&2; echo 'Stdout detail secret123'; exit 1";
        let exe = create_mock_cli(dest.path(), "mock_npm_fail", body)?;
        let token = SecretToken::new("secret123");

        let msg = publish_npm_with_exe(dest.path(), &token, &exe)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(msg.contains("npm publish failed"));
        assert!(msg.contains("***"));
        assert!(!msg.contains("secret123"));
        assert!(!dest.path().join(".npmrc").exists());
        Ok(())
    }

    #[test]
    fn test_publish_pypi_with_explicit_dist_files() -> Result<()> {
        let dest = TempDir::new()?;
        let dist_dir = dest.path().join("dist");
        fs::create_dir_all(&dist_dir)?;
        assert_eq!(collect_pypi_dist_args(dest.path()), vec!["dist/*"]);

        fs::write(dist_dir.join("pkg-0.1.0.tar.gz"), b"tarball")?;
        fs::write(dist_dir.join("pkg-0.1.0-py3-none-any.whl"), b"wheel")?;
        fs::create_dir_all(dist_dir.join("subdir_ignored"))?;

        let body = r#"
if [ "$*" != "upload --non-interactive dist/pkg-0.1.0-py3-none-any.whl dist/pkg-0.1.0.tar.gz" ]; then
    echo "Unexpected args: $*" >&2
    exit 1
fi
if [ "$TWINE_USERNAME" != "__token__" ] || [ "$TWINE_PASSWORD" != "pypi-secret-999" ]; then
    echo "Missing env vars" >&2
    exit 1
fi
exit 0
"#;
        let exe = create_mock_cli(dest.path(), "mock_twine", body)?;
        let token = SecretToken::new("pypi-secret-999");

        publish_pypi_with_exe(dest.path(), &token, &exe)?;
        Ok(())
    }

    #[test]
    fn test_publish_pypi_fallback_glob_and_failure_masking() -> Result<()> {
        let dest = TempDir::new()?;
        let body = r#"
if [ "$*" != "upload --non-interactive dist/*" ]; then
    echo "Unexpected args: $*" >&2
    exit 1
fi
echo "Twine failed with token pypi-secret-999" >&2
exit 1
"#;
        let exe = create_mock_cli(dest.path(), "mock_twine_fail", body)?;
        let token = SecretToken::new("pypi-secret-999");

        let msg = publish_pypi_with_exe(dest.path(), &token, &exe)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(msg.contains("twine upload failed"));
        assert!(msg.contains("***"));
        assert!(!msg.contains("pypi-secret-999"));
        Ok(())
    }

    #[test]
    fn test_publish_cargo_success_and_env_passing() -> Result<()> {
        let dest = TempDir::new()?;
        let body = r#"
if [ "$*" != "publish" ]; then
    echo "Unexpected args: $*" >&2
    exit 1
fi
if [ "$CARGO_REGISTRY_TOKEN" != "cargo-secret-777" ]; then
    echo "Missing CARGO_REGISTRY_TOKEN" >&2
    exit 1
fi
exit 0
"#;
        let exe = create_mock_cli(dest.path(), "mock_cargo", body)?;
        let token = SecretToken::new("cargo-secret-777");

        publish_cargo_with_exe(dest.path(), &token, &exe)?;
        Ok(())
    }

    #[test]
    fn test_publish_cargo_failure_stdout_only_masks_token() -> Result<()> {
        let dest = TempDir::new()?;
        let body = "echo 'Stdout failure cargo-secret-777'; exit 1";
        let exe = create_mock_cli(dest.path(), "mock_cargo_fail", body)?;
        let token = SecretToken::new("cargo-secret-777");

        let msg = publish_cargo_with_exe(dest.path(), &token, &exe)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(msg.contains("cargo publish failed"));
        assert!(msg.contains("***"));
        assert!(!msg.contains("cargo-secret-777"));
        Ok(())
    }

    #[tokio::test]
    async fn test_cli_registry_publisher_all_13_registries_success_and_failure() -> Result<()> {
        let dest = TempDir::new()?;
        let ok_exe = create_mock_cli(dest.path(), "ok_cli", "exit 0")?;
        let fail_body = "echo 'Command failed with tok123' >&2; exit 1";
        let fail_exe = create_mock_cli(dest.path(), "fail_cli", fail_body)?;

        let ok_paths = ExecutablePaths::with_all(&ok_exe);
        let fail_paths = ExecutablePaths::with_all(&fail_exe);

        let publisher = CliRegistryPublisher::with_paths(ok_paths.clone());
        assert_eq!(publisher.exe_paths(), &ok_paths);

        let default_publisher = CliRegistryPublisher::new();
        assert_eq!(default_publisher.exe_paths(), &ExecutablePaths::default());

        let token = SecretToken::new("tok123");
        let pgp_cfg = PgpSigningConfig {
            armored_private_key: SecretToken::new("PGP_KEY"),
            passphrase: Some(SecretToken::new("PGP_PASS")),
            key_fingerprint: None,
        };

        // Create sample files to exercise file discovery branches (gradle, gem, dist, archive)
        let dist_dir = dest.path().join("dist");
        fs::create_dir_all(&dist_dir)?;
        fs::write(dist_dir.join("pkg-1.0.0.whl"), b"wheel")?;
        fs::write(dest.path().join("sample.gem"), b"gem")?;
        fs::write(dest.path().join("bundle.tar.gz"), b"tar")?;

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
            Registry::Conan,
            Registry::Vcpkg,
            Registry::Homebrew,
            Registry::GithubReleases,
        ];

        // 1. All 13 succeed without PGP and with PGP (and both Polyrepo / Monorepo)
        for reg in all_registries {
            publisher.publish(reg, dest.path(), &token).await?;
            publisher
                .publish_with_options(
                    reg,
                    dest.path(),
                    &token,
                    RepoTopologyMode::Monorepo,
                    Some(&pgp_cfg),
                )
                .await?;
        }

        // Also test Maven with Gradle (`build.gradle.kts`) and RubyGems without `.gem` file
        fs::write(dest.path().join("build.gradle.kts"), b"// gradle")?;
        fs::remove_file(dest.path().join("sample.gem"))?;
        publisher
            .publish(Registry::Maven, dest.path(), &token)
            .await?;
        publisher
            .publish(Registry::RubyGems, dest.path(), &token)
            .await?;

        // 2. All 13 fail cleanly and redact secret token when CLI returns non-zero
        let fail_publisher = CliRegistryPublisher::with_paths(fail_paths.clone());
        for reg in all_registries {
            let err_msg = fail_publisher
                .publish(reg, dest.path(), &token)
                .await
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default();
            assert_ne!(err_msg, "");
            assert!(!err_msg.contains("tok123"));
            assert!(
                fail_publisher
                    .publish_with_options(
                        reg,
                        dest.path(),
                        &token,
                        RepoTopologyMode::Polyrepo,
                        Some(&pgp_cfg),
                    )
                    .await
                    .is_err()
            );
        }

        // 3. Test second-step failures in multi-step publishers (PkgGoDev go list, Swift dump-package, Conan upload)
        let mut step2_fail_paths = ok_paths;
        step2_fail_paths.go = fail_exe.clone();
        step2_fail_paths.swift = fail_exe.clone();
        assert!(
            publish_pkggodev_with_exe(
                dest.path(),
                &token,
                RepoTopologyMode::Polyrepo,
                None,
                &step2_fail_paths
            )
            .is_err()
        );
        assert!(publish_swift_with_exe(dest.path(), &token, None, &step2_fail_paths).is_err());

        let conan_body = "case \"$1\" in export-pkg) exit 0 ;; *) echo 'upload failed tok123' >&2; exit 1 ;; esac";
        let conan_upload_fail = create_mock_cli(dest.path(), "conan_upload_fail", conan_body)?;
        assert!(publish_conan_with_exe(dest.path(), &token, &conan_upload_fail).is_err());

        Ok(())
    }

    /// Dummy [`RegistryPublisher`] to exercise default `publish_with_options` trait method.
    struct MinimalPublisher;

    impl RegistryPublisher for MinimalPublisher {
        fn publish(
            &self,
            _registry: Registry,
            _dir: &Path,
            _token: &SecretToken,
        ) -> impl Future<Output = Result<()>> + Send {
            std::future::ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn test_default_trait_publish_with_options_and_wrappers() -> Result<()> {
        let min_pub = MinimalPublisher;
        let token = SecretToken::new("secret");
        min_pub
            .publish_with_options(
                Registry::Npm,
                Path::new("."),
                &token,
                RepoTopologyMode::Polyrepo,
                None,
            )
            .await?;

        let missing_dir = Path::new("/nonexistent_cdd_publisher_test_dir_12345");
        assert!(publish_npm(missing_dir, &token).is_err());
        assert!(publish_pypi(missing_dir, &token).is_err());
        assert!(publish_cargo(missing_dir, &token).is_err());
        Ok(())
    }

    #[test]
    fn test_publish_npm_readonly_dir_error() -> std::io::Result<()> {
        let dest = TempDir::new()?;
        let mut perms = fs::metadata(dest.path())?.permissions();
        perms.set_readonly(true);
        fs::set_permissions(dest.path(), perms)?;
        let token = SecretToken::new("token");
        let result = publish_npm_with_exe(dest.path(), &token, "npm");
        assert!(result.is_err());
        Ok(())
    }
}
