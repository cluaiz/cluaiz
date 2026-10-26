---
method: "tools.read_file"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Read a UTF-8 file inside an installed tool package"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
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
    type: "string"
    req: true
    desc: "A file inside the tool's folder, relative to it"
    example: "SKILL.md"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.read_file",
    "params": {
      "kind": "skill",
      "name": "installed-tool",
      "path": "SKILL.md"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "path": "SKILL.md",
      "text": "Tool instructions"
    }
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "`path` leaves the tool folder"
  - code: -32004
    type: "file_not_found"
    when: "No such file in the tool folder"
---

# tools.read_file

Reads a UTF-8 text file inside a tool's folder, such as a skill's reference file. `path` is relative to the tool's folder and may not leave it, even for the owner. A file that is not UTF-8 fails the call.

Answers `path` (canonical) and `text`.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.read_file","params":{"kind":"skill","name":"code-interpreter","path":"SKILL.md"}}'
```

Leave out the `Authorization` header when API authentication is off.
