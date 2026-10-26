# System & Infrastructure

<!-- Generated from the CEP registry by api/tests/cep_docs.rs; do not edit. -->

| Method | Scope | Runs as | Summary |
|---|---|---|---|
| [`config.get`](./config/config.get.md) | read | sync | The saved settings: one section, or all of them |
| [`config.set`](./config/config.set.md) | write | sync | Changes the given fields of one settings section; answers with the section as saved |
| [`rpc.discover`](./rpc/rpc.discover.md) | read | sync | The OpenRPC description of the methods you may call |
| [`storage.clean`](./storage/storage.clean.md) | write | sync | Removes cached temporary media |
| [`storage.status`](./storage/storage.status.md) | read | sync | Temporary media storage usage |
| [`system.audit`](./system/system.audit.md) | admin | sync | The newest lines of the CEP audit log, newest first |
| [`system.exec`](./system/system.exec.md) | admin | sync | Runs a program on the host and returns its output; the owner approves each call |
| [`system.info`](./system/system.info.md) | read | sync | Engine name and version, CEP version, hardware, and the LAN address |
| [`system.live`](./system/system.live.md) | read | sync | Current process, memory, CPU and GPU telemetry |
| [`system.plan`](./system/system.plan.md) | read | sync | Plans an installed model without loading its weights or changing saved settings |
| [`system.ps`](./system/system.ps.md) | read | sync | Loaded model execution plan and hardware snapshot |
| [`tasks.cancel`](./tasks/tasks.cancel.md) | read | sync | Cancels a running task |
| [`tasks.get`](./tasks/tasks.get.md) | read | sync | A task's status, progress and new events |
| [`tasks.list`](./tasks/tasks.list.md) | read | sync | Your tasks that have not expired, newest first |
| [`tasks.result`](./tasks/tasks.result.md) | read | sync | A task's final result or error; waits up to wait_ms |
| [`tokens.create`](./tokens/tokens.create.md) | admin | sync | Makes a new API key (at most 5) |
| [`tokens.list`](./tokens/tokens.list.md) | admin | sync | The API keys, and whether API authentication is required |
| [`tokens.revoke`](./tokens/tokens.revoke.md) | admin | sync | Removes an API key; with none left, API authentication turns off (as before) |

All categories: [CEP reference](../README.md). Folder: `docs/api/cep/system/`.
