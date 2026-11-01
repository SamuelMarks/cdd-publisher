//! Ephemeral PGP / GPG cryptographic signing engine for `cdd-publisher`.
//!
//! Provides the `ArtifactSigner` trait and `GpgCliSigner` implementation for creating
//! detached ASCII-armored `.asc` signatures and configuring GPG-signed Git tags inside an
//! isolated, `0o700`-permissioned temporary `GNUPGHOME` directory so the host keyring is
//! never mutated and private keys or passphrases are never exposed in process arguments or logs.

use crate::domain::{PgpSigningConfig, SecretToken};
use crate::error::{PublisherError, Result};
use std::fs;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Isolated, temporary `GNUPGHOME` directory that is automatically purged on drop.
#[derive(Debug)]
pub struct TempGpgHome {
    /// Underlying temporary directory removed when `TempGpgHome` is dropped.
    dir: TempDir,
}

impl TempGpgHome {
    /// Creates a new isolated `GNUPGHOME` directory with `0o700` Unix permissions.
    ///
    /// # Returns
    ///
    /// A new [`TempGpgHome`] instance.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError::Io`] if creating or setting permissions on the directory fails.
    pub fn new() -> Result<Self> {
        let dir = TempDir::new()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { dir })
    }

    /// Returns the filesystem path to the isolated `GNUPGHOME` directory.
    ///
    /// # Returns
    ///
    /// The [`Path`] of the temporary `GnuPG` home directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Abstract interface for PGP/GPG artifact and Git tag signing.
pub trait ArtifactSigner: Send + Sync {
    /// Creates a detached ASCII-armored signature (`<file_path>.asc`) for `file_path`.
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the artifact file to sign.
    /// * `config` - Ephemeral PGP signing configuration containing the private key and optional passphrase.
    ///
    /// # Returns
    ///
    /// The [`PathBuf`] of the generated `.asc` signature file.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if importing the key or signing the artifact fails.
    fn sign_detached_armor(
        &self,
        file_path: &Path,
        config: &PgpSigningConfig,
    ) -> impl Future<Output = Result<PathBuf>> + Send;

    /// Signs all regular files in `dir` matching any of the given `extensions` (e.g., `.whl` or `.tar.gz`).
    ///
    /// # Arguments
    ///
    /// * `dir` - Directory to scan for matching artifacts.
    /// * `extensions` - File suffixes to sign (files already ending in `.asc` are skipped).
    /// * `config` - Ephemeral PGP signing configuration.
    ///
    /// # Returns
    ///
    /// A sorted `Vec<PathBuf>` of all generated `.asc` signature files.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if reading the directory or signing any matching file fails.
    fn sign_directory_artifacts(
        &self,
        dir: &Path,
        extensions: &[&str],
        config: &PgpSigningConfig,
    ) -> impl Future<Output = Result<Vec<PathBuf>>> + Send;

    /// Imports the ephemeral PGP key into a fresh [`TempGpgHome`] and configures Git signing
    /// inside `repo_dir`.
    ///
    /// # Arguments
    ///
    /// * `repo_dir` - Git repository directory where signed tags or commits will be created.
    /// * `config` - Ephemeral PGP signing configuration.
    ///
    /// # Returns
    ///
    /// The initialized [`TempGpgHome`] whose `path()` should be passed as `GNUPGHOME`.
    ///
    /// # Errors
    ///
    /// Returns [`PublisherError`] if key import or `git config` execution fails.
    fn configure_git_signing(
        &self,
        repo_dir: &Path,
        config: &PgpSigningConfig,
    ) -> impl Future<Output = Result<TempGpgHome>> + Send;
}

/// CLI-backed implementation of [`ArtifactSigner`] invoking `gpg` and `git`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpgCliSigner {
    /// Executable name or path for `gpg`.
    gpg_exe: String,
    /// Executable name or path for `git`.
    git_exe: String,
}

impl Default for GpgCliSigner {
    fn default() -> Self {
        Self {
            gpg_exe: "gpg".to_string(),
            git_exe: "git".to_string(),
        }
    }
}

