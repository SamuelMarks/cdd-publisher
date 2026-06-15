import re

def rewrite_file(filepath):
    with open(filepath, "r") as f:
        code = f.read()

    # Just an example of trying to replace ? with match ... { Ok(v) => v, Err(e) => panic!("{e}") }
    pass

