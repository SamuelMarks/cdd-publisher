import re
with open("src/publish.rs", "r") as f:
    code = f.read()

code = code.replace("-> Result<std::path::PathBuf, Box<dyn std::error::Error>>", "-> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>>")
code = code.replace("-> Result<(), Box<dyn std::error::Error>>", "-> std::result::Result<(), Box<dyn std::error::Error>>")
code = code.replace('.ok_or("Valid path")', '.ok_or_else(|| std::io::Error::other("Valid path"))')

with open("src/publish.rs", "w") as f:
    f.write(code)
