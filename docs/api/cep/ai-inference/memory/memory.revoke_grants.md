---
method: "memory.revoke_grants"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "Asks again for this chat's tools (or one tool)"
scope: "write"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "chat_id"
    type: "string"
    req: true
    desc: "The chat: the same value as `session_id` in chat completions"
    example: "c1"
  - name: "tool"
    type: "string | null"
    req: false
    desc: "Only this tool; all of the chat's grants when absent"
    example: "web-search"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "memory.revoke_grants",
    "params": {
      "chat_id": "c1",
      "tool": "web-search"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "chat_id": "c1",
      "removed": 1
    }
  }
errors: []
---

# memory.revoke_grants

Remove remembered tool permission for one chat, forcing later tool use to be assessed again. This does not uninstall, detach or stop the tool, and it is narrower than deleting the chat history.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `memory.revoke_grants` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Select all grants or a named tool’s grants for chat_id
2. Revoke the selected remembered grants
3. Append a revocation event when something changed and return the count

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Select all grants or a named tool’s grants for chat_id"]
    P0 --> P1["Revoke the selected remembered grants"]
    P1 --> P2["Append a revocation event when something changed and return the count"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `chat_id` | string | Yes | The chat: the same value as `session_id` in chat completions Use the same chat_id that the client sends as session_id to chat completions. A stream id, note id or title is not a chat id. |
| `tool` | string \| null | No | Only this tool; all of the chat's grants when absent Omitted: all remembered grants of this chat. Present: only the exact tool id. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "memory.revoke_grants",
  "params": {
    "chat_id": "c1",
    "tool": "web-search"
  }
}
```

### 2. Revoke one tool

Other remembered tools in the same conversation remain unchanged.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "memory.revoke_grants",
  "params": {
    "chat_id": "c1",
    "tool": "web-search"
  }
}
```

### 3. Revoke every remembered tool in the chat

Only this conversation is targeted.

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "memory.revoke_grants",
  "params": {
    "chat_id": "c1"
  }
}
```

## Response and field meanings

removed counts grants actually revoked. Zero is a valid result when no matching live grant exists. The returned chat_id identifies the conversation whose remembered permission changed.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "chat_id": "c1",
    "removed": 1
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
  "method": "memory.revoke_grants",
  "params": {
    "chat_id": "c1",
    "tool": "web-search",
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
  "method": "memory.revoke_grants",
  "params": {
    "chat_id": "c1",
    "tool": "web-search"
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
  "method": "memory.revoke_grants",
  "params": {
    "chat_id": "c1",
    "tool": "web-search"
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

Revocation applies to remembered authorization, not necessarily a tool call already in progress. It does not remove pending approval records or reset another conversation. The revocation is logged when entries were removed so replay should not restore them.

- [`memory.grants`](memory.grants.md): use its separate documented contract.
- [`approvals.approve`](../../agents-tools/approvals/approvals.approve.md): use its separate documented contract.
- [`chat.set_tools`](../chat/chat.set_tools.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/memory.rs](../../../../../api/src/cep/methods/memory.rs)
