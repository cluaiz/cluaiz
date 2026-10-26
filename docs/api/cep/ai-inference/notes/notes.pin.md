---
method: "notes.pin"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "notes"
summary: "Pins or unpins a note; pinned notes go into the system prompt of new chats"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "id"
    type: "string"
    req: true
    desc: "The note's `id` (from `notes.list`)"
    example: "n7"
  - name: "pinned"
    type: "boolean"
    req: true
    desc: "true: goes into the system prompt of new chats; false: does not"
    example: true
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "notes.pin",
    "params": {
      "id": "n7",
      "pinned": true
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "note": {
        "id": "n7",
        "pinned": false,
        "scope": "workspace",
        "status": "confirmed",
        "text": "Prefers tabs over spaces"
      },
      "superseded": []
    }
  }
errors:
  - code: -32004
    type: "note_not_found"
    when: "No note with this id"
  - code: -32010
    type: "notes_unavailable"
    when: "The notes file could not be written"
---

# notes.pin

Change whether a stored note is pinned for future prompt construction. Pinning is a presentation/context policy, not confirmation, text editing or a global toggle for every note.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `notes.pin` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Locate the note by id
2. Update the pinned boolean while retaining other fields
3. Persist and return the updated note

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Locate the note by id"]
    P0 --> P1["Update the pinned boolean while retaining other fields"]
    P1 --> P2["Persist and return the updated note"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `id` | string | Yes | The note's `id` (from `notes.list`) Exact id from notes.list. |
| `pinned` | boolean | Yes | true: goes into the system prompt of new chats; false: does not Required boolean; true pins, false unpins. Omission is not interpreted as a toggle. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "notes.pin",
  "params": {
    "id": "n7",
    "pinned": true
  }
}
```

### 2. Unpin a note

Keeps the note record and content; only pinned state is changed.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "notes.pin",
  "params": {
    "id": "<id-from-notes.list>",
    "pinned": false
  }
}
```

## Response and field meanings

note.pinned is the saved state. The response also includes superseded from shared note updates. Read note.status and scope too: pinning alone does not change the note into a newly confirmed record.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "note": {
      "id": "n7",
      "pinned": false,
      "scope": "workspace",
      "status": "confirmed",
      "text": "Prefers tabs over spaces"
    },
    "superseded": []
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `note_not_found` | No note with this id | Refresh the relevant inventory/IDs and check whether the resource expired |
| `-32010` / `notes_unavailable` | The notes file could not be written | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "notes.pin",
  "params": {
    "id": "n7",
    "pinned": true,
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
  "method": "notes.pin",
  "params": {
    "id": "n7",
    "pinned": true
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
  "method": "notes.pin",
  "params": {
    "id": "n7",
    "pinned": true
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

Pinned notes influence construction of new chats/prompts according to lifecycle/scope rules. This does not reload a model or rewrite an answer already streaming. Use explicit false to unpin, rather than sending an empty patch.

- [`notes.list`](notes.list.md): use its separate documented contract.
- [`notes.confirm`](notes.confirm.md): use its separate documented contract.
- [`notes.create`](notes.create.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/notes.rs](../../../../../api/src/cep/methods/notes.rs)
