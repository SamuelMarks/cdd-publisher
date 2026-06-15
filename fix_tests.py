import re
with open("src/publish.rs", "r") as f:
    code = f.read()

tests_block = """mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn create_dummy_exe(dir: &Path, name: &str, exit_code: i32, expected_args: Option<&str>) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let exe_path = dir.join(name);
        
        let script = if let Some(args) = expected_args {
            format!(
                "#!/bin/sh\n\
                 if [ \\"$*\\" != \\"{args}\\" ]; then\n\
                    echo \\"Unexpected arguments: $*\\" >&2\n\
                    exit 1\n\
                 fi\n\
                 echo \\"Simulated error with secret123\\" >&2\n\
                 exit {exit_code}\n"
            )
        } else {
            format!(
                "#!/bin/sh\n\
                 echo \\"Simulated error with secret123\\" >&2\n\
                 exit {exit_code}\n"
            )
        };

        fs::write(&exe_path, script)?;
        fs::set_permissions(&exe_path, fs::Permissions::from_mode(0o755))?;
        Ok(exe_path)
    }

    #[test]
    fn test_publish_npm_success() -> Result<(), Box<dyn std::error::Error>> {
        let dest = TempDir::new()?;
        let exe = create_dummy_exe(dest.path(), "npm", 0, Some("publish --access public"))?;
        
        let result = publish_npm_with_exe(dest.path(), "secret123", exe.to_str().ok_or("Valid path")?);
        assert!(result.is_ok());

        let npmrc = fs::read_to_string(dest.path().join(".npmrc"))?;
        assert_eq!(npmrc, "//registry.npmjs.org/:_authToken=secret123");
        Ok(())
    }

    #[test]
    fn test_publish_npm_failure_masks_token() -> Result<(), Box<dyn std::error::Error>> {
        let dest = TempDir::new()?;
        let exe = create_dummy_exe(dest.path(), "npm", 1, None)?;
        
        let result = publish_npm_with_exe(dest.path(), "secret123", exe.to_str().ok_or("Valid path")?);
        assert!(result.is_err());
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(err_msg.contains("npm publish failed"));
            assert!(err_msg.contains("***"));
            assert!(!err_msg.contains("secret123"));
        }
        Ok(())
    }

    #[test]
    fn test_publish_pypi_success() -> Result<(), Box<dyn std::error::Error>> {
        let dest = TempDir::new()?;
        let exe = create_dummy_exe(dest.path(), "twine", 0, Some("upload --non-interactive -u __token__ -p secret123 dist/*"))?;
        
        let result = publish_pypi_with_exe(dest.path(), "secret123", exe.to_str().ok_or("Valid path")?);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_publish_pypi_failure_masks_token() -> Result<(), Box<dyn std::error::Error>> {
        let dest = TempDir::new()?;
        let exe = create_dummy_exe(dest.path(), "twine", 1, None)?;
        
        let result = publish_pypi_with_exe(dest.path(), "secret123", exe.to_str().ok_or("Valid path")?);
        assert!(result.is_err());
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(err_msg.contains("twine upload failed"));
            assert!(err_msg.contains("***"));
            assert!(!err_msg.contains("secret123"));
        }
        Ok(())
    }

    #[test]
    fn test_publish_cargo_success() -> Result<(), Box<dyn std::error::Error>> {
        let dest = TempDir::new()?;
        let exe = create_dummy_exe(dest.path(), "cargo", 0, Some("publish --token secret123"))?;
        
        let result = publish_cargo_with_exe(dest.path(), "secret123", exe.to_str().ok_or("Valid path")?);
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn test_publish_cargo_failure_masks_token() -> Result<(), Box<dyn std::error::Error>> {
        let dest = TempDir::new()?;
        let exe = create_dummy_exe(dest.path(), "cargo", 1, None)?;
        
        let result = publish_cargo_with_exe(dest.path(), "secret123", exe.to_str().ok_or("Valid path")?);
        assert!(result.is_err());
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(err_msg.contains("cargo publish failed"));
            assert!(err_msg.contains("***"));
            assert!(!err_msg.contains("secret123"));
        }
        Ok(())
    }

    #[test]
    fn test_default_publish_functions() -> Result<(), Box<dyn std::error::Error>> {
        let dest = TempDir::new()?;
        let _ = publish_npm(dest.path(), "test");
        let _ = publish_pypi(dest.path(), "test");
        let _ = publish_cargo(dest.path(), "test");
        Ok(())
    }
}"""

# Find mod tests
idx = code.find("mod tests {")
if idx != -1:
    code = code[:idx] + tests_block

with open("src/publish.rs", "w") as f:
    f.write(code)

