import re

with open("src/worker.rs", "r") as f:
    code = f.read()

# Fix missing docs for report_audit
code = code.replace("    #[cfg(not(tarpaulin_include))]\n    async fn report_audit", "    /// Reports the audit status.\n    #[cfg(not(tarpaulin_include))]\n    async fn report_audit")

# Fix unwrap in test
code = code.replace("Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", tokens, get_dummy_audit_client()).await.unwrap()", "Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", tokens, get_dummy_audit_client()).await.expect(\"Failed to create worker\")")

# Fix uninlined format args
code = code.replace("let valid_json = format!(r#\"{{\"artifact_id\":\"{}\",\"registry\":\"npm\"}}\"#, job_url).into_bytes();", "let valid_json = format!(r#\"{{\"artifact_id\":\"{job_url}\",\"registry\":\"npm\"}}\"#).into_bytes();")

with open("src/worker.rs", "w") as f:
    f.write(code)