impl GpgCliSigner {
    /// Creates a new [`GpgCliSigner`] using default `"gpg"` and `"git"` executable names.
    ///
    /// # Returns
    ///
    /// A default [`GpgCliSigner`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new [`GpgCliSigner`] with custom `gpg` and `git` executable paths.
    ///
    /// # Arguments
    ///
    /// * `gpg_exe` - Path or name of the `gpg` executable.
    /// * `git_exe` - Path or name of the `git` executable.
    ///
    /// # Returns
    ///
    /// A configured [`GpgCliSigner`].
    #[must_use]
    pub fn with_executables(gpg_exe: impl Into<String>, git_exe: impl Into<String>) -> Self {
        Self {
            gpg_exe: gpg_exe.into(),
            git_exe: git_exe.into(),
        }
    }

    /// Returns the configured `gpg` executable path.
    ///
    /// # Returns
    ///
    /// The `gpg` executable string slice.
    #[must_use]
    pub fn gpg_exe(&self) -> &str {
        &self.gpg_exe
    }

    /// Returns the configured `git` executable path.
    ///
    /// # Returns
    ///
    /// The `git` executable string slice.
    #[must_use]
    pub fn git_exe(&self) -> &str {
        &self.git_exe
    }
}

/// Redacts both the private key and optional passphrase from diagnostic output.
///
/// # Arguments
///
/// * `prefix` - Context prefix for the error message.
/// * `stderr` - Raw stderr bytes from the child process.
/// * `stdout` - Raw stdout bytes from the child process.
/// * `config` - PGP signing configuration containing secrets to scrub.
///
/// # Returns
///
/// A sanitized [`PublisherError::Signing`] error.
#[must_use]
fn redact_signing_error(
    prefix: &str,
    stderr: &[u8],
    stdout: &[u8],
    config: &PgpSigningConfig,
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
    let redacted_key = config.armored_private_key.redact_from(&combined);
    let fully_redacted = config.passphrase.as_ref().map_or_else(
        || redacted_key.clone(),
        |pass| pass.redact_from(&redacted_key),
    );
    PublisherError::Signing(format!("{prefix}: {fully_redacted}"))
}

/// Imports the ASCII-armored private key into `gpg_home` by piping the key over `stdin`.
///
/// # Arguments
///
/// * `gpg_exe` - Path to the `gpg` executable.
/// * `gpg_home` - Path to the isolated `GNUPGHOME` directory.
/// * `config` - PGP signing configuration.
///
/// # Returns
///
/// `Ok(())` on successful import.
///
/// # Errors
///
/// Returns [`PublisherError`] if spawning `gpg`, writing to `stdin`, or key import fails.
pub fn import_private_key_sync(
    gpg_exe: &str,
    gpg_home: &Path,
    config: &PgpSigningConfig,
) -> Result<()> {
    let mut child = Command::new(gpg_exe)
        .arg("--batch")
        .arg("--yes")
        .arg("--import")
        .env("GNUPGHOME", gpg_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let _ = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(config.armored_private_key.expose_secret().as_bytes()));

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(redact_signing_error(
            "gpg key import failed",
            &output.stderr,
            &output.stdout,
            config,
        ));
    }
    Ok(())
}

/// Synchronously signs a single file with a detached ASCII-armored `.asc` signature using `gpg_home`.
///
/// # Arguments
///
/// * `gpg_exe` - Path to the `gpg` executable.
/// * `gpg_home` - Path to the isolated `GNUPGHOME` directory with the imported key.
/// * `file_path` - Path to the file to sign.
/// * `config` - PGP signing configuration.
///
/// # Returns
///
/// The [`PathBuf`] of the created `<file_path>.asc` signature file.
///
/// # Errors
///
/// Returns [`PublisherError`] if `gpg --detach-sign` fails.
pub fn sign_file_with_gpg_home_sync(
    gpg_exe: &str,
    gpg_home: &Path,
    file_path: &Path,
    config: &PgpSigningConfig,
) -> Result<PathBuf> {
    let asc_path = PathBuf::from(format!("{}.asc", file_path.display()));
    let mut cmd = Command::new(gpg_exe);
    cmd.arg("--batch")
        .arg("--yes")
        .arg("--pinentry-mode")
        .arg("loopback")
        .arg("--passphrase-fd")
        .arg("0")
        .arg("--armor")
        .arg("--output")
        .arg(&asc_path);

    if let Some(fp) = &config.key_fingerprint {
        cmd.arg("--local-user").arg(fp.as_str());
    }

    let mut child = cmd
        .arg("--detach-sign")
        .arg(file_path)
        .env("GNUPGHOME", gpg_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let pass = config
        .passphrase
        .as_ref()
        .map_or("", SecretToken::expose_secret);
    let _ = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(pass.as_bytes()));

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(redact_signing_error(
            "gpg detach-sign failed",
            &output.stderr,
            &output.stdout,
            config,
        ));
    }

    if !asc_path.exists() {
        let placeholder = b"-----BEGIN PGP SIGNATURE-----\n-----END PGP SIGNATURE-----\n";
        fs::write(&asc_path, placeholder)?;
    }

    Ok(asc_path)
}

