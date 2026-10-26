---
method: "tools.get_settings"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Read installed tool settings"
scope: "read"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: true
params:
  - name: "kind"
    type: "\"skill\" | \"plugin\" | \"mcp\""
    req: true
    desc: "skill, plugin or mcp"
    example: "mcp"
  - name: "name"
    type: "string"
    req: true
    desc: "The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens"
    example: "filesystem"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.get_settings",
    "params": {
      "kind": "mcp",
      "name": "filesystem"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "schema": {},
      "values": {
        "enabled": true,
        "execution_mode": "manual",
        "security_mode": "inherit",
        "trust_annotations": false
      }
    }
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens"
---

# tools.get_settings

The settings of one installed tool: `values` (`enabled`, `security_mode`, `execution_mode`, `trust_annotations`) and `schema` (the tool's own settings schema, when its manifest declares one).

| Setting | Values | Meaning |
|---|---|---|
| `enabled` | `true` / `false` | Offered to the model or not |
| `security_mode` | `inherit`, `require_approval`, `always_allow` | `inherit` follows the agent's security mode (`config` section `engine`); the others ask before every call, or never |
| `execution_mode` | `auto`, `manual`, `confirm_destructive` | When the agent may call it on its own |
| `trust_annotations` | `true` / `false` | Trust an MCP server's own read-only / destructive hints |

Not an MCP tool.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.get_settings","params":{"kind":"mcp","name":"filesystem"}}'
```

Leave out the `Authorization` header when API authentication is off.
