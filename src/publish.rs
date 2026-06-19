//! Handlers for publishing artifacts to external registries.

use crate::error::{PublisherError, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Publishes to NPM.
///
/// Creates an `.npmrc` file with the provided auth token and runs `npm publish --access public`.
///
/// # Errors
///
/// Returns an error if writing the `.npmrc` file fails, if the command cannot be executed,
/// or if `npm publish` returns a non-zero exit code.
pub fn publish_npm(dir: &Path, token: &str) -> Result<()> {
    publish_npm_with_exe(dir, token, "npm")
}

/// Internal testable implementation of NPM publish that accepts an executable path.
///
/// # Errors
/// Returns an error on I/O or command execution failure.
pub fn publish_npm_with_exe(dir: &Path, token: &str, exe: &str) -> Result<()> {
    let npmrc_path = dir.join(".npmrc");
    fs::write(
        &npmrc_path,
        format!("//registry.npmjs.org/:_authToken={token}"),
    )?;

    let output = Command::new(exe)
        .arg("publish")
        .arg("--access")
        .arg("public")
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let safe_stderr = stderr.replace(token, "***");
        return Err(PublisherError::Publish(format!(
            "npm publish failed: {safe_stderr}"
        )));
    }
    Ok(())
}

/// Publishes to `PyPI`.
///
/// Uses `twine upload --non-interactive -u __token__ -p <token> dist/*`.
///
/// # Errors
///
/// Returns an error if the command cannot be executed, or if `twine upload` returns a non-zero exit code.
pub fn publish_pypi(dir: &Path, token: &str) -> Result<()> {
    publish_pypi_with_exe(dir, token, "twine")
}

/// Internal testable implementation of `PyPI` publish that accepts an executable path.
///
/// # Errors
/// Returns an error on command execution failure.
pub fn publish_pypi_with_exe(dir: &Path, token: &str, exe: &str) -> Result<()> {
    let output = Command::new(exe)
        .arg("upload")
        .arg("--non-interactive")
        .arg("-u")
        .arg("__token__")
        .arg("-p")
        .arg(token)
        .arg("dist/*")
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let safe_stderr = stderr.replace(token, "***");
        return Err(PublisherError::Publish(format!(
            "twine upload failed: {safe_stderr}"
        )));
    }
    Ok(())
}

/// Publishes to Cargo.
///
/// Uses `cargo publish --token <token>`.
///
/// # Errors
///
/// Returns an error if the command cannot be executed, or if `cargo publish` returns a non-zero exit code.
pub fn publish_cargo(dir: &Path, token: &str) -> Result<()> {
    publish_cargo_with_exe(dir, token, "cargo")
}

