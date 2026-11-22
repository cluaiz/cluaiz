---
method: "providers.remove"
transport: "POST /v1/cep"
category: "models"
category_title: "Models & Engine Vault"
group: "providers"
summary: "Removes a provider, with its key if it was kept in the file"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: true
idempotent: false
params:
  - name: "name"
    type: "string"
    req: true
    desc: "Provider name"
    example: "myapi"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "providers.remove",
    "params": {
      "name": "myapi"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "removed": "myapi"
    }
  }
errors:
  - code: -32004
    type: "provider_not_found"
    when: "No provider has this name"
  - code: -32010
    type: "settings_not_saved"
    when: "providers.json could not be written"
---

# providers.remove

Removes a provider from `config/providers.json`. A key kept in the file goes with it; an environment variable named in `api_key_env` is not touched.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `providers.remove` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `admin` |
| MCP facade | Not exposed: it reads or changes the provider settings |
| Side effects | Writes `config/providers.json` without this provider |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. See [the common CEP contract](../../README.md).

## How it works

1. Read providers.json
2. Find the provider by name
3. Remove it and save the file

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Read providers.json"]
    P0 --> P1["Find the provider by name"]
    P1 --> P2["Remove it and save the file"]
    P2 --> R["JSON-RPC result or explicit error"]
```

## Parameters, defaults and value effects

| Parameter | Type | Required | Meaning, default and constraints |
|---|---|---|---|
| `name` | string | Yes | The provider's name, as `providers.list` shows it |

Unknown parameters are rejected.

## Request examples

### 1. Basic request

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "providers.remove",
  "params": {
    "name": "myapi"
  }
}
```

## Response and field meanings

`removed`: the name of the removed provider.

## Errors and recovery

| Code / boundary | Meaning | Recovery |
|---|---|---|
| HTTP 401 | Missing/invalid token while authentication is enabled | Supply the configured token |
| `-32602` / typed params | Missing required fields, wrong JSON types or unknown fields | Correct the request against this page |
| `-32002` / principal | The caller has no `admin` scope | Use an owner or admin token |
| `-32004` / `provider_not_found` | No provider has this name | Read the names with `providers.list` |
| `-32010` / `providers_unreadable` | providers.json exists but could not be read or parsed | Fix or move the file in the Cluaiz `config/` folder, then retry |
| `-32010` / `settings_not_saved` | providers.json could not be written | Check the `config/` folder's permissions and free space; the file is unchanged |

## cURL

Save one valid request above as `request.json`:

```bash
curl --fail-with-body http://127.0.0.1:8000/v1/cep \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $CLUAIZ_TOKEN" \
  --data-binary @request.json
```

## Python

Install `requests` in your client environment. This sends the first request above.

```python
import json
import os
import requests

payload = {"jsonrpc": "2.0", "id": 1, "method": "providers.remove", "params": {"name": "myapi"}}
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
const payload = { jsonrpc: "2.0", id: 1, method: "providers.remove", params: { name: "myapi" } };
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

There is no undo: send `providers.set` again to add it back. Requests for the removed provider's models then fail as for any unknown model.

- [`providers.list`](providers.list.md), [`providers.set`](providers.set.md)

## Implementation reference

- [api/src/cep/methods/providers.rs](../../../../../api/src/cep/methods/providers.rs)
- [engine/core/src/providers.rs](../../../../../engine/core/src/providers.rs)
