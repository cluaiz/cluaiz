---
method: "memory.events"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "A chat's event log, oldest first"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "chat_id"
    type: "string"
    req: true
    desc: "The chat: the same value as `session_id` in chat completions"
    example: "c1"
  - name: "from_seq"
    type: "integer | null"
    req: false
    desc: "First seq to return (default 1)"
    example: 1
  - name: "limit"
    type: "integer | null"
    req: false
    desc: "Most events to return (default 500, at most 5000)"
    example: 2
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "memory.events",
    "params": {
      "chat_id": "c1",
      "from_seq": 1,
      "limit": 2
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "chat_id": "c1",
      "events": [
        {
          "data": {
            "role": "user",
            "text": "Hi"
          },
          "kind": "message",
          "seq": 1
        }
      ],
      "next_from_seq": 3,
      "skipped_lines": 0,
      "total": 40
    }
  }
errors:
  - code: -32010
    type: "event_log_unreadable"
    when: "The chat's event log could not be read"
---

# memory.events

Read the ordered event stream of a chat. This is the durable/history view used for replay and incremental synchronization, not just the latest assistant message or a model inference request. The selected store may be Engine disk or an App-provided external RAM log.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `memory.events` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Read the chat event store and count its readable events
2. Keep events whose sequence is at least from_seq, then take the requested limit
3. Return events, parse-skip diagnostics and the next sequence cursor

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Read the chat event store and count its readable events"]
    P0 --> P1["Keep events whose sequence is at least from_seq, then take the requested limit"]
    P1 --> P2["Return events, parse-skip diagnostics and the next sequence cursor"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `chat_id` | string | Yes | The chat: the same value as `session_id` in chat completions Use the same chat_id that the client sends as session_id to chat completions. A stream id, note id or title is not a chat id. |
| `from_seq` | integer \| null | No | First seq to return (default 1) Defaults to 1; inclusive lower sequence bound. Use next_from_seq from the preceding response. |
| `limit` | integer \| null | No | Most events to return (default 500, at most 5000) Defaults to 500; capped at 5000. A value of 0 requests no events, not an unlimited page. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "memory.events",
  "params": {
    "chat_id": "c1",
    "from_seq": 1,
    "limit": 2
  }
}
```

### 2. Continue a small page

Use the previous cursor; sequences start at 1 and the filter is inclusive.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "memory.events",
  "params": {
    "chat_id": "c1",
    "from_seq": 3,
    "limit": 2
  }
}
```

## Response and field meanings

total is the count read from the log before paging, not the returned page length. events is oldest first. skipped_lines reports malformed log lines skipped by the store. next_from_seq is last returned seq + 1 when the page reaches limit; otherwise null. A full last page can require one more request to discover the end.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "chat_id": "c1",
    "events": [
      {
        "data": {
          "role": "user",
          "text": "Hi"
        },
        "kind": "message",
        "seq": 1
      }
    ],
    "next_from_seq": 3,
    "skipped_lines": 0,
    "total": 40
  }
}
```

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32010` / `event_log_unreadable` | The chat's event log could not be read | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "memory.events",
  "params": {
    "chat_id": "c1",
    "from_seq": 1,
    "limit": 2,
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
  "method": "memory.events",
  "params": {
    "chat_id": "c1",
    "from_seq": 1,
    "limit": 2
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
  "method": "memory.events",
  "params": {
    "chat_id": "c1",
    "from_seq": 1,
    "limit": 2
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

A replay page is not a transaction snapshot across later writes. Preserve event seq and kind/data; do not assume every event is a visible chat message. Larger history can require multiple requests. This endpoint does not regenerate answers or search other chats.

- [`memory.export`](memory.export.md): use its separate documented contract.
- [`memory.index`](memory.index.md): use its separate documented contract.
- [`memory.item`](memory.item.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/memory.rs](../../../../../api/src/cep/methods/memory.rs)
