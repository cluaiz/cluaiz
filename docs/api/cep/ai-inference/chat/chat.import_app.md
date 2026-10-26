---
method: "chat.import_app"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "chat"
summary: "Imports the App's old chats (read-only) into the Engine's event logs, as a task"
scope: "write"
execution: "task"
mcp: false
owner_only: false
destructive: false
idempotent: false
params:
  - name: "all"
    type: "boolean"
    req: false
    desc: "Import every App chat that has no Engine event log yet"
    example: true
  - name: "chat_id"
    type: "string | null"
    req: false
    desc: "One App chat id (its folder name); leave out with `all: true` for every chat"
    example: null
  - name: "dry_run"
    type: "boolean"
    req: false
    desc: "Count what would be imported, write nothing"
    example: true
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "chat.import_app",
    "params": {
      "all": true,
      "dry_run": true
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "task": {
        "status": "working",
        "task_id": "t_5f0c…"
      }
    }
  }
errors:
  - code: -32602
    type: "invalid_params"
    when: "Neither `chat_id` nor `all: true` was sent"
---

# chat.import_app

Copies the App's existing chats into the Engine's chat event logs, as a task, so recall, the Chat Map and memory work for chats made before the event log existed. The App's folders are only read, never written.

## What it does

1. Looks for App chats in `%APPDATA%/cluaiz/data/chats/<chat id>/messages.jsonl` (and `~/.cluaiz/data/chats`).
2. With `chat_id`, imports that chat; with `all: true`, every chat.
3. A chat that already has an event log is skipped (`"skipped: the chat already has an event log"`). With `dry_run: true`, nothing is written and each chat is counted.
4. Messages become `message` events (tool calls before the answer they belong to, arguments redacted like the live log). Each chat is a task event; `progress` counts chats.
5. The task's result is `{"roots": [...], "chats": [{chat_id, from, messages, tool_calls, events, status}]}`.

Not an MCP tool.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"chat.import_app","params":{"all":true,"dry_run":true}}'
```

Leave out the `Authorization` header when API authentication is off.
