import re

with open("src/worker.rs", "r") as f:
    code = f.read()

new_test = """    #[tokio::test]
    async fn test_worker_run_once_success() {
        use wiremock::{MockServer, Mock, matchers::{method, path}, ResponseTemplate};
        let mock_server = MockServer::start().await;
        let mut zip_data = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_data));
            zip.start_file("hello.txt", zip::write::SimpleFileOptions::default()).expect("start_file failed");
            std::io::Write::write_all(&mut zip, b"hello world").expect("write_all failed");
            zip.finish().expect("finish failed");
        }
        Mock::given(method("GET"))
            .and(path("/artifact.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(zip_data))
            .mount(&mock_server).await;

        let mut mock_queue = MockRedisQueue::new();
        mock_queue.expect_create_group().returning(|_, _| Ok(()));
        let job_url = format!("{}/artifact.zip", mock_server.uri());
        let valid_json = format!(r#"{"artifact_id":"{}","registry":"npm"}"#, job_url).into_bytes();
        mock_queue.expect_read_one().returning(move |_, _, _| Ok(Some(("1-0".to_string(), valid_json.clone()))));
        mock_queue.expect_ack().returning(|_, _, _| Ok(()));
        
        let tokens = RegistryTokens { npm: Some("token".into()), pypi: None, cargo: None };
        let mut worker = Worker::new(mock_queue, "stream", "group", "consumer", tokens, get_dummy_audit_client()).await.unwrap();
        assert!(worker.run_once().await.is_ok());
    }"""

# Insert before the last `}`
idx = code.rfind("}")
code = code[:idx] + new_test + "\n" + code[idx:]

with open("src/worker.rs", "w") as f:
    f.write(code)
