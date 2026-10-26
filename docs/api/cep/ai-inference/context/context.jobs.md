---
method: "context.jobs"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "context"
summary: "The idle-time scheduler's queue, counts and recent results"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "context.jobs",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "counts": {
        "done": 12,
        "failed": 0,
        "skipped": 0,
        "stopped": 0
      },
      "indexer": "auto",
      "queue": [
        {
          "answer_pos": 3,
          "chat_id": "c1",
          "kind": "cold"
        }
      ],
      "recent": [
        {
          "answer_pos": 5,
          "chat": "c2",
          "cold": false,
          "ms": 8017,
          "outcome": "done"
        }
      ],
      "requests_in_flight": 0,
      "running": false
    }
  }
errors: []
---

# context.jobs

Inspect the existing idle-time indexing scheduler. This reports its queue and recent outcomes; it does not start a job, poll a CEP TaskStore job or create a separate worker pool.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `context.jobs` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Read the background context owner’s current scheduler state
2. Project pending/current work and cumulative outcome counts
3. Return recent outcomes and the active indexing policy

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Read the background context owner’s current scheduler state"]
    P0 --> P1["Project pending/current work and cumulative outcome counts"]
    P1 --> P2["Return recent outcomes and the active indexing policy"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `params` | object | No fields | Use `{}`. Unknown fields are rejected; do not invent filters or target identifiers. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "context.jobs",
  "params": {}
}
```

## Response and field meanings

indexer is the policy mode. requests_in_flight/running describe scheduling activity. queue entries identify chat/answer positions and work kind. counts aggregates done/failed/stopped/skipped outcomes; recent holds recent rows/timings, not a durable complete history.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "counts": {
      "done": 12,
      "failed": 0,
      "skipped": 0,
      "stopped": 0
    },
    "indexer": "auto",
    "queue": [
      {
        "answer_pos": 3,
        "chat_id": "c1",
        "kind": "cold"
      }
    ],
    "recent": [
      {
        "answer_pos": 5,
        "chat": "c2",
        "cold": false,
        "ms": 8017,
        "outcome": "done"
      }
    ],
    "requests_in_flight": 0,
    "running": false
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "context.jobs",
  "params": {
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
  "method": "context.jobs",
  "params": {}
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
  "method": "context.jobs",
  "params": {}
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

This is a snapshot and can change immediately after it is read. Indexing policy lives in config section context; queue items are not tasks.get identifiers. No run_now, cancel or priority parameter exists here. Do not treat a completed background card as a new visible assistant reply.

- [`config.get`](../../system/config/config.get.md): use its separate documented contract.
- [`memory.index`](../memory/memory.index.md): use its separate documented contract.
- [`context.reports`](context.reports.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/context.rs](../../../../../api/src/cep/methods/context.rs)
