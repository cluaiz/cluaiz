---
method: "tools.write_file"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Write SKILL.md or package.json in an installed tool package"
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
  - name: "text"
    type: "string"
    req: true
    desc: "The new content of `SKILL.md` (skill) or `package.json` (plugin, mcp)"
    example: "Updated instructions"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.write_file",
    "params": {
      "kind": "skill",
      "name": "installed-tool",
      "text": "Updated instructions"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "bytes": 20,
      "path": "SKILL.md"
    }
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens"
  - code: -32002
    type: "forbidden"
    when: "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)"
---

# tools.write_file

Replaces a tool's main file: `SKILL.md` for a skill, `package.json` for a plugin or MCP server. No other file can be written this way. Answers `path` and `bytes`. Not an MCP tool.

The new content is used the next time the tool is loaded; a chat that loaded it already keeps the old text until it loads it again.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.write_file","params":{"kind":"skill","name":"my-skill","text":"---\nname: my-skill\ndescription: …\n---\n…"}}'
```

Leave out the `Authorization` header when API authentication is off.
