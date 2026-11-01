# Architecture: `cdd-publisher`

`cdd-publisher` acts as the stateful, network-facing worker of the CDD ecosystem.

## Core Flow
1. **Initiation:** The user requests a package release directly from the [CDD Web UI](../cdd-web-ui). The central control plane enqueues this job.
2. **Dequeue Job:** `cdd-publisher` listens on a message broker (e.g., Redis Streams) for these publish events (`PublishJob`).
3. **Fetch & Unpack Artifacts:** It retrieves the generated SDK archive (`.zip`, `.tar.gz`, `.tgz`) from `cdd-storage`, validates all archive paths against Zip-Slip traversal, and resolves the working directory for either **Polyrepo** (`${GITHUB_ORG_NAME}-sdk-${LANGUAGE}`) or **Monorepo** (`${GITHUB_ORG_NAME}-sdk` with `<lang>/` subdirectories) topologies.
4. **Authenticate & Sign:** It utilizes decrypted `SecretToken` credentials and optional `PgpSigningConfig` inside an isolated `0o700` `TempGpgHome` (`GNUPGHOME`) to generate detached `.asc` signatures and GPG-signed Git tags without mutating the host keyring.
5. **Publish Package & Docs:** It runs the corresponding ecosystem CLI (`npm`, `twine`, `cargo`, `mvn`/`gradle`, `dotnet`, `gem`, `composer`, `git`/`go`, `swift`, `conan`, `vcpkg`, `brew`, `gh`) in a blocking worker thread, and optionally deploys documentation via `DocsPublisher` (`OfficialCddSite`, `GithubPages`, `ReadTheDocs`, `DocsRs`, `PkgGoDev`, `JavadocIo`, `RubyDoc`, `SwiftPackageIndexDocs`, `CustomUrl`).
6. **Report Back:** On completion or failure, an event (`AuditPayload`) is emitted back to `cdd-control-plane` to update the database, which is immediately reflected in the user's Audit Log within the Web UI.

## Design Constraints
- **Security:** This service handles highly sensitive API keys and PGP private keys. It must NEVER log secrets to standard out (`SecretToken` redacts `Debug`/`Display`, scrubs `stdout`/`stderr`, and zeroes memory on `Drop`).
- **Strong Typing & Unified Error Handling:** All domain primitives are validated value objects (`src/domain.rs`) and all errors route through a single `PublisherError` enum (`derive_more`) with zero `unwrap`/`expect`/`panic`.
