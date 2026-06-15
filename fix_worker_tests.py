import re

with open("src/worker.rs", "r") as f:
    code = f.read()

# Fix the test signatures
code = re.sub(r'async fn (test_[a-zA-Z0-9_]+)\(\) \{', r'async fn \1() -> std::result::Result<(), Box<dyn std::error::Error>> {', code)
code = re.sub(r'fn test_publish_job_serialization\(\) \{', r'fn test_publish_job_serialization() -> std::result::Result<(), Box<dyn std::error::Error>> {', code)

# Ensure Ok(()) is returned where necessary
code = code.replace("            } else {\n                panic!(\"Deserialization failed\");\n            }\n        } else {\n            panic!(\"Invalid JSON serialization\");\n        }", "            } else {\n                panic!(\"Deserialization failed\");\n            }\n        } else {\n            panic!(\"Invalid JSON serialization\");\n        }\n        Ok(())")
code = code.replace("assert_eq!(worker_err.run_once().await.unwrap_err().to_string(), \"Publish error: mock fetch error\");", "assert_eq!(worker_err.run_once().await.unwrap_err().to_string(), \"Publish error: mock fetch error\");\n        Ok(())")

with open("src/worker.rs", "w") as f:
    f.write(code)
