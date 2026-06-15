# Architecture: `cdd-publisher`

`cdd-publisher` acts as the stateful, network-facing worker of the CDD ecosystem.

## Core Flow
1. **Dequeue Job:** `cdd-publisher` listens on a message broker (e.g., Redis, SQS, RabbitMQ).
2. **Fetch Artifacts:** It retrieves the freshly generated code bundle from `cdd-engine` (or via `cdd-storage`).
3. **Authenticate:** It utilizes the decrypted registry tokens provided within the job payload (or fetches them securely from `cdd-control-plane`).
4. **Publish:** It runs the corresponding sub-commands (e.g., `npm publish`, `cargo publish`, `twine upload`) in a heavily isolated sandbox.
5. **Report Back:** On completion or failure, an event is emitted back to `cdd-control-plane` to update the database and the user's Audit Log.

## Design Constraints
- **Security:** This service handles highly sensitive API keys. It must NEVER log secrets to standard out.
- **Resiliency:** Must implement robust retry semantics with exponential backoff for network flakes on the registries.
