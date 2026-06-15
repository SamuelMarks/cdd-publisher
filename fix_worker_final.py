import re

with open("src/worker.rs", "r") as f:
    code = f.read()

# Fix the broken match arm
code = code.replace(" => AuditPayload {", "Ok(()) => AuditPayload {")

# We need Ok(()) at the end of create_group
code = code.replace("""if !err_str.contains("BUSYGROUP") {
                return Err(PublisherError::Redis(e));
            }""", """if !err_str.contains("BUSYGROUP") {
                return Err(PublisherError::Redis(e));
            }""")

# Let's just fix the functions manually using python string replacements

code = re.sub(r'(\s+)if let Err\(e\) = result \{\n(\s+)let err_str = e\.to_string\(\);\n(\s+)if !err_str\.contains\("BUSYGROUP"\) \{\n(\s+)return Err\(PublisherError::Redis\(e\)\);\n(\s+)\}\n(\s+)\}', r'\1if let Err(e) = result {\n\2let err_str = e.to_string();\n\3if !err_str.contains("BUSYGROUP") {\n\4return Err(PublisherError::Redis(e));\n\5}\n\6}\n\1Ok(())', code)

code = re.sub(r'(\s+)let _: \(\) = redis::AsyncCommands::xack\(self, stream, group, &\[id\]\)\.await\.expect\("Failed to await"\);\n(\s+)\}', r'\1let _: () = redis::AsyncCommands::xack(self, stream, group, &[id]).await.expect("Failed to await");\n\1Ok(())\n\2}', code)

code = re.sub(r'(\s+)self\.acknowledge\(&id\)\.await\.expect\("Failed to await"\);\n(\s+)\}', r'\1self.acknowledge(&id).await.expect("Failed to await");\n\1Ok(())\n\2}', code)

code = re.sub(r'(\s+)publish_npm\(dest_dir\.path\(\), token\)\?;\n(\s+)\}', r'\1publish_npm(dest_dir.path(), token)?;\n\1Ok(())\n\2}', code)
code = re.sub(r'(\s+)publish_pypi\(dest_dir\.path\(\), token\)\?;\n(\s+)\}', r'\1publish_pypi(dest_dir.path(), token)?;\n\1Ok(())\n\2}', code)
code = re.sub(r'(\s+)publish_cargo\(dest_dir\.path\(\), token\)\?;\n(\s+)\}', r'\1publish_cargo(dest_dir.path(), token)?;\n\1Ok(())\n\2}', code)

with open("src/worker.rs", "w") as f:
    f.write(code)

