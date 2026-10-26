---
method: "tools.discover"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Dry-run the existing installed-tool matcher"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "explain"
    type: "boolean"
    req: false
    desc: "Also answer `evidence`: each matching tool's score, matched words and where they matched"
    example: null
  - name: "previous"
    type: "string | null"
    req: false
    desc: "The message before it (follow-ups such as \"and in Hindi?\" use it)"
    example: null
  - name: "query"
    type: "string"
    req: true
    desc: "A user message, as the chat would see it"
    example: "read notes.txt from my project"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.discover",
    "params": {
      "query": "read notes.txt from my project"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "actionable": true,
      "candidates": [
        {
          "category": "mcp",
          "id": "filesystem",
          "level": "card",
          "load_cost_tokens": 1650,
          "matched": [
            "read",
            "project",
            "file"
          ],
          "score": 9.56,
          "summary": "Official Model Context Protocol secure file system bridge"
        },
        {
          "category": "mcp",
          "id": "pynotes",
          "level": "title",
          "load_cost_tokens": 55,
          "matched": [
            "read"
          ],
          "score": 3.58,
          "summary": "Test notes server"
        }
      ],
      "installed": 10,
      "query": "read notes.txt from my project"
    }
  }
errors: []
---

# tools.discover

Preview how the installed-tool matcher would interpret a user message. This is a dry-run suggestion/debugging tool, distinct from model inference, package installation or actual tool execution.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `tools.discover` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Build the existing installed-tool search catalogue
2. Score the query with optional previous-message context
3. Return candidates/actionability and optional explanatory evidence

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Build the existing installed-tool search catalogue"]
    P0 --> P1["Score the query with optional previous-message context"]
    P1 --> P2["Return candidates/actionability and optional explanatory evidence"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `explain` | boolean | No | Also answer `evidence`: each matching tool's score, matched words and where they matched Defaults to false. true adds detailed evidence for matches. |
| `previous` | string \| null | No | The message before it (follow-ups such as "and in Hindi?" use it) Optional previous message for follow-up context; not a saved conversation id. |
| `query` | string | Yes | A user message, as the chat would see it Required user message to match. A filename/link is interpreted by the existing matcher rules, not blindly as keywords. |

Unknown top-level method parameters are rejected. `params._meta` is protocol metadata, not a domain option; approval retries must preserve the same method and domain parameters. Where a parameter is optional, omission follows the method-specific rules above rather than implying a universal zero or empty-string default.

## Request examples

Examples describe contracts, not measured execution. Replace IDs, paths and installed-package names with real values from your Engine. Do not execute write/delete/process examples against valuable data merely to read the documentation.

### 1. Basic request

This request uses the documented selection. Read the result semantics below before acting on it.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools.discover",
  "params": {
    "query": "read notes.txt from my project"
  }
}
```

### 2. Explain a follow-up match

The matcher can use the previous message; it does not fetch an Engine chat transcript implicitly.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools.discover",
  "params": {
    "query": "and in Hindi?",
    "previous": "Read the project notes.",
    "explain": true
  }
}
```

## Response and field meanings

query echoes the message; actionable describes the matcher’s classification, installed is catalogue size and candidates holds ranked card/title suggestions with costs/evidence. With explain:true, evidence is added. Ranking scores are not calibrated probabilities or a promise the model will call a tool.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "actionable": true,
    "candidates": [
      {
        "category": "mcp",
        "id": "filesystem",
        "level": "card",
        "load_cost_tokens": 1650,
        "matched": [
          "read",
          "project",
          "file"
        ],
        "score": 9.56,
        "summary": "Official Model Context Protocol secure file system bridge"
      },
      {
        "category": "mcp",
        "id": "pynotes",
        "level": "title",
        "load_cost_tokens": 55,
        "matched": [
          "read"
        ],
        "score": 3.58,
        "summary": "Test notes server"
      }
    ],
    "installed": 10,
    "query": "read notes.txt from my project"
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
  "method": "tools.discover",
  "params": {
    "query": "read notes.txt from my project",
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
  "method": "tools.discover",
  "params": {
    "query": "read notes.txt from my project"
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
  "method": "tools.discover",
  "params": {
    "query": "read notes.txt from my project"
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

This never installs, attaches or starts candidates. A dry-run result can change when installed packages/configuration change. It is not a vector search endpoint and has no model selector.

- [`tools.list`](tools.list.md): use its separate documented contract.
- [`chat.tools`](../../ai-inference/chat/chat.tools.md): use its separate documented contract.
- [`context.preview`](../../ai-inference/context/context.preview.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/tools/catalog.rs](../../../../../api/src/cep/methods/tools/catalog.rs)
