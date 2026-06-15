import re

with open("src/worker.rs", "r") as f:
    code = f.read()

# Fix the broken match arm again
code = code.replace(" => AuditPayload {", "Ok(()) => AuditPayload {")

code = code.replace("if let Err(e) = result {\n            let err_str = e.to_string();\n            if !err_str.contains(\"BUSYGROUP\") {\n                return Err(PublisherError::Redis(e));\n            }\n        }", "if let Err(e) = result {\n            let err_str = e.to_string();\n            if !err_str.contains(\"BUSYGROUP\") {\n                return Err(PublisherError::Redis(e));\n            }\n        }\n        Ok(())")

code = code.replace("let _: () = redis::AsyncCommands::xack(self, stream, group, &[id]).await.expect(\"Failed to await\");\n\n    }", "let _: () = redis::AsyncCommands::xack(self, stream, group, &[id]).await.expect(\"Failed to await\");\n        Ok(())\n    }")

code = code.replace("self.acknowledge(&id).await.expect(\"Failed to await\");\n\n    }", "self.acknowledge(&id).await.expect(\"Failed to await\");\n        Ok(())\n    }")

code = code.replace("publish_npm(dest_dir.path(), token)?;\n            }", "publish_npm(dest_dir.path(), token)?;\n                Ok(())\n            }")
code = code.replace("publish_pypi(dest_dir.path(), token)?;\n            }", "publish_pypi(dest_dir.path(), token)?;\n                Ok(())\n            }")
code = code.replace("publish_cargo(dest_dir.path(), token)?;\n            }", "publish_cargo(dest_dir.path(), token)?;\n                Ok(())\n            }")

# Fix run_once match err arm
code = code.replace("Err(e) => {\n                eprintln!(\"Failed to parse job payload: {e}\");\n                // Invalid payload, acknowledge to drop it or move to DLQ.\n                self.acknowledge(&id).await.expect(\"Failed to await\");\n\n            }\n        }", "Err(e) => {\n                eprintln!(\"Failed to parse job payload: {e}\");\n                // Invalid payload, acknowledge to drop it or move to DLQ.\n                self.acknowledge(&id).await.expect(\"Failed to await\");\n            }\n        }\n        Ok(())")

with open("src/worker.rs", "w") as f:
    f.write(code)