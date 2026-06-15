import re

with open("src/storage.rs", "r") as f:
    code = f.read()

code = code.replace("async fn test_fetch_and_unpack_unsupported_format() {", "async fn test_fetch_and_unpack_unsupported_format() -> std::result::Result<(), Box<dyn std::error::Error>> {")
code = code.replace("async fn test_fetch_and_unpack_tar_gz() {", "async fn test_fetch_and_unpack_tar_gz() -> std::result::Result<(), Box<dyn std::error::Error>> {")
code = code.replace("async fn test_fetch_and_unpack_zip() {", "async fn test_fetch_and_unpack_zip() -> std::result::Result<(), Box<dyn std::error::Error>> {")

code = code.replace(".expect(\"Failed to create TempDir\")", "?")
code = code.replace(".expect(\"set_path failed\")", "?")
code = code.replace(".expect(\"append failed\")", "?")
code = code.replace(".expect(\"finish failed\")", "?")
code = code.replace(".expect(\"read_to_string failed\")", "?")
code = code.replace(".expect(\"start_file failed\")", "?")
code = code.replace(".expect(\"write_all failed\")", "?")

# add Ok(()) to the end of tests
code = code.replace('assert!(e.to_string().contains("Unsupported artifact format"));\n        }\n    }', 'assert!(e.to_string().contains("Unsupported artifact format"));\n        }\n        Ok(())\n    }')
code = code.replace('assert_eq!(unpacked_file, "hello world");\n    }', 'assert_eq!(unpacked_file, "hello world");\n        Ok(())\n    }')

with open("src/storage.rs", "w") as f:
    f.write(code)

