import re

with open("src/worker.rs", "r") as f:
    code = f.read()

fixed = """    async fn create_group(&mut self, stream: &str, group: &str) -> Result<()> {
        let result: redis::RedisResult<()> = redis::AsyncCommands::xgroup_create_mkstream(self, stream, group, "$").await;
        if let Err(e) = result {
            let err_str = e.to_string();
            if !err_str.contains("BUSYGROUP") {
                return Err(PublisherError::Redis(e));
            }
        }
        Ok(())
    }"""

idx1 = code.find("    async fn create_group(&mut self, stream: &str, group: &str) -> Result<()> {")
idx2 = code.find("    async fn read_one(&mut self, stream: &str, group: &str, consumer: &str) -> Result<Option<(String, Vec<u8>)>> {")

code = code[:idx1] + fixed + "\n\n" + code[idx2:]

with open("src/worker.rs", "w") as f:
    f.write(code)

