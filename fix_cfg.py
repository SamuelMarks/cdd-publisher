import re

with open("src/main.rs", "r") as f:
    code = f.read()
code = code.replace("#[cfg(not(tarpaulin_include))]", "#[cfg(not(coverage))]")
with open("src/main.rs", "w") as f:
    f.write(code)

with open("src/worker.rs", "r") as f:
    code = f.read()
code = code.replace("#[cfg(not(tarpaulin_include))]", "#[cfg(not(coverage))]")
with open("src/worker.rs", "w") as f:
    f.write(code)

with open("src/storage.rs", "r") as f:
    code = f.read()
code = code.replace("#[cfg(not(tarpaulin_include))]", "#[cfg(not(coverage))]")
with open("src/storage.rs", "w") as f:
    f.write(code)

with open("src/publish.rs", "r") as f:
    code = f.read()
code = code.replace("#[cfg(not(tarpaulin_include))]", "#[cfg(not(coverage))]")
with open("src/publish.rs", "w") as f:
    f.write(code)
