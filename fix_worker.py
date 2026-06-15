import re

with open("src/worker.rs", "r") as f:
    code = f.read()

# Change all test functions to return std::result::Result<(), Box<dyn std::error::Error>>
code = re.sub(r'async fn test_([a_zA-Z0-9_]+)\(\) \{', r'async fn test_\1() -> std::result::Result<(), Box<dyn std::error::Error>> {', code)

code = code.replace(".expect(\"Failed to create worker\")", "?")
code = code.replace(".expect(\"start_file failed\")", "?")
code = code.replace(".expect(\"write_all failed\")", "?")
code = code.replace(".expect(\"finish failed\")", "?")

code = code.replace("assert_eq!(worker_err.run_once().await.unwrap_err().to_string(), \"Publish error: mock fetch error\");\n    }", "assert_eq!(worker_err.run_once().await.unwrap_err().to_string(), \"Publish error: mock fetch error\");\n        Ok(())\n    }")

code = code.replace('assert_eq!(deserialized.payload.registry, "npm");\n    }', 'assert_eq!(deserialized.payload.registry, "npm");\n        Ok(())\n    }')
code = code.replace('assert_eq!(worker.group_name, "group");\n    }', 'assert_eq!(worker.group_name, "group");\n        Ok(())\n    }')
code = code.replace('assert!(result.is_err());\n    }', 'assert!(result.is_err());\n        Ok(())\n    }')
code = code.replace('assert!(worker.run_once().await.is_ok());\n    }', 'assert!(worker.run_once().await.is_ok());\n        Ok(())\n    }')
code = code.replace('assert!(worker.run_once().await.is_ok()); // Should log and continue\n    }', 'assert!(worker.run_once().await.is_ok()); // Should log and continue\n        Ok(())\n    }')
code = code.replace('assert!(worker.run_once().await.is_ok()); // Should fail to deserialize but continue\n    }', 'assert!(worker.run_once().await.is_ok()); // Should fail to deserialize but continue\n        Ok(())\n    }')
code = code.replace('assert_eq!(mock_queue_no_cargo.ack_count(), 1);\n    }', 'assert_eq!(mock_queue_no_cargo.ack_count(), 1);\n        Ok(())\n    }')

with open("src/worker.rs", "w") as f:
    f.write(code)
