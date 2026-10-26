---
method: "tools.call"
transport: "POST /v1/cep"
category: "agents-tools"
category_title: "Agents, MCP & Tools"
group: "tools"
summary: "Execute an installed tool using the existing ToolsEngine"
scope: "exec"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: false
params:
  - name: "fn"
    type: "string | null"
    req: false
    desc: "The function to run (a plugin export or an MCP tool); the tool's default when absent"
    example: "run"
  - name: "input"
    type: "any"
    req: true
    desc: "The function's arguments, as JSON"
    example: {}
  - name: "kind"
    type: "\"skill\" | \"plugin\" | \"mcp\""
    req: true
    desc: "skill, plugin or mcp"
    example: "plugin"
  - name: "name"
    type: "string"
    req: true
    desc: "The tool's folder name: 1–64 lowercase letters, digits, dots, underscores or hyphens"
    example: "installed-tool"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "tools.call",
    "params": {
      "fn": "run",
      "input": {},
      "kind": "plugin",
      "name": "installed-tool"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "result": "tool output"
    }
  }
errors:
  - code: -32004
    type: "tool_not_found"
    when: "No installed tool of this kind and name"
  - code: -32602
    type: "invalid_params"
    when: "A bad `name`, or `fn` looks like a path"
  - code: -32002
    type: "forbidden"
    when: "The tool folder is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32010
    type: "tool_operation_failed"
    when: "The installer, registry or tool itself failed (its message is passed on)"
---

# tools.call

Runs one function of an installed tool directly, the way the chat agent runs it, and answers its output. Use it to test a tool or to call one from a script.

## What it does

1. Checks `name`, that `fn` is a function name (not a path), and that the tool's folder is inside the caller's roots. A turned-off tool (`enabled: false`) is refused.
2. Runs it, by what the tool is:

| Tool | What runs |
|---|---|
| A plugin or skill whose `package.json` has an `execution` block | Its declared script runner, with `input` as the payload, in the workspace |
| `fn` `run_command`, `bash` or `terminal` | `input.command` in a terminal (`cwd`, `stdin`, `timeout` optional), under the tool's security mode and the agent's hard-deny list |
| An MCP server | Its tool `fn` (default: the tool named like the server), with `input` as arguments; the server is started when it does not run |
| A skill without an `execution` block | Nothing runs: the answer is its `SKILL.md` text |

3. Answers the output: parsed as JSON when it is JSON, else as a string. A tool error is `-32010` `tool_operation_failed` with its message.

Needs `exec` scope. The agent's per-tool approval ([`tools.get_settings`](tools.get_settings.md)) is the chat agent's; a direct call here is the caller's own decision.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools.call","params":{"kind":"mcp","name":"filesystem","fn":"read_text_file","input":{"path":"notes.txt"}}}'
```

Leave out the `Authorization` header when API authentication is off.
