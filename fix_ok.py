import re

def fix_file(filename):
    with open(filename, "r") as f:
        code = f.read()

    # Storage.rs
    code = code.replace("archive.extract(dest_dir)?;\n    } else {", "archive.extract(dest_dir)?;\n        Ok(())\n    } else {")
    code = code.replace("archive.unpack(dest_dir)?;\n    }}", "archive.unpack(dest_dir)?;\n        Ok(())\n    }")

    # worker.rs
    code = code.replace("return Err(PublisherError::Redis(e));\n            }", "return Err(PublisherError::Redis(e));\n            }\n            Ok(())")
    code = code.replace(".expect(\"Failed to await\");    }", ".expect(\"Failed to await\");\n        Ok(())\n    }")
    code = code.replace("self.acknowledge(&id).await.expect(\"Failed to await\");\n            }", "self.acknowledge(&id).await.expect(\"Failed to await\");\n            }\n            Ok(())")
    code = code.replace("self.acknowledge(&id).await.expect(\"Failed to await\");\n                }", "self.acknowledge(&id).await.expect(\"Failed to await\");\n                }\n                Ok(())")
    code = code.replace("publish_npm(dest_dir.path(), token)?;\n            }", "publish_npm(dest_dir.path(), token)?;\n                Ok(())\n            }")
    code = code.replace("publish_pypi(dest_dir.path(), token)?;\n            }", "publish_pypi(dest_dir.path(), token)?;\n                Ok(())\n            }")
    code = code.replace("publish_cargo(dest_dir.path(), token)?;\n            }", "publish_cargo(dest_dir.path(), token)?;\n                Ok(())\n            }")

    # publish.rs
    code = code.replace("return Err(PublisherError::Publish(format!(\"npm publish failed: {safe_stderr}\")));\n    }}", "return Err(PublisherError::Publish(format!(\"npm publish failed: {safe_stderr}\")));\n    }\n    Ok(())}")
    code = code.replace("return Err(PublisherError::Publish(format!(\"twine upload failed: {safe_stderr}\")));\n    }}", "return Err(PublisherError::Publish(format!(\"twine upload failed: {safe_stderr}\")));\n    }\n    Ok(())}")
    code = code.replace("return Err(PublisherError::Publish(format!(\"cargo publish failed: {safe_stderr}\")));\n    }}", "return Err(PublisherError::Publish(format!(\"cargo publish failed: {safe_stderr}\")));\n    }\n    Ok(())}")
    
    # Also publish.rs has create_dummy_exe ? that are used in tests but tests return ()
    # Just fix the tests to use expect instead of ?
    code = code.replace('let exe = create_dummy_exe(dest.path(), "npm", 0, Some("publish --access public"))?;', 'let exe = create_dummy_exe(dest.path(), "npm", 0, Some("publish --access public"));')
    code = code.replace('let exe = create_dummy_exe(dest.path(), "npm", 1, None)?;', 'let exe = create_dummy_exe(dest.path(), "npm", 1, None);')
    code = code.replace('let exe = create_dummy_exe(dest.path(), "twine", 0, Some("upload --non-interactive -u __token__ -p secret123 dist/*"))?;', 'let exe = create_dummy_exe(dest.path(), "twine", 0, Some("upload --non-interactive -u __token__ -p secret123 dist/*"));')
    code = code.replace('let exe = create_dummy_exe(dest.path(), "twine", 1, None)?;', 'let exe = create_dummy_exe(dest.path(), "twine", 1, None);')
    code = code.replace('let exe = create_dummy_exe(dest.path(), "cargo", 0, Some("publish --token secret123"))?;', 'let exe = create_dummy_exe(dest.path(), "cargo", 0, Some("publish --token secret123"));')
    code = code.replace('let exe = create_dummy_exe(dest.path(), "cargo", 1, None)?;', 'let exe = create_dummy_exe(dest.path(), "cargo", 1, None);')
    
    with open(filename, "w") as f:
        f.write(code)

for filename in ["src/storage.rs", "src/worker.rs", "src/publish.rs"]:
    fix_file(filename)

