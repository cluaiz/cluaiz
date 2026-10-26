---
method: "approvals.list"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "approvals"
summary: "Approval requests waiting for the owner"
scope: "admin"
execution: "sync"
mcp: false
owner_only: true
destructive: false
idempotent: false
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "approvals.list",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "pending": [
        {
          "created_at": "2026-10-25T10:30:00Z",
          "expires_at": "2026-10-25T10:35:00Z",
          "id": "ap_41c2…",
          "method": "mcp:web-search",
          "principal": "agent",
          "status": "pending",
          "target": "{\"query\":\"news\"}"
        }
      ]
    }
  }
errors: []
---

# approvals.list

Read pending owner approval requests shared by chat tools, explicit host execution and callers requesting access outside their roots. These requests are distinct from remembered chat grants.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `approvals.list` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `admin`; owner principal required |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Read the shared approval store
2. Select entries still pending and not expired
3. Return pending request identity and decision context

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Read the shared approval store"]
    P0 --> P1["Select entries still pending and not expired"]
    P1 --> P2["Return pending request identity and decision context"]
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
  "method": "approvals.list",
  "params": {}
}
```

## Response and field meanings

pending[] includes an id, principal/method/target, creation/expiry times and status. Use the actual id as approval_id for a decision. A returned target is informational decision context, not an instruction the client should execute blindly.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "pending": [
      {
        "created_at": "2026-10-25T10:30:00Z",
        "expires_at": "2026-10-25T10:35:00Z",
        "id": "ap_41c2…",
        "method": "mcp:web-search",
        "principal": "agent",
        "status": "pending",
        "target": "{\"query\":\"news\"}"
      }
    ]
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
  "method": "approvals.list",
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
  "method": "approvals.list",
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
  "method": "approvals.list",
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

Owner and admin scope required. Requests normally expire after five minutes. A list is a snapshot: another owner UI can decide a request before your next call. No automatic approve-all or filter parameter exists.

- [`approvals.approve`](approvals.approve.md): use its separate documented contract.
- [`approvals.reject`](approvals.reject.md): use its separate documented contract.
- [`memory.grants`](../../ai-inference/memory/memory.grants.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/approvals.rs](../../../../../api/src/cep/methods/approvals.rs)
