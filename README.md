# cdd-publisher

> Background worker and publisher service for the `cdd` ecosystem.

[![CI](https://github.com/SamuelMarks/cdd-publisher/actions/workflows/ci.yml/badge.svg)](https://github.com/SamuelMarks/cdd-publisher/actions/workflows/ci.yml)
![Test Coverage](https://img.shields.io/badge/coverage-100%25-success.svg)
![Doc Coverage](https://img.shields.io/badge/docs-100%25-success.svg)
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://opensource.org/licenses/Apache-2.0)

`cdd-publisher` is an asynchronous worker node responsible for pushing generated software development kits (SDKs) to external package registries (npm, PyPI, Cargo, etc.).

## Ecosystem Integration
While users interact primarily with the [CDD Web UI](../cdd-web-ui)—the central graphical interface for configuring, generating, and reviewing SDKs—`cdd-publisher` operates entirely in the background. When a user triggers a release from the Web UI, the request flows through the control plane to the publisher queue. This architecture ensures that slow, network-dependent registry uploads do not bottleneck the frontend experience or the core generation engine.

## Features
- **Asynchronous Execution:** Listens to Redis Streams event queues for publish requests.
- **Universal 13-Registry Publishing:** Orchestrates package releases across `npm`, `PyPI`, `Cargo`, `Maven Central`, `NuGet`, `RubyGems`, `Packagist`, `pkg.go.dev`, `Swift Package Index`, `vcpkg`, `Conan`, `Homebrew`, and `GitHub Releases`.
- **Monorepo & Polyrepo Topologies:** Supports both `${GITHUB_ORG_NAME}-sdk` monorepos (with validated `<lang>/` subdirectories) and standalone `${GITHUB_ORG_NAME}-sdk-${LANGUAGE}` polyrepos.
- **Ephemeral PGP / GPG Signing:** Generates detached `.asc` signatures and GPG-signed Git tags inside an isolated `0o700` `GNUPGHOME` directory (`TempGpgHome`).
- **Multi-Target Documentation Publishing:** Deploys generated SDK docs and OpenAPI specs to the Official CDD Hosted Doc Site (`cdd-storage` + `cdd-docs-ui`), GitHub Pages (`gh-pages`), `ReadTheDocs`, ecosystem doc hosts (`docs.rs`, `pkg.go.dev`, `javadoc.io`, `rubydoc.info`, Swift `DocC`), or custom webhooks.
- **Secure Handling:** Receives decrypted secrets in-memory (`SecretToken`), redacts tokens in logs/diagnostics, uses `0o600` RAII-guarded credential files, and zeroes secret buffers on `Drop`.
- **Audit Logging:** Reports the success or failure of release events back to `cdd-control-plane`.

## Prerequisites

- **Rust** (Edition 2024 via the `cargo` toolchain)
- **Redis** server for the event queue broker
- **Pre-commit** (for Git hooks)

## Getting Started

### 1. Start the Event Broker (Redis)
You can start a local development Redis server on the default port (6379):
```bash
redis-server
```

### 2. Run the Publisher
Execute the worker node using Cargo (configure `REDIS_URL`, `CONTROL_PLANE_URL`, `NPM_TOKEN`, `TWINE_PASSWORD`, and `CARGO_REGISTRY_TOKEN` as needed):
```bash
cargo run
```

## Development and Contributing

Our codebase enforces 100% documentation coverage, 100% test coverage, and strict linting.

- **Testing:** Run the comprehensive test suite with `cargo test`.
- **Linting:** We enforce strict Clippy rules. Run `cargo clippy --all-targets --all-features -- -D warnings`.
- **Pre-commit:** Install pre-commit hooks to ensure code quality before committing:
  ```bash
  pre-commit install
  ```
- **Documentation Coverage:** Ensure all public and internal items are documented (`#![deny(missing_docs)]` and `#![deny(clippy::missing_docs_in_private_items)]`):
  ```bash
  cargo +nightly rustdoc -- -Z unstable-options --show-coverage
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
