---
method: "approvals.reject"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "approvals"
summary: "Refuses the waiting call"
scope: "admin"
execution: "sync"
mcp: false
owner_only: true
destructive: false
idempotent: false
params:
  - name: "approval_id"
    type: "string"
    req: true
    desc: "The `id` of the pending approval"
    example: "ap_41c2…"
  - name: "feedback"
    type: "string | null"
    req: false
    desc: "Passed to the waiting call (the model sees it as the reason)"
    example: "Not this site"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "approvals.reject",
    "params": {
      "approval_id": "ap_41c2…",
      "feedback": "Not this site"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "approval": {
        "created_at": "2026-10-25T10:30:00Z",
        "expires_at": "2026-10-25T10:35:00Z",
        "id": "ap_41c2…",
        "method": "mcp:web-search",
        "principal": "agent",
        "status": "rejected",
        "target": "{\"query\":\"news\"}"
      }
    }
  }
errors:
  - code: -32004
    type: "approval_not_found"
    when: "No pending approval with this id, or it expired (5 minutes)"
  - code: -32005
    type: "approval_decided"
    when: "The approval was already approved or rejected"
---

# approvals.reject

Refuse one pending request and optionally provide a reason to the waiting model/caller. It does not execute the target, revoke old remembered grants or delete the pending request’s underlying chat.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `approvals.reject` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `admin`; owner principal required |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Find the pending shared approval record
2. Record the owner’s negative decision and feedback
3. Return the decided approval record

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Find the pending shared approval record"]
    P0 --> P1["Record the owner’s negative decision and feedback"]
    P1 --> P2["Return the decided approval record"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `approval_id` | string | Yes | The `id` of the pending approval Required pending id from the shared store. |
| `feedback` | string \| null | No | Passed to the waiting call (the model sees it as the reason) Optional reason passed to the waiting call; do not embed secrets merely as feedback. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "approvals.reject",
  "params": {
    "approval_id": "ap_41c2…",
    "feedback": "Not this site"
  }
}
```

### 2. Refuse with a reason

Use the returned id, not a target description as the identifier.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "approvals.reject",
  "params": {
    "approval_id": "<pending-id>",
    "feedback": "The selected folder is not intended for this operation."
  }
}
```

## Response and field meanings

approval identifies the decided request and its status. The original waiter/retried call handles the refusal; this response is not a tool execution result.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "approval": {
      "created_at": "2026-10-25T10:30:00Z",
      "expires_at": "2026-10-25T10:35:00Z",
      "id": "ap_41c2…",
      "method": "mcp:web-search",
      "principal": "agent",
      "status": "pending",
      "target": "{\"query\":\"news\"}"
    }
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `approval_not_found` | No pending approval with this id, or it expired (5 minutes) | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32005` / `approval_decided` | The approval was already approved or rejected | Inspect current state before retrying; avoid a tight retry loop |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "approvals.reject",
  "params": {
    "approval_id": "ap_41c2…",
    "feedback": "Not this site",
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
  "method": "approvals.reject",
  "params": {
    "approval_id": "ap_41c2…",
    "feedback": "Not this site"
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
  "method": "approvals.reject",
  "params": {
    "approval_id": "ap_41c2…",
    "feedback": "Not this site"
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

Owner/admin required. Unknown/expired requests return approval_not_found and already-decided requests return approval_decided. To revoke earlier remembered permission, use memory.revoke_grants rather than rejecting an unrelated pending id.

- [`approvals.list`](approvals.list.md): use its separate documented contract.
- [`approvals.approve`](approvals.approve.md): use its separate documented contract.
- [`memory.revoke_grants`](../../ai-inference/memory/memory.revoke_grants.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/approvals.rs](../../../../../api/src/cep/methods/approvals.rs)
