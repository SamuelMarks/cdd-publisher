import re

with open("src/publish.rs", "r") as f:
    code = f.read()

# Fix .? to ?
code = code.replace(".?", "?")
code = code.replace('exe.to_str()?', 'exe.to_str().ok_or("Valid path")?')

# Add Result return types
code = re.sub(r'fn create_dummy_exe\((.*?)\) -> std::path::PathBuf \{', r'fn create_dummy_exe(\1) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {', code)
code = re.sub(r'exe_path\n    \}', r'Ok(exe_path)\n    }', code)

code = re.sub(r'fn test_publish_npm_success\(\) \{', r'fn test_publish_npm_success() -> Result<(), Box<dyn std::error::Error>> {', code)
code = re.sub(r'fn test_publish_npm_failure_masks_token\(\) \{', r'fn test_publish_npm_failure_masks_token() -> Result<(), Box<dyn std::error::Error>> {', code)
code = re.sub(r'fn test_publish_pypi_success\(\) \{', r'fn test_publish_pypi_success() -> Result<(), Box<dyn std::error::Error>> {', code)
code = re.sub(r'fn test_publish_pypi_failure_masks_token\(\) \{', r'fn test_publish_pypi_failure_masks_token() -> Result<(), Box<dyn std::error::Error>> {', code)
code = re.sub(r'fn test_publish_cargo_success\(\) \{', r'fn test_publish_cargo_success() -> Result<(), Box<dyn std::error::Error>> {', code)
code = re.sub(r'fn test_publish_cargo_failure_masks_token\(\) \{', r'fn test_publish_cargo_failure_masks_token() -> Result<(), Box<dyn std::error::Error>> {', code)
code = re.sub(r'fn test_default_publish_functions\(\) \{', r'fn test_default_publish_functions() -> Result<(), Box<dyn std::error::Error>> {', code)

# Add Ok(()) to the end of each test function
code = re.sub(r'(assert_eq!\(npmrc, "//registry.npmjs.org/:_authToken=secret123"\);\n    \})', r'\1'.replace('}\n', 'Ok(())\n    }'), code)

with open("src/publish.rs", "w") as f:
    f.write(code)

