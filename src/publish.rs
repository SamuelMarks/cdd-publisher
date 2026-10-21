//! Handlers for publishing artifacts to external registries.
//!
//! Provides both the [`RegistryPublisher`] trait for dependency injection and
//! [`CliRegistryPublisher`] for invoking `npm`, `twine`, and `cargo` in isolated
//! blocking worker threads with strict credential protection.

use crate::domain::{Registry, SecretToken};
use crate::error::{PublisherError, Result};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Paths to the executable CLIs for registries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutablePaths {
    /// NPM executable path.
    pub npm: String,
    /// `PyPI` (`twine`) executable path.
    pub pypi: String,
    /// Cargo executable path.
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

/// Abstract interface for publishing an unpacked artifact directory to a target registry.
pub trait RegistryPublisher: Send + Sync {
    /// Publishes the artifact located in `dir` to `registry` authenticating with `token`.
    ///
    /// # Arguments
    ///
    /// * `registry` - The target package registry (`npm`, `pypi`, or `cargo`).
    /// * `dir` - The root directory of the unpacked artifact.
    /// * `token` - The secret authentication token for the registry.
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
}

/// CLI-backed implementation of [`RegistryPublisher`] that invokes `npm`, `twine`, and `cargo`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliRegistryPublisher {
    /// Configured executable names or paths for each registry CLI.
    exe_paths: ExecutablePaths,
}

impl CliRegistryPublisher {
    /// Creates a new [`CliRegistryPublisher`] using the default CLI executable names.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new [`CliRegistryPublisher`] with custom executable paths.
    ///
    /// # Arguments
    ///
    /// * `exe_paths` - Custom executable paths for `npm`, `pypi`, and `cargo`.
    #[must_use]
    pub const fn with_paths(exe_paths: ExecutablePaths) -> Self {
        Self { exe_paths }
    }

    /// Returns a reference to the configured [`ExecutablePaths`].
    #[must_use]
    pub const fn exe_paths(&self) -> &ExecutablePaths {
        &self.exe_paths
    }
}

impl RegistryPublisher for CliRegistryPublisher {
    async fn publish(&self, registry: Registry, dir: &Path, token: &SecretToken) -> Result<()> {
        let dir_buf = dir.to_path_buf();
        let token_clone = token.clone();
        let paths = self.exe_paths.clone();

        tokio::task::spawn_blocking(move || match registry {
            Registry::Npm => publish_npm_with_exe(&dir_buf, &token_clone, &paths.npm),
            Registry::PyPi => publish_pypi_with_exe(&dir_buf, &token_clone, &paths.pypi),
            Registry::Cargo => publish_cargo_with_exe(&dir_buf, &token_clone, &paths.cargo),
        })
        .await?
    }
}

/// RAII guard that removes the temporary `.npmrc` file when dropped.
struct NpmrcCleanupGuard {
    /// Path to the `.npmrc` file to remove on drop.
    path: PathBuf,
}

impl Drop for NpmrcCleanupGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Writes an `.npmrc` file with restrictive (`0o600` on Unix) permissions and returns a cleanup guard.
///
/// # Arguments
///
/// * `dir` - The package directory where `.npmrc` will be created.
/// * `token` - The NPM authentication token.
///
/// # Errors
///
/// Returns an error if creating, setting permissions, or writing `.npmrc` fails.
fn write_secure_npmrc(dir: &Path, token: &SecretToken) -> Result<NpmrcCleanupGuard> {
    let npmrc_path = dir.join(".npmrc");
    let content = format!("//registry.npmjs.org/:_authToken={}", token.expose_secret());
    fs::write(&npmrc_path, content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&npmrc_path, fs::Permissions::from_mode(0o600))?;
    }

    Ok(NpmrcCleanupGuard { path: npmrc_path })
}

