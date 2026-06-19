# Architecture: `cdd-publisher`

`cdd-publisher` acts as the stateful, network-facing worker of the CDD ecosystem.

## Core Flow
1. **Initiation:** The user requests a package release directly from the [CDD Web UI](../cdd-web-ui). The central control plane enqueues this job.
2. **Dequeue Job:** `cdd-publisher` listens on a message broker (e.g., Redis, SQS, RabbitMQ) for these publish events.
3. **Fetch Artifacts:** It retrieves the freshly generated code bundle from `cdd-engine` (or via `cdd-storage`).
4. **Authenticate:** It utilizes the decrypted registry tokens provided within the job payload (or fetches them securely from `cdd-control-plane`).
5. **Publish:** It runs the corresponding sub-commands (e.g., `npm publish`, `cargo publish`, `twine upload`) in a heavily isolated sandbox.
6. **Report Back:** On completion or failure, an event is emitted back to `cdd-control-plane` to update the database, which is immediately reflected in the user's Audit Log within the Web UI.

## Design Constraints
- **Security:** This service handles highly sensitive API keys. It must NEVER log secrets to standard out.
- **Resiliency:** Must implement robust retry semantics with exponential backoff for network flakes on the registries.
