---
method: "workspace.set"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "workspace"
summary: "Changes the workspace to an existing directory"
scope: "write"
execution: "sync"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "path"
    type: "string"
    req: true
    desc: "An existing folder: absolute, or relative to the current workspace"
    example: "/project"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "workspace.set",
    "params": {
      "path": "/project"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "path": "/project"
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "The path is not a folder, or is drive-relative / root-relative"
  - code: -32004
    type: "file_not_found"
    when: "The folder does not exist"
  - code: -32002
    type: "forbidden"
    when: "The folder is outside the caller's roots (`data.needs: \"roots\"`)"
---

# workspace.set

Makes an existing folder the Engine's workspace. Relative paths in `fs.*`, chat attachments and the agent's file tools use it from the next call on.

The path is canonicalized and must be an existing folder inside the caller's roots. Not an MCP tool. The workspace is kept in memory: a restart goes back to the Engine's home folder.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"workspace.set","params":{"path":"D:/work/my-project"}}'
```

Leave out the `Authorization` header when API authentication is off.
