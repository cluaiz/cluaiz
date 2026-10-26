---
method: "tools.list"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Installed tools with settings and known function schemas"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "kind"
    type: "\"skill\" | \"plugin\" | \"mcp\" | null"
    req: false
    desc: "Only tools of this kind; all when absent"
    example: "mcp"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.list",
    "params": {
      "kind": "mcp"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": [
      {
        "category": "mcp",
        "default_turns": 3,
        "description": "Official Model Context Protocol secure file system bridge",
        "enabled": true,
        "execution_mode": "manual",
        "functions": [
          {
            "annotations": {
              "openWorldHint": false,
              "readOnlyHint": true
            },
            "description": "Read the complete contents of a file from the file system as text.",
            "inputSchema": {
              "properties": {
                "path": {
                  "type": "string"
                }
              },
              "required": [
                "path"
              ],
              "type": "object"
            },
            "name": "read_text_file"
          }
        ],
        "id": "filesystem",
        "security_mode": "inherit"
      }
    ]
  }
errors: []
---

# tools.list

The installed skills, plugins and MCP servers, sorted by kind and name, each with its settings and the functions the model can call.

A tool is named by `kind` and `name`:

| `kind` | What it is | Main file |
|---|---|---|
| `skill` | Instructions the model loads (`SKILL.md`, with optional reference files and scripts) | `SKILL.md` |
| `plugin` | A WASM or native library with functions | `package.json` |
| `mcp` | An MCP server the Engine starts and talks to | `package.json` |

`name` is the tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens, starting with a letter or digit. Tools live in `tools/<kind>s/<name>/` in the Engine home.

## Entry fields

| Field | Meaning |
|---|---|
| `id`, `category` | The tool's name and kind |
| `description` | What it is for (from its manifest) |
| `enabled` | Offered to the model or not ([`tools.set_settings`](tools.set_settings.md)) |
| `security_mode`, `execution_mode`, `trust_annotations` | Its approval settings |
| `default_turns` | How many answers a loaded tool stays loaded without being used |
| `functions[]` | `name`, `description`, `inputSchema` (JSON Schema), and MCP `annotations` (`readOnlyHint`…). Listed from the manifest and the MCP server's cached tool list; listing never starts a server |

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.list","params":{"kind":"mcp"}}'
```

Leave out the `Authorization` header when API authentication is off.
