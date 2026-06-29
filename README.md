# cdd-publisher

> Background worker and publisher service for the `cdd` ecosystem.

[![CI](https://github.com/SamuelMarks/cdd-publisher/actions/workflows/ci.yml/badge.svg)](https://github.com/SamuelMarks/cdd-publisher/actions/workflows/ci.yml)
![Test Coverage](https://img.shields.io/badge/coverage-100%25-success.svg)
![Doc Coverage](https://img.shields.io/badge/docs-0%25-red.svg)
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://opensource.org/licenses/Apache-2.0)

`cdd-publisher` is an asynchronous worker node responsible for pushing generated software development kits (SDKs) to external package registries (npm, PyPI, Cargo, etc.).

## Ecosystem Integration
While users interact primarily with the [CDD Web UI](../cdd-web-ui)—the central graphical interface for configuring, generating, and reviewing SDKs—`cdd-publisher` operates entirely in the background. When a user triggers a release from the Web UI, the request flows through the control plane to the publisher queue. This architecture ensures that slow, network-dependent registry uploads do not bottleneck the frontend experience or the core generation engine.

## Features
- **Asynchronous Execution:** Listens to event queues for publish requests.
- **Registry Integration:** Orchestrates package manager commands safely and consistently.
- **Secure Handling:** Receives decrypted secrets in-memory just-in-time to perform registry authentication.
- **Audit Logging:** Reports the success or failure of release events back to the control plane.

## Prerequisites

- **Rust** (Edition 2024 via the `cargo` toolchain)
- **Redis** server for the event queue broker
- **Python 3** (for local development doc checks)
- **Pre-commit** (for Git hooks)

## Getting Started

### 1. Start the Event Broker (Redis)
You can start a local development Redis server on port 63799:
```bash
redis-server --port 63799
```

### 2. Run the Publisher
Execute the worker node using Cargo:
```bash
cargo run
```

## Development and Contributing

Our codebase enforces 100% documentation coverage and strict linting. 

- **Testing:** Run the comprehensive test suite with `cargo test`.
- **Linting:** We enforce strict Clippy rules. Run `cargo clippy --all-targets --all-features`.
- **Pre-commit:** Install pre-commit hooks to ensure code quality before pushing:
  ```bash
  pre-commit install
  ```
- **Documentation Coverage:** Ensure all public and internal items are documented. You can check for missing docs using the provided utility:
  ```bash
  python check_docs.py
  ```

## Deployment

The project includes Dockerfiles for containerized deployment:
- `alpine.Dockerfile`: For a minimal, musl-based deployment.
- `debian.Dockerfile`: For glibc-based deployment.

## Architecture

For more deep-dive technical details on constraints, security, and internal workflows, refer to the [ARCHITECTURE.md](ARCHITECTURE.md).

---

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
