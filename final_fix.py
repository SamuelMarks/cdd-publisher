import re

with open("src/worker.rs", "r") as f:
    code = f.read()

# Fix expect in tests
code = code.replace("Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await.expect(\"Failed to await\")", "Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await.expect(\"Failed to create\")")
code = code.replace("Worker::new(mock_queue_no_pypi, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await.expect(\"Failed to await\")", "Worker::new(mock_queue_no_pypi, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await.expect(\"Failed to create\")")
code = code.replace("Worker::new(mock_queue_no_cargo, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await.expect(\"Failed to await\")", "Worker::new(mock_queue_no_cargo, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await.expect(\"Failed to create\")")
code = code.replace("Worker::new(mock_queue_err, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await.expect(\"Failed to await\")", "Worker::new(mock_queue_err, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await.expect(\"Failed to create\")")
code = code.replace("Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", tokens, get_dummy_audit_client()).await.expect(\"Failed to await\")", "Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", tokens, get_dummy_audit_client()).await.expect(\"Failed to create\")")

# For test_worker_new_error
code = code.replace("let worker = Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await.expect(\"Failed to create\");\n        assert!(worker.is_err());", "let worker = Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await;\n        assert!(worker.is_err());")

# For test_worker_new_success
code = code.replace("let worker = Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await;\n        assert!(worker.is_ok());", "let worker = Worker::new(mock_queue, \"stream\", \"group\", \"consumer\", RegistryTokens::default(), get_dummy_audit_client()).await;\n        assert!(worker.is_ok());")

# Fix expect in process_job
code = code.replace("fetch_and_unpack(&self.client, &job.artifact_id, dest_dir.path()).await.expect(\"Failed to await\")", "fetch_and_unpack(&self.client, &job.artifact_id, dest_dir.path()).await?")

with open("src/worker.rs", "w") as f:
    f.write(code)

with open("src/storage.rs", "r") as f:
    code = f.read()

# Fix the bug in fetch_and_unpack I introduced: check URL before request!
old_func = """pub async fn fetch_and_unpack(client: &Client, url: &str, dest_dir: &Path) -> Result<()> {
    let response = client.get(url).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;

    let is_tar_gz = url.ends_with(".tar.gz") || url.ends_with(".tgz");
    let is_zip = url.ends_with(".zip");

    if !is_tar_gz && !is_zip {
        return Err(PublisherError::Publish(
            "Unsupported artifact format".to_string(),
        ));
    }"""

new_func = """pub async fn fetch_and_unpack(client: &Client, url: &str, dest_dir: &Path) -> Result<()> {
    let is_tar_gz = url.ends_with(".tar.gz") || url.ends_with(".tgz");
    let is_zip = url.ends_with(".zip");

    if !is_tar_gz && !is_zip {
        return Err(PublisherError::Publish(
            "Unsupported artifact format".to_string(),
        ));
    }

    let response = client.get(url).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;"""

code = code.replace(old_func, new_func)

with open("src/storage.rs", "w") as f:
    f.write(code)
