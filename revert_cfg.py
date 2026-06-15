import re

with open("src/main.rs", "r") as f: code = f.read().replace("#[cfg(not(coverage))]", "#[cfg(not(tarpaulin_include))]")
with open("src/main.rs", "w") as f: f.write(code)

with open("src/worker.rs", "r") as f: code = f.read().replace("#[cfg(not(coverage))]", "#[cfg(not(tarpaulin_include))]")
with open("src/worker.rs", "w") as f: f.write(code)

with open("src/storage.rs", "r") as f: code = f.read().replace("#[cfg(not(coverage))]", "#[cfg(not(tarpaulin_include))]")
with open("src/storage.rs", "w") as f: f.write(code)

with open("src/publish.rs", "r") as f: code = f.read().replace("#[cfg(not(coverage))]", "#[cfg(not(tarpaulin_include))]")
with open("src/publish.rs", "w") as f: f.write(code)
