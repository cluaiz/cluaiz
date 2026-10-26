---
method: "tools.open_file"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Ask the local file editor to open a tool file (owner only)"
scope: "read"
execution: "sync"
mcp: false
owner_only: true
destructive: false
idempotent: true
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
  - name: "path"
    type: "string | null"
    req: false
    desc: "A file inside the tool's folder (default: `SKILL.md` or `package.json`)"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.open_file",
    "params": {
      "kind": "skill",
      "name": "installed-tool"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "opened": true
    }
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "`path` leaves the tool folder or is not a file"
---

# tools.open_file

Opens a tool's file in a text editor on the Engine's machine: Notepad on Windows, TextEdit on macOS, the default application (`xdg-open`) on Linux. Default file: `SKILL.md` or `package.json`.

The editor is a fixed program and the path is passed as one argument, never through a shell. Only the owner may call it (it opens a window on the host); it is never an MCP tool. Answers `{"opened": true}` once the editor was started.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.open_file","params":{"kind":"skill","name":"code-interpreter"}}'
```

Leave out the `Authorization` header when API authentication is off.