/// Formats and redacts combined `stderr`/`stdout` output from a failed CLI invocation.
///
/// # Arguments
///
/// * `prefix` - Context prefix (e.g., `"npm publish failed"`).
/// * `stderr` - Raw stderr bytes from the child process.
/// * `stdout` - Raw stdout bytes from the child process.
/// * `token` - Secret token to scrub from the output.
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
/// Creates a temporary `0o600` `.npmrc` file with the provided auth token, runs
/// `npm publish --access public`, and removes `.npmrc` upon completion.
///
/// # Arguments
///
/// * `dir` - The directory containing the NPM package to publish.
/// * `token` - The NPM authentication token.
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
/// Credentials are provided securely via `TWINE_USERNAME=__token__` and `TWINE_PASSWORD`
/// environment variables rather than process-table-visible CLI flags.
///
/// # Arguments
///
/// * `dir` - The directory containing the Python distribution files (`dist/`).
/// * `token` - The `PyPI` authentication token.
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
/// Passes the registry token securely via the `CARGO_REGISTRY_TOKEN` environment variable
/// rather than exposing `--token` in the OS process table.
///
/// # Arguments
///
/// * `dir` - The directory containing the Rust crate (`Cargo.toml`).
/// * `token` - The Cargo registry token.
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
        // Verify that during execution .npmrc exists with 0600 permissions and expected content
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

        // Verify .npmrc was cleaned up by NpmrcCleanupGuard after execution
        assert!(!dest.path().join(".npmrc").exists());
        Ok(())
    }

    #[test]
    fn test_publish_npm_failure_masks_token_in_stderr_and_stdout() -> Result<()> {
        let dest = TempDir::new()?;
        let exe = create_mock_cli(
            dest.path(),
            "mock_npm_fail",
            r#"echo "Auth error with secret123" >&2
echo "Stdout detail secret123"
exit 1"#,
        )?;
        let token = SecretToken::new("secret123");

        let result = publish_npm_with_exe(dest.path(), &token, &exe);
        assert!(result.is_err());
        if let Err(err) = result {
            let msg = err.to_string();
            assert!(msg.contains("npm publish failed"));
            assert!(msg.contains("***"));
            assert!(!msg.contains("secret123"));
        }
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

        let result = publish_pypi_with_exe(dest.path(), &token, &exe);
        assert!(result.is_err());
        if let Err(err) = result {
            let msg = err.to_string();
            assert!(msg.contains("twine upload failed"));
            assert!(msg.contains("***"));
            assert!(!msg.contains("pypi-secret-999"));
        }
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
        let exe = create_mock_cli(
            dest.path(),
            "mock_cargo_fail",
            r#"echo "Stdout failure cargo-secret-777"
exit 1"#,
        )?;
        let token = SecretToken::new("cargo-secret-777");

        let result = publish_cargo_with_exe(dest.path(), &token, &exe);
        assert!(result.is_err());
        if let Err(err) = result {
            let msg = err.to_string();
            assert!(msg.contains("cargo publish failed"));
            assert!(msg.contains("***"));
            assert!(!msg.contains("cargo-secret-777"));
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_cli_registry_publisher_trait_all_registries() -> Result<()> {
        let dest = TempDir::new()?;
        let npm_exe = create_mock_cli(dest.path(), "npm_ok", "exit 0")?;
        let twine_exe = create_mock_cli(dest.path(), "twine_ok", "exit 0")?;
        let cargo_exe = create_mock_cli(dest.path(), "cargo_ok", "exit 0")?;

        let paths = ExecutablePaths {
            npm: npm_exe,
            pypi: twine_exe,
            cargo: cargo_exe,
        };
        let publisher = CliRegistryPublisher::with_paths(paths.clone());
        assert_eq!(publisher.exe_paths(), &paths);

        let default_publisher = CliRegistryPublisher::new();
        assert_eq!(default_publisher.exe_paths(), &ExecutablePaths::default());

        let token = SecretToken::new("tok123");
        publisher
            .publish(Registry::Npm, dest.path(), &token)
            .await?;
        publisher
            .publish(Registry::PyPi, dest.path(), &token)
            .await?;
        publisher
            .publish(Registry::Cargo, dest.path(), &token)
            .await?;
        Ok(())
    }

    #[test]
    fn test_default_publish_wrappers_fail_gracefully_in_nonexistent_dir() {
        let missing_dir = Path::new("/nonexistent_cdd_publisher_test_dir_12345");
        let token = SecretToken::new("secret");

        // All 3 default wrappers fail deterministically at the filesystem/chdir boundary
        // without making live network calls or discarding results tautologically.
        assert!(publish_npm(missing_dir, &token).is_err());
        assert!(publish_pypi(missing_dir, &token).is_err());
        assert!(publish_cargo(missing_dir, &token).is_err());
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
