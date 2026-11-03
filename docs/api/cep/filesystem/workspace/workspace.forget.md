---
method: "workspace.forget"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "workspace"
summary: "Deletes a fact of the project's memory (`.agents/memory/<name>.md`) the user wants gone"
scope: "write"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "name"
    type: "string"
    req: true
    desc: "The fact's name, e.g. `deploy-port`"
    example: "test-command"
  - name: "path"
    type: "string | null"
    req: false
    desc: "The project folder (default: the workspace)"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "workspace.forget",
    "params": {
      "name": "test-command"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "forgotten": "test-command",
      "path": "/project/.agents/memory/test-command.md"
    }
  }
errors:
  - code: -32002
    type: "forbidden"
    when: "The folder is outside the caller's roots"
  - code: -32004
    type: "fact_not_found"
    when: "No live fact of this name"
---

# workspace.forget

Delete a fact from a project's memory, for example when the user marks it wrong in the App. Call it only for a fact the user chose: the terminal agent's `forget` tool asks the same question before it deletes.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `workspace.forget` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| Side effects | Deletes `.agents/memory/<name>.md` |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. See [the common CEP contract](../../README.md).

## How it works

1. Resolve `path` (default: the workspace) and check it and its `.agents/memory/` folder are inside the caller's roots
2. Find the live fact named `name` (the name as listed by `workspace.memory`)
3. Delete its file

## Parameters

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `name` | string | Yes | The fact's name, e.g. `deploy-port` |
| `path` | string | No | The project folder (default: the workspace) |

Unknown parameters are rejected.

## Request examples

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "workspace.forget",
  "params": { "name": "test-command" }
}
```

## Response

`forgotten`: the name; `path`: the deleted file.

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token |
| `-32002` / principal | The folder is outside the caller's roots | Use an authorized caller or a folder inside its roots |
| `-32004` / `fact_not_found` | No live fact of this name | Read the names with `workspace.memory` |

## Limits and related operations

There is no undo in the Engine; in a git project the file's history is in git.

- [`workspace.remember`](workspace.remember.md)
- [`workspace.memory`](workspace.memory.md)

## Implementation reference

- [api/src/cep/methods/workspace.rs](../../../../../api/src/cep/methods/workspace.rs)
- [agents/src/workspace/memory.rs](../../../../../agents/src/workspace/memory.rs)
