---
method: "fs.pick_folder"
transport: "POST /v1/cep"
category: "filesystem"
category_title: "Filesystem & Security Jail"
group: "fs"
summary: "Opens the host folder picker and selects the workspace"
scope: "read"
execution: "sync"
mcp: false
owner_only: true
destructive: false
idempotent: true
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "fs.pick_folder",
    "params": {}
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
  - code: -32002
    type: "forbidden"
    when: "The caller is not the owner (`data.needs: \"owner\"`): it opens a dialog on the host"
---

# fs.pick_folder

Opens the operating system's folder dialog on the Engine's machine (starting in the user's home folder) and makes the chosen folder the Engine's workspace.

Only the owner may call it, because it shows a window on the host; it is never an MCP tool. The call waits until the user chooses or closes the dialog. Closing it answers `{"path": null}` and changes nothing.

The chosen folder becomes the workspace for CEP and for the agent's tools, as [`workspace.set`](../workspace/workspace.set.md) does.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"fs.pick_folder","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
