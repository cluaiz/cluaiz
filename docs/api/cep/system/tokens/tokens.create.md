---
method: "tokens.create"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "tokens"
summary: "Makes a new API key (at most 5)"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tokens.create",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "api_auth_required": false,
      "token": "sk-cluaiz-1f0c…",
      "tokens": [
        "sk-cluaiz-1f0c…"
      ]
    }
  }
errors:
  - code: -32005
    type: "token_limit"
    when: "There are 5 keys already: revoke one first"
  - code: -32010
    type: "settings_not_saved"
    when: "permission.json could not be written"
---

# tokens.create

Generate and save one API key using the existing permission owner. Key creation and switching API authentication on are separate actions; this call does not automatically turn the toggle on.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `tokens.create` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `admin` |
| MCP facade | Not exposed by the current read-only MCP facade |
| Side effects | Updates the state described below |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Check the maximum of five stored keys
2. Generate, sanitize and save the updated key list
3. Replace live permission state and return the new token plus inventory

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Check the maximum of five stored keys"]
    P0 --> P1["Generate, sanitize and save the updated key list"]
    P1 --> P2["Replace live permission state and return the new token plus inventory"]
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
  "method": "tokens.create",
  "params": {}
}
```

## Response and field meanings

token is the newly generated usable secret. tokens is the saved current inventory and api_auth_required remains the saved toggle. Store token securely in the client; the displayed shortened example is not a real credential.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "api_auth_required": false,
    "token": "sk-cluaiz-1f0c…",
    "tokens": [
      "sk-cluaiz-1f0c…"
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
| `-32005` / `token_limit` | There are 5 keys already: revoke one first | Inspect current state before retrying; avoid a tight retry loop |
| `-32010` / `settings_not_saved` | permission.json could not be written | Read the error message, correct the backend/filesystem/configuration condition, then retry deliberately |

### Invalid-field example

This is deliberately invalid. The extra field must be removed; it is not an alternate feature toggle.

```text
{
  "jsonrpc": "2.0",
  "id": 90,
  "method": "tokens.create",
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
  "method": "tokens.create",
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
  "method": "tokens.create",
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

At five keys, revoke one first. A save failure is settings_not_saved, not a success you should retry blindly without inspecting inventory. This does not create a separate principal/scope per key in this documentation or rotate the Engine session token.

- [`tokens.list`](tokens.list.md): use its separate documented contract.
- [`tokens.revoke`](tokens.revoke.md): use its separate documented contract.
- [`config.set`](../config/config.set.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/tokens.rs](../../../../../api/src/cep/methods/tokens.rs)
