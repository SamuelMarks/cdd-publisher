import re

for filename in ["src/publish.rs", "src/storage.rs", "src/worker.rs"]:
    with open(filename, "r") as f:
        code = f.read()

    # Revert signatures
    code = re.sub(r'-> std::result::Result<\(\), Box<dyn std::error::Error>> ', '', code)
    code = re.sub(r'-> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> ', '-> std::path::PathBuf ', code)
    
    # Revert Ok(())
    code = re.sub(r'(\s*)Ok\(\(\)\)\n(\s*)\}', r'\2}', code)
    code = re.sub(r'Ok\(exe_path\)', 'exe_path', code)
    
    # Revert ?
    code = code.replace("TempDir::new()?", 'TempDir::new().expect("Failed to create TempDir")')
    code = code.replace('.ok_or_else(|| std::io::Error::other("Valid path"))?', '.expect("Valid path")')
    code = code.replace('.ok_or("Valid path")?', '.expect("Valid path")')
    code = code.replace('fs::write(&exe_path, script)?;', 'fs::write(&exe_path, script).expect("Failed to write mock exe");')
    code = code.replace('fs::set_permissions(&exe_path, fs::Permissions::from_mode(0o755))?;', 'fs::set_permissions(&exe_path, fs::Permissions::from_mode(0o755)).expect("Failed to set perms");')
    code = code.replace('fs::read_to_string(dest.path().join(".npmrc"))?', 'fs::read_to_string(dest.path().join(".npmrc")).expect("Failed to read .npmrc")')
    code = code.replace('.expect("Failed to create TempDir")?', '.expect("Failed to create TempDir")') # In case of double
    
    # Storage
    code = code.replace('header.set_path("hello.txt")?', 'header.set_path("hello.txt").expect("set_path failed")')
    code = code.replace('builder.append(&header, "hello world".as_bytes())?', 'builder.append(&header, "hello world".as_bytes()).expect("append failed")')
    code = code.replace('builder.finish()?', 'builder.finish().expect("finish failed")')
    code = code.replace('std::fs::read_to_string(dest.path().join("hello.txt"))?', 'std::fs::read_to_string(dest.path().join("hello.txt")).expect("read_to_string failed")')
    code = code.replace('zip.start_file("hello.txt", options)?', 'zip.start_file("hello.txt", options).expect("start_file failed")')
    code = code.replace('zip.write_all(b"hello world")?', 'zip.write_all(b"hello world").expect("write_all failed")')
    code = code.replace('zip.finish()?', 'zip.finish().expect("finish failed")')
    
    # Worker
    code = code.replace('await?', 'await.expect("Failed to await")')
    code = code.replace('zip.start_file("hello.txt", zip::write::SimpleFileOptions::default())?', 'zip.start_file("hello.txt", zip::write::SimpleFileOptions::default()).expect("start_file failed")')
    code = code.replace('std::io::Write::write_all(&mut zip, b"hello world")?', 'std::io::Write::write_all(&mut zip, b"hello world").expect("write_all failed")')
    
    with open(filename, "w") as f:
        f.write(code)

