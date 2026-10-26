---
method: "tools.set_settings"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Validate and save a tool settings patch"
scope: "write"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "kind"
    type: "\"skill\" | \"plugin\" | \"mcp\""
    req: true
    desc: "skill, plugin or mcp"
    example: "skill"
  - name: "name"
    type: "string"
    req: true
    desc: "The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens"
    example: "installed-tool"
  - name: "patch"
    type: "object"
    req: true
    desc: "Only the fields to change"
    example: {"enabled":true}
    fields:
      - name: "enabled"
        type: "boolean | null"
        req: false
        desc: "false: the tool is not offered to the model (an MCP server is also stopped)"
        example: true
      - name: "execution_mode"
        type: "string | null"
        req: false
        desc: "auto, manual or confirm_destructive"
        example: null
      - name: "security_mode"
        type: "string | null"
        req: false
        desc: "inherit, require_approval or always_allow"
        example: null
      - name: "trust_annotations"
        type: "boolean | null"
        req: false
        desc: "Trust the MCP server's own read-only / destructive hints"
        example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.set_settings",
    "params": {
      "kind": "skill",
      "name": "installed-tool",
      "patch": {
        "enabled": true
      }
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "schema": {},
      "values": {
        "enabled": true
      }
    }
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "An unknown `security_mode` / `execution_mode`, or a bad `name`"
  - code: -32010
    type: "tool_operation_failed"
    when: "The tool registry could not be saved"
---

# tools.set_settings

Changes the given settings of one installed tool and answers its settings as saved. Only the fields in `patch` change; unknown `security_mode` / `execution_mode` values are refused before anything is saved. Turning an MCP server off (`enabled: false`) also stops it if it runs.

See [`tools.get_settings`](tools.get_settings.md) for what each setting means. Not an MCP tool.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.set_settings","params":{"kind":"mcp","name":"filesystem","patch":{"security_mode":"require_approval"}}}'
```

Leave out the `Authorization` header when API authentication is off.