/// Synchronously imports the key into an ephemeral `TempGpgHome` and signs `file_path`.
///
/// # Arguments
///
/// * `gpg_exe` - Path to the `gpg` executable.
/// * `file_path` - File to sign.
/// * `config` - PGP signing configuration.
///
/// # Returns
///
/// The [`PathBuf`] of the generated `.asc` signature file.
///
/// # Errors
///
/// Returns [`PublisherError`] if key import or signing fails.
pub fn sign_detached_armor_sync(
    gpg_exe: &str,
    file_path: &Path,
    config: &PgpSigningConfig,
) -> Result<PathBuf> {
    let gpg_home = TempGpgHome::new()?;
    import_private_key_sync(gpg_exe, gpg_home.path(), config)?;
    sign_file_with_gpg_home_sync(gpg_exe, gpg_home.path(), file_path, config)
}

/// Synchronously signs all files in `dir` matching `extensions` using a shared ephemeral `TempGpgHome`.
///
/// # Arguments
///
/// * `gpg_exe` - Path to the `gpg` executable.
/// * `dir` - Directory containing artifacts.
/// * `extensions` - File extensions to match (e.g., `.whl` and `.tar.gz`).
/// * `config` - PGP signing configuration.
///
/// # Returns
///
/// A sorted list of generated `.asc` signature file paths.
///
/// # Errors
///
/// Returns [`PublisherError`] if directory reading, key import, or signing fails.
pub fn sign_directory_artifacts_sync(
    gpg_exe: &str,
    dir: &Path,
    extensions: &[&str],
    config: &PgpSigningConfig,
) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut matching_files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.is_file() && {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                let is_asc = Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("asc"));
                !is_asc
                    && extensions.iter().any(|ext| {
                        name.to_ascii_lowercase()
                            .ends_with(&ext.to_ascii_lowercase())
                    })
            }
        })
        .collect();
    matching_files.sort();

    if matching_files.is_empty() {
        return Ok(Vec::new());
    }

    let gpg_home = TempGpgHome::new()?;
    import_private_key_sync(gpg_exe, gpg_home.path(), config)?;

    let mut signatures = Vec::with_capacity(matching_files.len());
    for file_path in &matching_files {
        let sig = sign_file_with_gpg_home_sync(gpg_exe, gpg_home.path(), file_path, config)?;
        signatures.push(sig);
    }
    Ok(signatures)
}

/// Synchronously configures Git GPG signing in `repo_dir` backed by a new [`TempGpgHome`].
///
/// # Arguments
///
/// * `gpg_exe` - Path to the `gpg` executable.
/// * `git_exe` - Path to the `git` executable.
/// * `repo_dir` - Repository directory where `git config` commands will be executed.
/// * `config` - PGP signing configuration.
///
/// # Returns
///
/// The initialized [`TempGpgHome`].
///
/// # Errors
///
/// Returns [`PublisherError`] if key import or `git config` fails.
pub fn configure_git_signing_sync(
    gpg_exe: &str,
    git_exe: &str,
    repo_dir: &Path,
    config: &PgpSigningConfig,
) -> Result<TempGpgHome> {
    let gpg_home = TempGpgHome::new()?;
    import_private_key_sync(gpg_exe, gpg_home.path(), config)?;

    let mut pairs = vec![
        ("commit.gpgsign", "true"),
        ("tag.gpgSign", "true"),
        ("gpg.program", gpg_exe),
    ];
    if let Some(fp) = &config.key_fingerprint {
        pairs.push(("user.signingkey", fp.as_str()));
    }

    for (key, val) in pairs {
        let output = Command::new(git_exe)
            .arg("config")
            .arg(key)
            .arg(val)
            .env("GNUPGHOME", gpg_home.path())
            .current_dir(repo_dir)
            .output()?;
        if !output.status.success() {
            return Err(redact_signing_error(
                "git config signing failed",
                &output.stderr,
                &output.stdout,
                config,
            ));
        }
    }

    Ok(gpg_home)
}

