# cdd-publisher

> Background worker and publisher service for the `cdd` ecosystem.

[![CI](https://github.com/SamuelMarks/cdd-publisher/actions/workflows/ci.yml/badge.svg)](https://github.com/SamuelMarks/cdd-publisher/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue.svg)](https://opensource.org/licenses/Apache-2.0)

`cdd-publisher` is an asynchronous worker node responsible for pushing generated software development kits (SDKs) to external package registries (npm, PyPI, Cargo, etc.). Decoupled from the main request/response cycle, it ensures that slow, network-dependent registry uploads do not bottleneck the core API or the generation engine.

## Features
- **Asynchronous Execution:** Listens to event queues for publish requests.
- **Registry Integration:** Orchestrates package manager commands safely and consistently.
- **Secure Handling:** Receives decrypted secrets in-memory just-in-time to perform registry authentication.
- **Audit Logging:** Reports the success or failure of release events back to the control plane.

## License
Dual-licensed under Apache 2.0 and MIT.
