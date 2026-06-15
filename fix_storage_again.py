import re

with open("src/storage.rs", "r") as f:
    code = f.read()

# Replace the broken function
new_func = """pub async fn fetch_and_unpack(client: &Client, url: &str, dest_dir: &Path) -> Result<()> {
    let response = client.get(url).send().await?.error_for_status()?;
    let bytes = response.bytes().await?;

    let is_tar_gz = url.ends_with(".tar.gz") || url.ends_with(".tgz");
    let is_zip = url.ends_with(".zip");

    if !is_tar_gz && !is_zip {
        return Err(PublisherError::Publish(
            "Unsupported artifact format".to_string(),
        ));
    }

    let temp_dir = tempfile::tempdir()?;
    let temp_file_path = temp_dir.path().join("archive");
    std::fs::write(&temp_file_path, &bytes)?;
    let temp_file = std::fs::File::open(&temp_file_path)?;

    if is_zip {
        let mut archive = zip::ZipArchive::new(temp_file)?;
        archive.extract(dest_dir)?;
    } else {
        let tar = flate2::read::GzDecoder::new(temp_file);
        let mut archive = tar::Archive::new(tar);
        archive.unpack(dest_dir)?;
    }
    
    Ok(())
}"""

idx1 = code.find("pub async fn fetch_and_unpack")
idx2 = code.find("#[cfg(test)]")

if idx1 != -1 and idx2 != -1:
    code = code[:idx1] + new_func + "\n\n" + code[idx2:]

with open("src/storage.rs", "w") as f:
    f.write(code)