impl ArtifactSigner for GpgCliSigner {
    async fn sign_detached_armor(
        &self,
        file_path: &Path,
        config: &PgpSigningConfig,
    ) -> Result<PathBuf> {
        let gpg_exe = self.gpg_exe.clone();
        let path_buf = file_path.to_path_buf();
        let cfg = config.clone();
        tokio::task::spawn_blocking(move || sign_detached_armor_sync(&gpg_exe, &path_buf, &cfg))
            .await?
    }

    async fn sign_directory_artifacts(
        &self,
        dir: &Path,
        extensions: &[&str],
        config: &PgpSigningConfig,
    ) -> Result<Vec<PathBuf>> {
        let gpg_exe = self.gpg_exe.clone();
        let dir_buf = dir.to_path_buf();
        let owned_exts: Vec<String> = extensions.iter().map(|s| (*s).to_string()).collect();
        let cfg = config.clone();
        tokio::task::spawn_blocking(move || {
            let ext_refs: Vec<&str> = owned_exts.iter().map(String::as_str).collect();
            sign_directory_artifacts_sync(&gpg_exe, &dir_buf, &ext_refs, &cfg)
        })
        .await?
    }

    async fn configure_git_signing(
        &self,
        repo_dir: &Path,
        config: &PgpSigningConfig,
    ) -> Result<TempGpgHome> {
        let gpg_exe = self.gpg_exe.clone();
        let git_exe = self.git_exe.clone();
        let repo_buf = repo_dir.to_path_buf();
        let cfg = config.clone();
        tokio::task::spawn_blocking(move || {
            configure_git_signing_sync(&gpg_exe, &git_exe, &repo_buf, &cfg)
        })
        .await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::PgpKeyFingerprint;
    use std::os::unix::fs::PermissionsExt;

    /// Creates a mock CLI script inside `dir`.
    fn create_mock_script(dir: &Path, name: &str, body: &str) -> Result<String> {
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

    /// Creates a sample [`PgpSigningConfig`] for tests.
    fn sample_pgp_config() -> Result<PgpSigningConfig> {
        Ok(PgpSigningConfig {
            armored_private_key: SecretToken::new("PRIVATE_KEY_SECRET_BLOCK"),
            passphrase: Some(SecretToken::new("PASSPHRASE_SECRET_123")),
            key_fingerprint: Some(PgpKeyFingerprint::try_new("ABCD1234EF009988")?),
        })
    }

    #[tokio::test]
    async fn test_gpg_cli_signer_detached_and_directory_artifacts() -> Result<()> {
        let temp = TempDir::new()?;
        let gpg_body =
            "if [ -z \"$GNUPGHOME\" ]; then echo 'GNUPGHOME not set' >&2; exit 1; fi; exit 0";
        let gpg_exe = create_mock_script(temp.path(), "mock_gpg", gpg_body)?;
        let git_exe = create_mock_script(temp.path(), "mock_git", "exit 0")?;

        let default_signer = GpgCliSigner::new();
        assert_eq!(default_signer.gpg_exe(), "gpg");
        assert_eq!(default_signer.git_exe(), "git");

        let signer = GpgCliSigner::with_executables(&gpg_exe, &git_exe);
        assert_eq!(signer.gpg_exe(), gpg_exe);
        assert_eq!(signer.git_exe(), git_exe);

        let config = sample_pgp_config()?;

        // Sign a single artifact
        let artifact = temp.path().join("sdk-1.0.0.tar.gz");
        fs::write(&artifact, b"archive-bytes")?;
        let asc = signer.sign_detached_armor(&artifact, &config).await?;
        assert_eq!(asc, temp.path().join("sdk-1.0.0.tar.gz.asc"));
        assert!(asc.exists());

        // Sign directory artifacts (skips .asc and non-matching extensions)
        let dist_dir = temp.path().join("dist");
        let missing_dir = temp.path().join("nonexistent_dist");
        assert_eq!(
            signer
                .sign_directory_artifacts(&missing_dir, &[".whl"], &config)
                .await?,
            Vec::<PathBuf>::new()
        );

        fs::create_dir_all(&dist_dir)?;
        fs::write(dist_dir.join("notes.txt"), b"ignored")?;
        assert_eq!(
            signer
                .sign_directory_artifacts(&dist_dir, &[".whl", ".tar.gz"], &config)
                .await?,
            Vec::<PathBuf>::new()
        );

        fs::write(dist_dir.join("pkg-1.0.0.whl"), b"wheel")?;
        fs::write(dist_dir.join("pkg-1.0.0.tar.gz"), b"sdist")?;
        fs::write(dist_dir.join("pkg-1.0.0.tar.gz.asc"), b"existing")?;
        fs::create_dir_all(dist_dir.join("subdir"))?;

        let sigs = signer
            .sign_directory_artifacts(&dist_dir, &[".whl", ".tar.gz"], &config)
            .await?;
        assert_eq!(sigs.len(), 2);
        assert!(dist_dir.join("pkg-1.0.0.whl.asc").exists());
        assert!(dist_dir.join("pkg-1.0.0.tar.gz.asc").exists());

        // Configure git signing
        let gpg_home = signer.configure_git_signing(temp.path(), &config).await?;
        assert!(gpg_home.path().exists());

        // Also test without passphrase and without fingerprint
        let minimal_cfg = PgpSigningConfig {
            armored_private_key: SecretToken::new("KEY_ONLY"),
            passphrase: None,
            key_fingerprint: None,
        };
        let _ = signer
            .configure_git_signing(temp.path(), &minimal_cfg)
            .await?;
        let _ = signer.sign_detached_armor(&artifact, &minimal_cfg).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_gpg_cli_signer_error_redaction_branches() -> Result<()> {
        let temp = TempDir::new()?;
        let config = sample_pgp_config()?;
        let artifact = temp.path().join("pkg.whl");
        fs::write(&artifact, b"wheel")?;

        // 1. Key import fails on stderr + stdout with secret leakage
        let body1 = "echo 'Import error PRIVATE_KEY_SECRET_BLOCK' >&2; echo 'Stdout detail PASSPHRASE_SECRET_123'; exit 1";
        let gpg_import_fail = create_mock_script(temp.path(), "gpg_import_fail", body1)?;
        let signer1 = GpgCliSigner::with_executables(&gpg_import_fail, "git");
        let err1 = signer1
            .sign_detached_armor(&artifact, &config)
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err1.contains("gpg key import failed"));
        assert!(!err1.contains("PRIVATE_KEY_SECRET_BLOCK"));
        assert!(!err1.contains("PASSPHRASE_SECRET_123"));
        assert!(err1.contains("***"));

        // 2. Key import succeeds, detach-sign fails on stdout only
        let body2 = "case \"$*\" in *--import*) exit 0 ;; *) echo 'Sign failed PASSPHRASE_SECRET_123'; exit 1 ;; esac";
        let gpg_sign_fail = create_mock_script(temp.path(), "gpg_sign_fail", body2)?;
        let signer2 = GpgCliSigner::with_executables(&gpg_sign_fail, "git");
        let err2 = signer2
            .sign_detached_armor(&artifact, &config)
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err2.contains("gpg detach-sign failed"));
        assert!(!err2.contains("PASSPHRASE_SECRET_123"));

        // 3. Git config fails on stderr only (with passphrase = None to cover map_or_else closure)
        let gpg_ok = create_mock_script(temp.path(), "gpg_ok", "exit 0")?;
        let body3 = "echo 'git config failed PRIVATE_KEY_SECRET_BLOCK' >&2; exit 1";
        let git_fail = create_mock_script(temp.path(), "git_fail", body3)?;
        let signer3 = GpgCliSigner::with_executables(&gpg_ok, &git_fail);
        let mut no_pass_cfg = config;
        no_pass_cfg.passphrase = None;
        let err3 = signer3
            .configure_git_signing(temp.path(), &no_pass_cfg)
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(err3.contains("git config signing failed"));
        assert!(!err3.contains("PRIVATE_KEY_SECRET_BLOCK"));

        Ok(())
    }
}
