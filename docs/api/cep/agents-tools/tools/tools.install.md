---
method: "tools.install"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Install a registry tool; task result reports the actual outcome"
scope: "write"
execution: "task"
mcp: true
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
    example: "registry-tool"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.install",
    "params": {
      "kind": "skill",
      "name": "registry-tool"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "task": {
        "status": "working",
        "task_id": "t_example"
      }
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "`name` is not 1–64 lowercase letters, digits, dots, underscores or hyphens"
  - code: -32002
    type: "forbidden"
    when: "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32010
    type: "tool_operation_failed"
    when: "The installer, registry or tool itself failed (its message is passed on)"
  - code: -32008
    type: "cancelled"
    when: "`tasks.cancel` stopped it before or after the download (from the task's result)"
---

# tools.install

Installs a tool from the tools registry (the `cluaiz-tools` catalog), as a task. `cluaiz skill install`, `cluaiz plugin install` and `cluaiz mcp install` run the same installer.

A tool is named by `kind` and `name`:

| `kind` | What it is | Main file |
|---|---|---|
| `skill` | Instructions the model loads (`SKILL.md`, with optional reference files and scripts) | `SKILL.md` |
| `plugin` | A WASM or native library with functions | `package.json` |
| `mcp` | An MCP server the Engine starts and talks to | `package.json` |

`name` is the tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens, starting with a letter or digit. Tools live in `tools/<kind>s/<name>/` in the Engine home.

## What it does

1. Checks `name`, and that the target folder is inside the caller's roots, before anything is downloaded.
2. Answers at once with a task handle.
3. Downloads the tool's package from the registry, extracts it into its folder, and registers it. Events: `{"stage": "installing"}`, then `{"stage": "installed"}`.
4. The task's result is the installed tool's registry entry (as in [`tools.list`](tools.list.md)). A failure (unknown name, download, extraction) is the task's error `-32010` with the installer's message.

[`tasks.cancel`](../../system/tasks/tasks.cancel.md) is checked before and after the download; a download in progress finishes first.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.install","params":{"kind":"skill","name":"code-interpreter"}}'
```

Leave out the `Authorization` header when API authentication is off.
