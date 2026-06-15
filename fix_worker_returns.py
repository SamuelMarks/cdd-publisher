import re

with open("src/worker.rs", "r") as f:
    code = f.read()

tests = [
    "test_publish_job_serialization",
    "test_worker_new_success",
    "test_worker_new_error",
    "test_worker_run_once_no_messages",
    "test_worker_run_once_empty_payload",
    "test_worker_run_once_invalid_json",
    "test_worker_process_job",
    "test_worker_run_once_process_error"
]

for test in tests:
    # Find the start of the test
    start_idx = code.find(f"fn {test}()")
    if start_idx == -1:
        continue
    # Find the next `^    }`
    end_idx = code.find("\n    }", start_idx)
    if end_idx != -1:
        # Check if Ok(()) is already there
        block = code[start_idx:end_idx]
        if not block.rstrip().endswith("Ok(())"):
            code = code[:end_idx] + "\n        Ok(())" + code[end_idx:]

with open("src/worker.rs", "w") as f:
    f.write(code)
