---
method: "notes.create"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "notes"
summary: "A note written by the user, confirmed at once; it supersedes a live note on the same subject"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "chat_id"
    type: "string | null"
    req: false
    desc: "The chat, for scope chat"
    example: null
  - name: "pinned"
    type: "boolean"
    req: false
    desc: "Pinned notes go into the system prompt of new chats"
    example: null
  - name: "scope"
    type: "string | null"
    req: false
    desc: "chat, workspace (default) or global"
    example: "workspace"
  - name: "text"
    type: "string"
    req: true
    desc: "1 to 2000 characters"
    example: "Prefers tabs over spaces"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "notes.create",
    "params": {
      "scope": "workspace",
      "text": "Prefers tabs over spaces"
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
  - code: -32602
    type: "invalid_params"
    when: "`text` is empty or over 2000 characters, `scope` is unknown, or scope chat has no `chat_id`"
  - code: -32010
    type: "notes_unavailable"
    when: "The notes file could not be written"
---

# notes.create

Persist a user-authored fact or preference beyond the immediate message. User notes are confirmed on creation and can supersede an existing live note on the same subject; this is not a model-generated proposal awaiting confirmation.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `notes.create` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Trim and validate text, select the scope and owner
2. Add a confirmed user note through the shared NoteStore
3. Return the new note and ids/records superseded by subject matching

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Trim and validate text, select the scope and owner"]
    P0 --> P1["Add a confirmed user note through the shared NoteStore"]
    P1 --> P2["Return the new note and ids/records superseded by subject matching"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `chat_id` | string \| null | No | The chat, for scope chat Required when scope is chat; identifies the scope owner. Does not select a different current workspace. |
| `pinned` | boolean | No | Pinned notes go into the system prompt of new chats Defaults to false. Pinned confirmed notes can enter the system prompt of new chats. |
| `scope` | string \| null | No | chat, workspace (default) or global Defaults to workspace. Allowed chat, workspace, global. |
| `text` | string | Yes | 1 to 2000 characters After trimming, non-empty and at most 2000 UTF-8 BYTES in the current implementation (String.len), despite the brief schema description saying characters. Multibyte text reaches the boundary earlier. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "notes.create",
  "params": {
    "scope": "workspace",
    "text": "Prefers tabs over spaces"
  }
}
```

### 2. Keep a global preference

This is confirmed user input and can affect new chats across workspaces.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "notes.create",
  "params": {
    "text": "Prefer concise explanations.",
    "scope": "global",
    "pinned": true
  }
}
```

### 3. Keep a chat-specific fact

The chat id is mandatory for this scope.

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "notes.create",
  "params": {
    "text": "The local development port is 1420.",
    "scope": "chat",
    "chat_id": "c1"
  }
}
```

## Response and field meanings

note is the stored record with its generated id/status/scope. superseded lists previous live notes displaced by the note-store subject rules; it is not a general delete-all operation. Reuse note.id in notes.pin/confirm/invalidate.

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
| `-32602` / `invalid_params` | `text` is empty or over 2000 characters, `scope` is unknown, or scope chat has no `chat_id` | Correct the option/value combination; install required assets where relevant |
| `-32010` / `notes_unavailable` | The notes file could not be written | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "notes.create",
  "params": {
    "scope": "workspace",
    "text": "Prefers tabs over spaces",
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
  "method": "notes.create",
  "params": {
    "scope": "workspace",
    "text": "Prefers tabs over spaces"
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
  "method": "notes.create",
  "params": {
    "scope": "workspace",
    "text": "Prefers tabs over spaces"
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

Writing a pinned note can affect future prompts; existing loaded sessions are not retroactively rewritten. No custom note id or arbitrary status can be supplied. Unicode size is byte-based here. Store write errors surface as notes_unavailable rather than fake persistence.

- [`notes.list`](notes.list.md): use its separate documented contract.
- [`notes.pin`](notes.pin.md): use its separate documented contract.
- [`notes.invalidate`](notes.invalidate.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/notes.rs](../../../../../api/src/cep/methods/notes.rs)
