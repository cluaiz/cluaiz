---
method: "rpc.discover"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "rpc"
summary: "The OpenRPC description of the methods you may call"
scope: "read"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: true
params:
  - name: "group"
    type: "string | null"
    req: false
    desc: "Only this group, for example \"fs\""
    example: "tasks"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "rpc.discover",
    "params": {
      "group": "tasks"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "info": {
        "title": "Cluaiz Engine Protocol"
      },
      "methods": [],
      "openrpc": "1.3.2"
    }
  }
errors: []
---

# rpc.discover

The [OpenRPC 1.3.2](https://spec.open-rpc.org/) description of the methods the caller may call: each method's params schema, result, errors, an example, and `x-cep` (`scope`, `execution`, `destructive`, `idempotent`, `mcp`, `owner_only`). With `group`, only that group's methods.

A caller only sees what it may call: the MCP principal sees read methods, the owner sees all of them. The docs in this folder and [`openrpc.json`](../../openrpc.json) are generated from the owner's answer.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"rpc.discover","params":{"group":"fs"}}'
```

Leave out the `Authorization` header when API authentication is off.
