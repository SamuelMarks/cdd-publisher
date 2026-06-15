with open("src/storage.rs", "r") as f:
    code = f.read()

old_func = """pub async fn fetch_and_unpack(client: &Client, url: &str, dest_dir: &Path) -> Result<()> {
    let response = client.get(url).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;

    let is_tar_gz = url.ends_with(".tar.gz") || url.ends_with(".tgz");
    let is_zip = url.ends_with(".zip");

    if !is_tar_gz && !is_zip {
        return Err(crate::error::PublisherError::Publish(
            "Unsupported artifact format".to_string(),
        ));
    }"""

new_func = """pub async fn fetch_and_unpack(client: &Client, url: &str, dest_dir: &Path) -> Result<()> {
    let is_tar_gz = url.ends_with(".tar.gz") || url.ends_with(".tgz");
    let is_zip = url.ends_with(".zip");

    if !is_tar_gz && !is_zip {
        return Err(crate::error::PublisherError::Publish(
            "Unsupported artifact format".to_string(),
        ));
    }

    let response = client.get(url).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;"""

code = code.replace(old_func, new_func)

with open("src/storage.rs", "w") as f:
    f.write(code)
