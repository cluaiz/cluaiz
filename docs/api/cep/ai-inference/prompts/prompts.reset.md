---
method: "prompts.reset"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "prompts"
summary: "Goes back to the default text; the user's text is kept in the history"
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
    desc: "The prompt's id (from `prompts.list`), e.g. `agent.system`"
    example: "tools.workspace"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "prompts.reset",
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
  - code: -32010
    type: "prompt_not_saved"
    when: "The prompt file could not be removed"
---

# prompts.reset

Go back to the default text of a prompt. The user's text is kept in the history, so it can be sent again with `prompts.set`.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `prompts.reset` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `write` |
| Side effects | Removes `<CLUAIZ_HOME>/prompts/<id>.md` after copying it to the history |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. See [the common CEP contract](../../README.md).

## How it works

1. Find the prompt by id
2. Copy the user's file, if any, into the history
3. Remove the user's file
4. Return the default prompt

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Find the prompt by id"]
    P0 --> P1["Copy the user's file, if any, into the history"]
    P1 --> P2["Remove the user's file"]
    P2 --> P3["Return the default prompt"]
    P3 --> R["JSON-RPC result or explicit error"]
```

## Parameters

| Parameter | Type | Required | Meaning and constraints |
|---|---|---|---|
| `id` | string | Yes | The prompt's id from `prompts.list`. Resetting a prompt that has no user text changes nothing and returns the default. |

Unknown parameters are rejected.

## Request examples

### 1. Basic request

Back to the default tools folder line.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "prompts.reset",
  "params": {
    "id": "tools.workspace"
  }
}
```

## Response

`prompt` is the default, with `overridden: false`.

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token; a CEP params change cannot repair authentication |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page; do not retry the same invalid payload |
| `-32002` / principal | Required scope/owner/root authorization is absent | Inspect `error.data.needs`; use an authorized caller or permitted target |
| `-32004` / `prompt_not_found` | No prompt with this id | Correct the request or the condition named in the message, then retry |
| `-32010` / `prompt_not_saved` | The prompt file could not be removed | Correct the request or the condition named in the message, then retry |

## cURL

Save one valid request above as `request.json`:

```bash
curl --fail-with-body http://127.0.0.1:8000/v1/cep \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $CLUAIZ_TOKEN" \
  --data-binary @request.json
```

## Limits and related operations

Only one prompt per call. The history keeps the last 20 texts of each prompt.

- [`prompts.set`](prompts.set.md)
- [`prompts.history`](prompts.history.md)

## Implementation reference

- [api/src/cep/methods/prompts.rs](../../../../../api/src/cep/methods/prompts.rs)
- [engine/core/src/prompts.rs](../../../../../engine/core/src/prompts.rs)
- [engine/core/prompts/](../../../../../engine/core/prompts/)
