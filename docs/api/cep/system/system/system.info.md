---
method: "system.info"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "system"
summary: "Engine name and version, CEP version, hardware, and the LAN address"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "system.info",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "cep_version": 1,
      "engine": "Cluaiz Engine",
      "hardware": {
        "architecture": "x86_64",
        "cpu": {
          "brand": "AMD Ryzen 7 7435HS",
          "logical_threads": 16,
          "physical_cores": 8
        },
        "gpus": [
          {
            "compute_capability": "8.9",
            "index": 0,
            "name": "NVIDIA GeForce RTX 3050 A Laptop GPU",
            "vendor": "NVIDIA",
            "vram_total_gb": 4.0
          }
        ],
        "memory": {
          "available_gb": 7.47,
          "total_gb": 23.69
        },
        "os_name": "Windows",
        "os_version": "11"
      },
      "network": {
        "lan_ip": "192.168.1.20"
      },
      "version": "0.1.0"
    }
  }
errors: []
---

# system.info

Read Engine identity, protocol version and the machine hardware snapshot. This is a discovery/about operation, separate from checking whether an inference package can execute.

## Endpoint, execution and permissions

| Property | Contract |
|---|---|
| Wire endpoint | `POST /v1/cep`; logical method `system.info` |
| Execution | Synchronous domain result in the JSON-RPC result field |
| Required scope | `read` |
| MCP facade | Eligible for the current read-only MCP facade |
| Side effects | Read/control behavior described below; this scope alone is not a promise of zero side effects |

Send a Bearer token when API authentication is enabled. Authentication failures are HTTP 401; ordinary CEP method errors are JSON-RPC error objects, normally with HTTP 200. A successful HTTP status alone does not prove the operation succeeded. See [the common CEP contract](../../README.md).

## How it works

1. Read compiled Engine and CEP versions
2. Read shared HardwareProber snapshot
3. Determine the local LAN address when available and return identity/hardware/network

```mermaid
flowchart TD
    C["Client request"] --> A["Authentication, principal and typed params"]
    A --> P0["Read compiled Engine and CEP versions"]
    P0 --> P1["Read shared HardwareProber snapshot"]
    P1 --> P2["Determine the local LAN address when available and return identity/hardware/network"]
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
  "method": "system.info",
  "params": {}
}
```

## Response and field meanings

engine/version/cep_version identify software. hardware includes architecture, OS, CPU/memory/GPU information from the shared prober. network.lan_ip can be null if address discovery is unavailable; it is not proof the firewall/listener is reachable remotely.

The following response is an illustrative shape. IDs, byte counts, timings and array contents depend on the actual request and machine; shortened nested objects are not a second schema or a measured result.

```json
{
  "id": 1,
  "jsonrpc": "2.0",
  "result": {
    "cep_version": 1,
    "engine": "Cluaiz Engine",
    "hardware": {
      "architecture": "x86_64",
      "cpu": {
        "brand": "AMD Ryzen 7 7435HS",
        "logical_threads": 16,
        "physical_cores": 8
      },
      "gpus": [
        {
          "compute_capability": "8.9",
          "index": 0,
          "name": "NVIDIA GeForce RTX 3050 A Laptop GPU",
          "vendor": "NVIDIA",
          "vram_total_gb": 4
        }
      ],
      "memory": {
        "available_gb": 7.47,
        "total_gb": 23.69
      },
      "os_name": "Windows",
      "os_version": "11"
    },
    "network": {
      "lan_ip": "192.168.1.20"
    },
    "version": "0.1.0"
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
  "method": "system.info",
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
  "method": "system.info",
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
  "method": "system.info",
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

No model is loaded, no request port is opened by this method, and hardware metadata is not an admission/performance guarantee. The LAN lookup uses the socket’s selected local address. Do not expose host identifiers publicly merely because this is a read method.

- [`system.live`](system.live.md): use its separate documented contract.
- [`system.ps`](system.ps.md): use its separate documented contract.
- [`system.plan`](system.plan.md): use its separate documented contract.

## Implementation reference

- [api/src/cep/methods/system.rs](../../../../../api/src/cep/methods/system.rs)
