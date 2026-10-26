---
method: "system.audit"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "system"
summary: "The newest lines of the CEP audit log, newest first"
scope: "admin"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "limit"
    type: "integer | null"
    req: false
    desc: "How many of the newest lines (default 100, at most 1000)"
    example: 1
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "system.audit",
    "params": {
      "limit": 1
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "lines": [
        {
          "code": null,
          "method": "fs.write",
          "ms": 3,
          "params_sha256": "9f2c…",
          "principal": "owner",
          "scope": "write",
          "ts": "2026-10-25T10:30:00.000Z"
        }
      ],
      "path": "C:/Users/me/.cluaiz/cep-audit.jsonl"
    }
  }
errors: []
---

# system.audit

The newest lines of the CEP audit log, newest first.

## What is logged

One JSON line per call that writes, executes or administers (`scope` write, exec or admin), and per denied call of any scope (`-32001`, `-32002`, `-32003`). Read calls that succeed are not logged. The params are kept only as a SHA-256 hash, never as text, so file contents, tokens and chat text never reach the log.

The log is `cep-audit.jsonl` in the Engine home. It is rotated past 10 MB, and three old files are kept (`.1` is the newest).

## Line fields

| Field | Meaning |
|---|---|
| `ts` | When the call ended (UTC) |
| `principal` | Who called: `owner`, `mcp`, … |
| `method`, `scope` | The method and its scope |
| `params_sha256` | Hash of the params |
| `code` | `null` on success, else the error code |
| `ms` | How long the call took |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.audit","params":{"limit":20}}'
```

Leave out the `Authorization` header when API authentication is off.
