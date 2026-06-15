import re

with open("src/publish.rs", "r") as f:
    code = f.read()

def replace_expect(match):
    return '? /* replaced expect */'

code = re.sub(r'expect\("[^"]+"\)', '?', code)

with open("src/publish.rs", "w") as f:
    f.write(code)

