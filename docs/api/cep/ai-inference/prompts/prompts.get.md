---
method: "prompts.get"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "prompts"
summary: "One prompt and the text in use"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "id"
    type: "string"
    req: true
    desc: "The prompt's id (from `prompts.list`), e.g. `agent.system`"
    example: "tools.workspace"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "prompts.get",
    "params": {
      "id": "tools.workspace"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "prompt": {
        "description": "Tells a chat model with tools which folder they run in.",
        "id": "tools.workspace",
        "overridden": false,
        "text": "Tools run in the project folder {{folder}}. Use paths relative to it (\".\" is the project folder).",
        "variables": [
          "folder"
        ]
      }
    }
  }
errors:
  - code: -32004
    type: "prompt_not_found"
    when: "No prompt with this id"
---

# prompts.get

Read one prompt: its description, its variables and the text in use now.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `prompts.get` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| Side effects | Read only |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. See [the common CEP contract](../../README.md).

## How it works

1. Find the prompt by id
2. Use the user's file when it exists and can be read, else the default
3. Return the prompt

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Find the prompt by id"]
    P0 --> P1["Use the user's file when it exists and can be read, else the default"]
    P1 --> P2["Return the prompt"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters

| Parameter | Type | Required | Meaning and constraints |
|---|---|---|---|
| `id` | string | Yes | The prompt's id from `prompts.list`, for example `agent.system`. An unknown id is `prompt_not_found`. |

Unknown parameters are rejected.

## Request examples

### 1. Basic request

Reads the tools folder line.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "prompts.get",
  "params": {
    "id": "tools.workspace"
  }
}
```

### 2. The terminal agent's instructions

Reads the agent's system prompt.

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "prompts.get",
  "params": {
    "id": "agent.system"
  }
}
```

## Response

`prompt` is the same object `prompts.list` returns for this id. `text` shows `{{name}}` placeholders as they are; the Engine fills them when it uses the prompt.

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `prompt_not_found` | No prompt with this id | Correct the request or the condition named in the message, then retry |

## cURL

Save one valid request above as `request.json`:

```bash
curl --fail-with-body http://127.0.0.1:8000/v1/cep \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $CLUAIZ_TOKEN" \
  --data-binary @request.json
```

## Limits and related operations

A user file that cannot be read is logged and the default is returned with `overridden: false`.

- [`prompts.list`](prompts.list.md)
- [`prompts.set`](prompts.set.md)
- [`prompts.history`](prompts.history.md)

## Implementation reference

- [api/src/cep/methods/prompts.rs](../../../../../api/src/cep/methods/prompts.rs)
- [engine/core/src/prompts.rs](../../../../../engine/core/src/prompts.rs)
- [engine/core/prompts/](../../../../../engine/core/prompts/)
