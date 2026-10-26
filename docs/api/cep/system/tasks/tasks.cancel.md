---
method: "tasks.cancel"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "tasks"
summary: "Cancels a running task"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "task_id"
    type: "string"
    req: true
    desc: "The `task.task_id` a task method answered with"
    example: "t_5f0c…"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tasks.cancel",
    "params": {
      "task_id": "t_5f0c…"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "status": "cancelled",
      "status_message": "Cancelled by request"
    }
  }
errors:
  - code: -32004
    type: "task_not_found"
    when: "No task with this id for this caller, or it expired"
  - code: -32602
    type: "invalid_params"
    when: "The task has already ended"
---

# tasks.cancel

Request cooperative cancellation of one still-running caller-owned task. This affects task state and supported worker cancellation flags; it is not a general kill-process command or an automatic rollback.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `tasks.cancel` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Verify task ownership, expiry and nonterminal state
2. Set the task cancellation flag and cancelled state/message
3. Notify waiters and return the updated task view

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Verify task ownership, expiry and nonterminal state"]
    P0 --> P1["Set the task cancellation flag and cancelled state/message"]
    P1 --> P2["Notify waiters and return the updated task view"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `task_id` | string | Yes | The `task.task_id` a task method answered with Required active task id. An already completed/failed/cancelled task returns invalid_params rather than repeatedly acknowledging cancellation. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tasks.cancel",
  "params": {
    "task_id": "t_5f0c…"
  }
}
```

### 2. Cancel a discovered running task

Choose a current id from tasks.list/get; do not substitute a model id.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tasks.cancel",
  "params": {
    "task_id": "<working-task-id>"
  }
}
```

## Response and field meanings

The result includes cancelled status and status_message, along with task identity/times in the task view. It is an acknowledgement of task cancellation, not proof that every native callback/process had already exited when the response was serialized.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "status": "cancelled",
    "status_message": "Cancelled by request"
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `task_not_found` | No task with this id for this caller, or it expired | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32602` / `invalid_params` | The task has already ended | Correct the option/value combination; install required assets where relevant |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "tasks.cancel",
  "params": {
    "task_id": "t_5f0c…",
    "__unknown_option": true
  }
}
```

The error envelope is `{"jsonrpc":"2.0","id":90,"error":{"code":-32602,"message":"...","data":{"type":"invalid_params", "errors":[...]}}}`. Exact message and field diagnostics are generated by typed deserialization; callers should use codes and structured data, not match a sentence.

## cURL

Save one valid request above as `request.json`:

```bash
curl --fail-with-body http://127.0.0.1:8000/v1/cep \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $CLUAIZ_TOKEN" \
  --data-binary @request.json
```

## Python

Install `requests` in your client environment. This executes the basic request; edit `params` for a different valid case.

```python
import json
import os
import requests

payload = json.loads(r'''{
  "id": 1,
  "jsonrpc": "2.0",
  "method": "tasks.cancel",
  "params": {
    "task_id": "t_5f0c…"
  }
}''')
response = requests.post(
    "http://127.0.0.1:8000/v1/cep",
    headers={"Authorization": "Bearer " + os.environ["CLUAIZ_TOKEN"]},
    json=payload, timeout=60,
)
response.raise_for_status()
reply = response.json()
if "error" in reply:
    raise RuntimeError(json.dumps(reply["error"], ensure_ascii=False))
print(reply["result"])
```

## JavaScript / Node.js

Use an owner token only in a trusted client. Do not ship it inside a public web page.

```javascript
const payload = {
  "id": 1,
  "jsonrpc": "2.0",
  "method": "tasks.cancel",
  "params": {
    "task_id": "t_5f0c…"
  }
};
const response = await fetch("http://127.0.0.1:8000/v1/cep", {
  method: "POST",
  headers: { "content-type": "application/json", authorization: `Bearer ${process.env.CLUAIZ_TOKEN}` },
  body: JSON.stringify(payload),
});
const raw = await response.text();
if (!response.ok) throw new Error(`HTTP ${response.status}: ${raw}`);
const reply = JSON.parse(raw);
if (reply.error) throw new Error(JSON.stringify(reply.error));
console.log(reply.result);
```

## Limits, state and related operations

Worker owners observe cancellation at their supported boundaries. Already downloaded/installed/written data may remain; inspect the corresponding inventory/state. A task created by another principal is task_not_found, not something this method can cancel through an owner-supplied principal parameter.

- [`tasks.get`](tasks.get.md): use its separate documented contract.
- [`tasks.result`](tasks.result.md): use its separate documented contract.
- [`tasks.list`](tasks.list.md): use its separate documented contract.

## Implementation reference

- [cep/src/protocol.rs](../../../../../cep/src/protocol.rs)
- [cep/src/task.rs](../../../../../cep/src/task.rs)