/// Internal testable implementation of Cargo publish that accepts an executable path.
///
/// # Errors
/// Returns an error on command execution failure.
pub fn publish_cargo_with_exe(dir: &Path, token: &str, exe: &str) -> Result<()> {
    let output = Command::new(exe)
        .arg("publish")
        .arg("--token")
        .arg(token)
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let safe_stderr = stderr.replace(token, "***");
        return Err(PublisherError::Publish(format!(
            "cargo publish failed: {safe_stderr}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn create_dummy_exe(
        dir: &Path,
        name: &str,
        exit_code: i32,
        expected_args: Option<&str>,
    ) -> std::path::PathBuf {
        let exe_path = dir.join(name);

        let script = expected_args.map_or_else(
            || {
                format!(
                    "#!/bin/sh\n\
                     echo \"Simulated error with secret123\" >&2\n\
                     exit {exit_code}\n"
                )
            },
            |args| {
                format!(
                    "#!/bin/sh\n\
                     if [ \"$*\" != \"{args}\" ]; then\n\
                        echo \"Unexpected arguments: $*\" >&2\n\
                        exit 1\n\
                     fi\n\
                     echo \"Simulated error with secret123\" >&2\n\
                     exit {exit_code}\n"
                )
            },
        );

        fs::write(&exe_path, script).unwrap_or_else(|_| panic!("Failed to write mock exe"));
        fs::set_permissions(&exe_path, fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|_| panic!("Failed to set perms"));
        exe_path
    }

    #[test]
    fn test_publish_npm_success() {
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));
        let exe = create_dummy_exe(dest.path(), "npm", 0, Some("publish --access public"));

        let result = publish_npm_with_exe(
            dest.path(),
            "secret123",
            exe.to_str().unwrap_or_else(|| panic!("Valid path")),
        );
        assert!(result.is_ok());

        let npmrc = fs::read_to_string(dest.path().join(".npmrc"))
            .unwrap_or_else(|_| panic!("Failed to read .npmrc"));
        assert_eq!(npmrc, "//registry.npmjs.org/:_authToken=secret123");
    }

    #[test]
    fn test_publish_npm_failure_masks_token() {
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));
        let exe = create_dummy_exe(dest.path(), "npm", 1, None);

        let result = publish_npm_with_exe(
            dest.path(),
            "secret123",
            exe.to_str().unwrap_or_else(|| panic!("Valid path")),
        );
        let e = result.map_or_else(|e| e, |()| panic!("Expected error"));
        let err_msg = e.to_string();
        assert!(err_msg.contains("npm publish failed"));
        assert!(err_msg.contains("***"));
        assert!(!err_msg.contains("secret123"));
    }

    #[test]
    fn test_publish_pypi_success() {
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));
        let exe = create_dummy_exe(
            dest.path(),
            "twine",
            0,
            Some("upload --non-interactive -u __token__ -p secret123 dist/*"),
        );

        let result = publish_pypi_with_exe(
            dest.path(),
            "secret123",
            exe.to_str().unwrap_or_else(|| panic!("Valid path")),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_publish_pypi_failure_masks_token() {
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));
        let exe = create_dummy_exe(dest.path(), "twine", 1, None);

        let result = publish_pypi_with_exe(
            dest.path(),
            "secret123",
            exe.to_str().unwrap_or_else(|| panic!("Valid path")),
        );
        let e = result.map_or_else(|e| e, |()| panic!("Expected error"));
        let err_msg = e.to_string();
        assert!(err_msg.contains("twine upload failed"));
        assert!(err_msg.contains("***"));
        assert!(!err_msg.contains("secret123"));
    }

    #[test]
    fn test_publish_cargo_success() {
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));
        let exe = create_dummy_exe(dest.path(), "cargo", 0, Some("publish --token secret123"));

        let result = publish_cargo_with_exe(
            dest.path(),
            "secret123",
            exe.to_str().unwrap_or_else(|| panic!("Valid path")),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_publish_cargo_failure_masks_token() {
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));
        let exe = create_dummy_exe(dest.path(), "cargo", 1, None);

        let result = publish_cargo_with_exe(
            dest.path(),
            "secret123",
            exe.to_str().unwrap_or_else(|| panic!("Valid path")),
        );
        let e = result.map_or_else(|e| e, |()| panic!("Expected error"));
        let err_msg = e.to_string();
        assert!(err_msg.contains("cargo publish failed"));
        assert!(err_msg.contains("***"));
        assert!(!err_msg.contains("secret123"));
    }

    #[test]
    fn test_default_publish_functions() {
        let dest = TempDir::new().unwrap_or_else(|_| panic!("Failed to create TempDir"));
        let _ = publish_npm(dest.path(), "test");
        let _ = publish_pypi(dest.path(), "test");
        let _ = publish_cargo(dest.path(), "test");
    }

    #[test]
    fn test_publish_npm_io_error() -> std::io::Result<()> {
        let dest = TempDir::new()?;
        let mut perms = std::fs::metadata(dest.path())?.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(dest.path(), perms)?;
        let result = publish_npm_with_exe(dest.path(), "token", "npm");
        assert!(result.is_err());
        Ok(())
    }
}
