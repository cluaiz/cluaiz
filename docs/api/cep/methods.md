# CEP method reference

Generated from `rpc.discover` of Cluaiz Engine 0.1.0 (CEP 1) by `test/cep/export_openrpc.py`. Do not edit by hand. The machine-readable form is [`openrpc.json`](openrpc.json); concepts are in [README.md](README.md).

| Group | Methods |
|---|---|
| `approvals` | [`approvals.approve`](#approvalsapprove), [`approvals.list`](#approvalslist), [`approvals.reject`](#approvalsreject) |
| `audio` | [`audio.languages`](#audiolanguages), [`audio.run`](#audiorun), [`audio.voices`](#audiovoices) |
| `chat` | [`chat.cancel`](#chatcancel), [`chat.import_app`](#chatimport_app), [`chat.set_tools`](#chatset_tools), [`chat.skip_reasoning`](#chatskip_reasoning), [`chat.telemetry`](#chattelemetry), [`chat.tools`](#chattools) |
| `config` | [`config.get`](#configget), [`config.set`](#configset) |
| `context` | [`context.jobs`](#contextjobs), [`context.preview`](#contextpreview), [`context.report`](#contextreport), [`context.reports`](#contextreports) |
| `fs` | [`fs.copy`](#fscopy), [`fs.delete`](#fsdelete), [`fs.list`](#fslist), [`fs.mkdir`](#fsmkdir), [`fs.pick_folder`](#fspick_folder), [`fs.read`](#fsread), [`fs.rename`](#fsrename), [`fs.write`](#fswrite) |
| `ingest` | [`ingest.run`](#ingestrun) |
| `kv` | [`kv.clear`](#kvclear), [`kv.delete`](#kvdelete), [`kv.status`](#kvstatus) |
| `memory` | [`memory.delete`](#memorydelete), [`memory.events`](#memoryevents), [`memory.export`](#memoryexport), [`memory.grants`](#memorygrants), [`memory.import`](#memoryimport), [`memory.index`](#memoryindex), [`memory.item`](#memoryitem), [`memory.revoke_grants`](#memoryrevoke_grants), [`memory.search`](#memorysearch) |
| `models` | [`models.download`](#modelsdownload), [`models.inspect`](#modelsinspect), [`models.list`](#modelslist), [`models.load`](#modelsload), [`models.remove`](#modelsremove) |
| `notes` | [`notes.confirm`](#notesconfirm), [`notes.create`](#notescreate), [`notes.invalidate`](#notesinvalidate), [`notes.list`](#noteslist), [`notes.pin`](#notespin) |
| `rpc` | [`rpc.discover`](#rpcdiscover) |
| `storage` | [`storage.clean`](#storageclean), [`storage.status`](#storagestatus) |
| `system` | [`system.audit`](#systemaudit), [`system.exec`](#systemexec), [`system.info`](#systeminfo), [`system.live`](#systemlive), [`system.plan`](#systemplan), [`system.ps`](#systemps) |
| `tasks` | [`tasks.cancel`](#taskscancel), [`tasks.get`](#tasksget), [`tasks.list`](#taskslist), [`tasks.result`](#tasksresult) |
| `tokens` | [`tokens.create`](#tokenscreate), [`tokens.list`](#tokenslist), [`tokens.revoke`](#tokensrevoke) |
| `tools` | [`tools.cache`](#toolscache), [`tools.call`](#toolscall), [`tools.clear_cache`](#toolsclear_cache), [`tools.discover`](#toolsdiscover), [`tools.files`](#toolsfiles), [`tools.get_settings`](#toolsget_settings), [`tools.install`](#toolsinstall), [`tools.list`](#toolslist), [`tools.open_file`](#toolsopen_file), [`tools.read_file`](#toolsread_file), [`tools.remove`](#toolsremove), [`tools.set_settings`](#toolsset_settings), [`tools.write_file`](#toolswrite_file) |
| `workspace` | [`workspace.get`](#workspaceget), [`workspace.set`](#workspaceset) |

## approvals

### approvals.approve

Allows the waiting call; with remember chat, the tool also runs without asking in that chat for a while

scope `admin` · `sync` · owner_only · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `approval_id` | string | yes |  |
| `feedback` | string \| null | no | Passed to the waiting call |
| `minutes` | integer \| null | no | How long a "chat" grant lasts (default 60, at most 1440) |
| `remember` | string \| null | no | "chat": also allow this tool in this chat without asking again, for `minutes` |

```json
{"jsonrpc": "2.0", "id": 1, "method": "approvals.approve", "params": {"approval_id": "ap_41c2…", "minutes": 60, "remember": "chat"}}
{"jsonrpc": "2.0", "id": 1, "result": {"approval": {"created_at": "2026-10-25T10:30:00Z", "expires_at": "2026-10-25T10:35:00Z", "id": "ap_41c2…", "method": "mcp:web-search", "principal": "agent", "status": "pending", "target": "{\"query\":\"news\"}"}, "grant": {"chat_id": "c1", "minutes": 60, "tool": "web-search"}}}
```

### approvals.list

Approval requests waiting for the owner

scope `admin` · `sync` · owner_only · not on MCP

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "approvals.list", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"pending": [{"created_at": "2026-10-25T10:30:00Z", "expires_at": "2026-10-25T10:35:00Z", "id": "ap_41c2…", "method": "mcp:web-search", "principal": "agent", "status": "pending", "target": "{\"query\":\"news\"}"}]}}
```

### approvals.reject

Refuses the waiting call

scope `admin` · `sync` · owner_only · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `approval_id` | string | yes |  |
| `feedback` | string \| null | no | Passed to the waiting call (the model sees it as the reason) |

```json
{"jsonrpc": "2.0", "id": 1, "method": "approvals.reject", "params": {"approval_id": "ap_41c2…", "feedback": "Not this site"}}
{"jsonrpc": "2.0", "id": 1, "result": {"approval": {"created_at": "2026-10-25T10:30:00Z", "expires_at": "2026-10-25T10:35:00Z", "id": "ap_41c2…", "method": "mcp:web-search", "principal": "agent", "status": "pending", "target": "{\"query\":\"news\"}"}}}
```

## audio

### audio.languages

Languages declared by the selected transcription model

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `model` | string \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "audio.languages", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"data": [], "model": "installed-model", "object": "list"}}
```

### audio.run

Run the existing speech/transcription executor; chunks are task events

scope `read` · `task` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `input_source` | StrictInputSource | yes |  |
| `instruction` | string \| null | no |  |
| `keep_alive` | integer \| null | no |  |
| `model` | string \| null | no |  |
| `parameters` | AudioParameters \| null | no |  |
| `task` | string \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "audio.run", "params": {"input_source": {"data": "Speech input", "type": "text"}}}
{"jsonrpc": "2.0", "id": 1, "result": {"task": {"status": "working", "task_id": "t_example"}}}
```

### audio.voices

Voice inventory and speech capabilities without loading weights

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `model` | string \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "audio.voices", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"data": [], "model": "installed-model", "object": "list"}}
```

## chat

### chat.cancel

Stops a running chat stream

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `reason` | string \| null | no |  |
| `stream_id` | string | yes | The `id` of the running chat stream (from its SSE chunks) |

```json
{"jsonrpc": "2.0", "id": 1, "method": "chat.cancel", "params": {"stream_id": "chatcmpl-1a2b"}}
{"jsonrpc": "2.0", "id": 1, "result": {"status": "cancelled", "stream_id": "chatcmpl-1a2b"}}
```

### chat.import_app

Imports the App's old chats (read-only) into the Engine's event logs, as a task

scope `write` · `task` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `all` | boolean | no |  |
| `chat_id` | string \| null | no | One App chat id (its folder name); leave out with `all: true` for every chat |
| `dry_run` | boolean | no | Count what would be imported, write nothing |

```json
{"jsonrpc": "2.0", "id": 1, "method": "chat.import_app", "params": {"all": true, "dry_run": true}}
{"jsonrpc": "2.0", "id": 1, "result": {"task": {"status": "working", "task_id": "t_5f0c…"}}}
```

### chat.set_tools

Attaches, updates or detaches a chat's tools

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |
| `detach` | string[] | no | Tool ids to detach |
| `tools` | any[] | no | Tools to attach or update: a tool id (its default turns), or `{"id", "turns"}` |

```json
{"jsonrpc": "2.0", "id": 1, "method": "chat.set_tools", "params": {"chat_id": "c1", "detach": ["old-tool"], "tools": ["web-search", {"id": "calc", "turns": 2}]}}
{"jsonrpc": "2.0", "id": 1, "result": {"active_tools": [{"id": "web-search", "turns": 3}], "chat_id": "c1"}}
```

### chat.skip_reasoning

Ends the model's reasoning; the same stream continues with the answer

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `reason` | string \| null | no |  |
| `stream_id` | string | yes | The `id` of the running chat stream (from its SSE chunks) |

```json
{"jsonrpc": "2.0", "id": 1, "method": "chat.skip_reasoning", "params": {"stream_id": "chatcmpl-1a2b"}}
{"jsonrpc": "2.0", "id": 1, "result": {"status": "skip_requested", "stream_id": "chatcmpl-1a2b"}}
```

### chat.telemetry

How a chat's context is used by its tools, for a model

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string \| null | no |  |
| `model` | string \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "chat.telemetry", "params": {"chat_id": "c1", "model": "qwen3-4b-gguf-Q4_K_M"}}
{"jsonrpc": "2.0", "id": 1, "result": {"chat_id": "c1", "model": "qwen3-4b-gguf-Q4_K_M", "telemetry": {}}}
```

### chat.tools

The tools attached to a chat, with their turns left

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "chat.tools", "params": {"chat_id": "c1"}}
{"jsonrpc": "2.0", "id": 1, "result": {"active_tools": [{"id": "web-search", "turns": 3}], "chat_id": "c1"}}
```

## config

### config.get

The saved settings: one section, or all of them

scope `read` · `sync` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `section` | Section \| null | no | One section; all of them when absent |

```json
{"jsonrpc": "2.0", "id": 1, "method": "config.get", "params": {"section": "kv"}}
{"jsonrpc": "2.0", "id": 1, "result": {"kv": {"enabled": true, "max_age_days": 30, "max_disk_gb": 10.0}}}
```

### config.set

Changes the given fields of one settings section; answers with the section as saved

scope `write` · `sync` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `patch` | any | yes | Only the fields to change |
| `section` | Section | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "config.set", "params": {"patch": {"cross_chat": "ask"}, "section": "context"}}
{"jsonrpc": "2.0", "id": 1, "result": {"context": {"cross_chat": "ask"}}}
```

## context

### context.jobs

The idle-time scheduler's queue, counts and recent results

scope `read` · `sync` · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "context.jobs", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"done": [], "queued": 0, "running": null}}
```

### context.preview

The plan and tool choices a chat request would get, without generating (loads the model for its tokenizer)

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |
| `request` | any | yes | A chat completions request (model, messages, tools…); its `session_id` becomes `chat_id` |

```json
{"jsonrpc": "2.0", "id": 1, "method": "context.preview", "params": {"chat_id": "c1", "request": {"messages": [{"content": "Hi", "role": "user"}], "model": "qwen3-4b-gguf-Q4_K_M"}}}
{"jsonrpc": "2.0", "id": 1, "result": {"plan": {"n_ctx": 8192, "prompt_tokens": 412}, "tools": {"cards": []}}}
```

### context.report

One of a chat's last 20 context reports (kept in memory)

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |
| `request_id` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "context.report", "params": {"chat_id": "c1", "request_id": "chatcmpl-1a2b"}}
{"jsonrpc": "2.0", "id": 1, "result": {"chat_id": "c1", "context_report": {"timing": {"preflight_ms": 23}}, "request_id": "chatcmpl-1a2b"}}
```

### context.reports

Ids of a chat's kept context reports, newest last

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "context.reports", "params": {"chat_id": "c1"}}
{"jsonrpc": "2.0", "id": 1, "result": {"chat_id": "c1", "reports": ["chatcmpl-1a2b"]}}
```

## fs

### fs.copy

Copies a file or directory between authorized paths

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `from` | string | yes |  |
| `to` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "fs.copy", "params": {"from": "notes.txt", "to": "copy.txt"}}
{"jsonrpc": "2.0", "id": 1, "result": {"from": "/project/notes.txt", "to": "/project/copy.txt"}}
```

### fs.delete

Deletes a file or directory

scope `write` · `sync` · destructive · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "fs.delete", "params": {"path": "old.txt"}}
{"jsonrpc": "2.0", "id": 1, "result": {"path": "/project/old.txt"}}
```

### fs.list

Lists directory entries, optionally recursively

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `path` | string \| null | no |  |
| `recursive` | boolean | no |  |
| `root` | string \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "fs.list", "params": {"path": ".", "recursive": false}}
{"jsonrpc": "2.0", "id": 1, "result": {"items": [{"is_folder": false, "modified": "2026-10-25T00:00:00.000Z", "name": "notes.txt", "path": "notes.txt", "size": 5}], "skipped": 0, "truncated": false}}
```

### fs.mkdir

Creates a directory and missing parents

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "fs.mkdir", "params": {"path": "documents"}}
{"jsonrpc": "2.0", "id": 1, "result": {"path": "/project/documents"}}
```

### fs.pick_folder

Opens the host folder picker and selects the workspace

scope `read` · `sync` · owner_only · not on MCP

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "fs.pick_folder", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"path": "/project"}}
```

### fs.read

Reads a UTF-8 file, or returns base64 for binary data

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `binary` | boolean | no |  |
| `path` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "fs.read", "params": {"path": "notes.txt"}}
{"jsonrpc": "2.0", "id": 1, "result": {"binary": false, "path": "/project/notes.txt", "size": 5, "text": "hello"}}
```

### fs.rename

Moves a file or directory between authorized paths

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `from` | string | yes |  |
| `to` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "fs.rename", "params": {"from": "old.txt", "to": "new.txt"}}
{"jsonrpc": "2.0", "id": 1, "result": {"from": "/project/old.txt", "to": "/project/new.txt"}}
```

### fs.write

Writes exactly one of text or data_base64 to a file

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `data_base64` | string \| null | no |  |
| `path` | string | yes |  |
| `text` | string \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "fs.write", "params": {"path": "notes.txt", "text": "hello"}}
{"jsonrpc": "2.0", "id": 1, "result": {"bytes": 5, "path": "/project/notes.txt"}}
```

## ingest

### ingest.run

Extracts documents (and optionally runs a model on them), as a task; one event per source

scope `read` · `task` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chunking_strategy` | ChunkingStrategy \| null | no |  |
| `model` | string \| null | no | An installed embedding, vision or chat model; extraction only when absent |
| `output_controls` | OutputControls \| null | no |  |
| `source` | SourceInput | yes | A path (inside the caller's roots), an http(s) URL or a data URI; or a list of them |
| `vision_settings` | VisionSettings \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "ingest.run", "params": {"chunking_strategy": {"max_chunk_size": 2000}, "source": "docs/report.pdf"}}
{"jsonrpc": "2.0", "id": 1, "result": {"task": {"method": "ingest.run", "status": "working", "task_id": "t_5f0c…"}}}
```

## kv

### kv.clear

Forgets every chat's saved KV

scope `write` · `sync` · destructive · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "kv.clear", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"deleted": 3}}
```

### kv.delete

Forgets one chat's saved KV

scope `write` · `sync` · destructive · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "kv.delete", "params": {"chat_id": "c1"}}
{"jsonrpc": "2.0", "id": 1, "result": {"chat_id": "c1", "deleted": true}}
```

### kv.status

Where saved KV caches are kept, the disk they use, and the saved chats (most recently used first)

scope `read` · `sync` · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "kv.status", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"bytes": 52428800, "chats": [], "directory": "C:/Users/me/.cluaiz/kv_cache"}}
```

## memory

### memory.delete

Deletes a chat's event log, ledger, saved KV, context reports and Chat Map

scope `write` · `sync` · destructive · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes | The chat: the same value as `session_id` in chat completions |

```json
{"jsonrpc": "2.0", "id": 1, "method": "memory.delete", "params": {"chat_id": "c1"}}
{"jsonrpc": "2.0", "id": 1, "result": {"chat_id": "c1", "events": {"bytes": 2048, "files": 1}, "folder_removed": true, "ledger": true, "saved_kv": {}}}
```

### memory.events

A chat's event log, oldest first

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |
| `from_seq` | integer \| null | no | First seq to return (default 1) |
| `limit` | integer \| null | no | Most events to return (default 500, at most 5000) |

```json
{"jsonrpc": "2.0", "id": 1, "method": "memory.events", "params": {"chat_id": "c1", "from_seq": 1, "limit": 2}}
{"jsonrpc": "2.0", "id": 1, "result": {"chat_id": "c1", "events": [{"data": {"role": "user", "text": "Hi"}, "kind": "message", "seq": 1}], "next_from_seq": 3, "skipped_lines": 0, "total": 40}}
```

### memory.export

A chat's events and cards, for an app's own store

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes | The chat: the same value as `session_id` in chat completions |

```json
{"jsonrpc": "2.0", "id": 1, "method": "memory.export", "params": {"chat_id": "c1"}}
{"jsonrpc": "2.0", "id": 1, "result": {"cards": [], "chat_id": "c1", "events": [], "format": "cluaiz-chat-export", "version": 1}}
```

### memory.grants

Tools the user allowed for this chat, with minutes left

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes | The chat: the same value as `session_id` in chat completions |

```json
{"jsonrpc": "2.0", "id": 1, "method": "memory.grants", "params": {"chat_id": "c1"}}
{"jsonrpc": "2.0", "id": 1, "result": {"chat_id": "c1", "grants": [{"minutes_left": 42, "tool": "web-search"}]}}
```

### memory.import

Gives back a chat an app keeps itself (storage external), held in RAM only

scope `write` · `sync` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |
| `events` | any[] | yes | The events of `memory.export` (or the `memory_events` an app collected) |

```json
{"jsonrpc": "2.0", "id": 1, "method": "memory.import", "params": {"chat_id": "c1", "events": []}}
{"jsonrpc": "2.0", "id": 1, "result": {"cards": 0, "chat_id": "c1", "events": 0, "storage": "external"}}
```

### memory.index

A chat's cards (one per visible message, no text), episodes, tasks and graph

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes | The chat: the same value as `session_id` in chat completions |

```json
{"jsonrpc": "2.0", "id": 1, "method": "memory.index", "params": {"chat_id": "c1"}}
{"jsonrpc": "2.0", "id": 1, "result": {"cards": [{"pos": 0, "role": "user", "title": "Greeting"}], "chat_id": "c1", "episodes": [], "graph": {}, "tasks": []}}
```

### memory.item

The exact message behind a reference (m:<position>)

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |
| `ref` | string | yes | A reference from a search result, `m:<position>` |

```json
{"jsonrpc": "2.0", "id": 1, "method": "memory.item", "params": {"chat_id": "c1", "ref": "m:12"}}
{"jsonrpc": "2.0", "id": 1, "result": {"card": {}, "chat_id": "c1", "ref": "m:12", "text": "The deploy server listens on port 8443."}}
```

### memory.revoke_grants

Asks again for this chat's tools (or one tool)

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |
| `tool` | string \| null | no | Only this tool; all of the chat's grants when absent |

```json
{"jsonrpc": "2.0", "id": 1, "method": "memory.revoke_grants", "params": {"chat_id": "c1", "tool": "web-search"}}
{"jsonrpc": "2.0", "id": 1, "result": {"chat_id": "c1", "removed": 1}}
```

### memory.search

Messages of a chat that fit a query, or text from the other chats

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string | yes |  |
| `limit` | integer \| null | no | Most results (default 5; at most 50 in a chat, 20 across chats) |
| `query` | string | yes |  |
| `scope` | string \| null | no | `chat` (default), or `workspace` / `all` to search the other chats (not with cross_chat: deny) |

```json
{"jsonrpc": "2.0", "id": 1, "method": "memory.search", "params": {"chat_id": "c1", "query": "deploy port"}}
{"jsonrpc": "2.0", "id": 1, "result": {"chat_id": "c1", "query": "deploy port", "results": [{"card": {}, "ref": "m:12", "score": 3.2, "why": ["port"]}]}}
```

## models

### models.download

Downloads and registers a model (catalog id, or Hugging Face owner/name[:QUANT]) as `cluaiz pull` does; the `:QUANT` variant, else the first

scope `write` · `task` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `model` | string | yes | A catalog id, or a Hugging Face repo `owner/name` (optionally `:QUANT`, an `hf://` or https URL) |

```json
{"jsonrpc": "2.0", "id": 1, "method": "models.download", "params": {"model": "owner/name-GGUF:Q4_K_M"}}
{"jsonrpc": "2.0", "id": 1, "result": {"task": {"method": "models.download", "status": "working", "task_id": "t_5f0c…"}}}
```

### models.inspect

Raw GGUF metadata and tensor table of an installed model

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `file` | string \| null | no |  |
| `model` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "models.inspect", "params": {"model": "installed-model"}}
{"jsonrpc": "2.0", "id": 1, "result": {"metadata": {}, "model": "installed-model", "tensors": []}}
```

### models.list

Installed registry entries, optionally filtered by category

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `category` | Category \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "models.list", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": []}
```

### models.load

Loads an installed GGUF model using the existing Engine host

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `model` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "models.load", "params": {"model": "installed-model"}}
{"jsonrpc": "2.0", "id": 1, "result": {"loaded_ms": 125, "model": "installed-model"}}
```

### models.remove

Unloads the selected model if active and removes its installed package

scope `write` · `sync` · destructive · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `model` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "models.remove", "params": {"model": "installed-model"}}
{"jsonrpc": "2.0", "id": 1, "result": {"dir": "models/chat/installed-model", "model": "installed-model"}}
```

## notes

### notes.confirm

A proposed note becomes confirmed (and supersedes a live note on the same subject)

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "notes.confirm", "params": {"id": "n7"}}
{"jsonrpc": "2.0", "id": 1, "result": {"note": {"id": "n7", "pinned": false, "scope": "workspace", "status": "confirmed", "text": "Prefers tabs over spaces"}, "superseded": []}}
```

### notes.create

A note written by the user, confirmed at once; it supersedes a live note on the same subject

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string \| null | no | The chat, for scope chat |
| `pinned` | boolean | no | Pinned notes go into the system prompt of new chats |
| `scope` | string \| null | no | chat, workspace (default) or global |
| `text` | string | yes | 1 to 2000 characters |

```json
{"jsonrpc": "2.0", "id": 1, "method": "notes.create", "params": {"scope": "workspace", "text": "Prefers tabs over spaces"}}
{"jsonrpc": "2.0", "id": 1, "result": {"note": {"id": "n7", "pinned": false, "scope": "workspace", "status": "confirmed", "text": "Prefers tabs over spaces"}, "superseded": []}}
```

### notes.invalidate

The note no longer holds (it is kept, with the time)

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "notes.invalidate", "params": {"id": "n7"}}
{"jsonrpc": "2.0", "id": 1, "result": {"note": {"id": "n7", "pinned": false, "scope": "workspace", "status": "confirmed", "text": "Prefers tabs over spaces"}, "superseded": []}}
```

### notes.list

Notes, newest last

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `chat_id` | string \| null | no | Only notes that apply to this chat (with the current workspace) |
| `scope` | string \| null | no | chat, workspace or global |
| `status` | string \| null | no | proposed, confirmed, superseded or invalidated |

```json
{"jsonrpc": "2.0", "id": 1, "method": "notes.list", "params": {"status": "confirmed"}}
{"jsonrpc": "2.0", "id": 1, "result": {"notes": [{"id": "n7", "pinned": false, "scope": "workspace", "status": "confirmed", "text": "Prefers tabs over spaces"}], "workspace": "D:/work"}}
```

### notes.pin

Pins or unpins a note; pinned notes go into the system prompt of new chats

scope `write` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `id` | string | yes |  |
| `pinned` | boolean | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "notes.pin", "params": {"id": "n7", "pinned": true}}
{"jsonrpc": "2.0", "id": 1, "result": {"note": {"id": "n7", "pinned": false, "scope": "workspace", "status": "confirmed", "text": "Prefers tabs over spaces"}, "superseded": []}}
```

## rpc

### rpc.discover

The OpenRPC description of the methods you may call

scope `read` · `sync` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `group` | string \| null | no | Only this group, for example "fs" |

```json
{"jsonrpc": "2.0", "id": 1, "method": "rpc.discover", "params": {"group": "tasks"}}
{"jsonrpc": "2.0", "id": 1, "result": {"info": {"title": "Cluaiz Engine Protocol"}, "methods": [], "openrpc": "1.3.2"}}
```

## storage

### storage.clean

Removes cached temporary media

scope `write` · `sync` · destructive · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "storage.clean", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"freed_bytes": 0, "removed_files": 0}}
```

### storage.status

Temporary media storage usage

scope `read` · `sync` · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "storage.status", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"file_count": 0, "total_size_bytes": 0, "total_size_mb": "0.00 MB"}}
```

## system

### system.audit

The newest lines of the CEP audit log, newest first

scope `admin` · `sync` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `limit` | integer \| null | no | How many of the newest lines (default 100, at most 1000) |

```json
{"jsonrpc": "2.0", "id": 1, "method": "system.audit", "params": {"limit": 1}}
{"jsonrpc": "2.0", "id": 1, "result": {"lines": [{"code": null, "method": "fs.write", "ms": 3, "params_sha256": "9f2c…", "principal": "owner", "scope": "write", "ts": "2026-10-25T10:30:00.000Z"}], "path": "C:/Users/me/.cluaiz/cep-audit.jsonl"}}
```

### system.exec

Runs a program on the host and returns its output; the owner approves each call

scope `admin` · `sync` · destructive · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `args` | string[] | no | Its arguments, each passed as it is |
| `command` | string \| null | no | One shell line instead of `program`; only when permission.json `exec.allow_shell` is on |
| `program` | string \| null | no | The program to run (a name on PATH or a path); not passed through a shell |

```json
{"jsonrpc": "2.0", "id": 1, "method": "system.exec", "params": {"args": ["--version"], "program": "git"}}
{"jsonrpc": "2.0", "id": 1, "result": {"exit_code": 0, "stderr": "", "stdout": "git version 2.47.0\n"}}
```

### system.info

Engine name and version, CEP version, hardware, and the LAN address

scope `read` · `sync` · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "system.info", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"cep_version": 1, "engine": "Cluaiz Engine", "hardware": {"cpu": {}, "gpus": [], "memory": {}}, "network": {"lan_ip": "192.168.1.20"}, "version": "0.1.0"}}
```

### system.live

Current process, memory, CPU and GPU telemetry

scope `read` · `sync` · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "system.live", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"cpu": {}, "gpus": [], "memory": {"available_gb": 8, "total_gb": 16, "used_gb": 8}, "process_id": 1}}
```

### system.plan

Plans an installed model without loading its weights or changing saved settings

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `batch_tokens` | integer \| null | no |  |
| `model` | string | yes |  |
| `model_path` | string \| null | no |  |
| `n_ctx` | integer \| null | no |  |
| `ram_buffer_gb` | number \| null | no |  |
| `vram_buffer_gb` | number \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "system.plan", "params": {"model": "installed-model"}}
{"jsonrpc": "2.0", "id": 1, "result": {"execution_plan": {}, "hardware_info": {}, "model_info": {}}}
```

### system.ps

Loaded model execution plan and hardware snapshot

scope `read` · `sync` · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "system.ps", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"active_processes": [], "hardware_snapshot": {}}}
```

## tasks

### tasks.cancel

Cancels a running task

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `task_id` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tasks.cancel", "params": {"task_id": "t_5f0c…"}}
{"jsonrpc": "2.0", "id": 1, "result": {"status": "cancelled", "status_message": "Cancelled by request"}}
```

### tasks.get

A task's status, progress and new events

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `after` | integer \| null | no | Events after this cursor (`next` of the last answer); 0 or absent = all kept events |
| `task_id` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tasks.get", "params": {"after": 0, "task_id": "t_5f0c…"}}
{"jsonrpc": "2.0", "id": 1, "result": {"events": [], "next": 0, "progress": {"done": 1, "total": 3}, "status": "working"}}
```

### tasks.list

Your tasks that have not expired, newest first

scope `read` · `sync` · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "tasks.list", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"tasks": [{"created_at": "2026-10-25T10:30:00Z", "last_updated_at": "2026-10-25T10:30:02Z", "method": "ingest.run", "poll_ms": 500, "status": "working", "task_id": "t_5f0c…", "ttl_ms": 3600000}]}}
```

### tasks.result

A task's final result or error; waits up to wait_ms

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `task_id` | string | yes |  |
| `wait_ms` | integer \| null | no | How long to wait for the end, at most 30000 ms; 0 or absent = do not wait |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tasks.result", "params": {"task_id": "t_5f0c…", "wait_ms": 30000}}
{"jsonrpc": "2.0", "id": 1, "result": {"any": "the result the method returns"}}
```

## tokens

### tokens.create

Makes a new API key (at most 5)

scope `admin` · `sync` · not on MCP

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "tokens.create", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"api_auth_required": false, "token": "sk-cluaiz-1f0c…", "tokens": ["sk-cluaiz-1f0c…"]}}
```

### tokens.list

The API keys, and whether API authentication is required

scope `admin` · `sync` · not on MCP

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "tokens.list", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"api_auth_required": false, "tokens": ["sk-cluaiz-1f0c…"]}}
```

### tokens.revoke

Removes an API key; with none left, API authentication turns off (as before)

scope `admin` · `sync` · destructive · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `index` | integer \| null | no | Or its position in `tokens.list`, from 1 |
| `token` | string \| null | no | The key itself, or its masked form (at least 4 visible trailing characters) |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tokens.revoke", "params": {"index": 1}}
{"jsonrpc": "2.0", "id": 1, "result": {"api_auth_required": false, "revoked_token": "sk-cluaiz-1f0c…", "tokens": []}}
```

## tools

### tools.cache

Actual cache entries and byte counts for installed tools

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.cache", "params": {"kind": "skill"}}
{"jsonrpc": "2.0", "id": 1, "result": []}
```

### tools.call

Execute an installed tool using the existing ToolsEngine

scope `exec` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `fn` | string \| null | no |  |
| `input` | any | yes |  |
| `kind` | Kind | yes |  |
| `name` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.call", "params": {"fn": "run", "input": {}, "kind": "plugin", "name": "installed-tool"}}
{"jsonrpc": "2.0", "id": 1, "result": {"result": "tool output"}}
```

### tools.clear_cache

Remove cached files without uninstalling tools

scope `write` · `sync` · destructive · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.clear_cache", "params": {"kind": "skill"}}
{"jsonrpc": "2.0", "id": 1, "result": {"freed_bytes": 0, "removed": 0}}
```

### tools.discover

Dry-run the existing installed-tool matcher

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `explain` | boolean | no |  |
| `previous` | string \| null | no |  |
| `query` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.discover", "params": {"query": "Find a tool for this task"}}
{"jsonrpc": "2.0", "id": 1, "result": {"candidates": [], "installed": 0}}
```

### tools.files

Files inside a tool package, with cache counts

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.files", "params": {"kind": "skill", "name": "installed-tool"}}
{"jsonrpc": "2.0", "id": 1, "result": {"cache": {}, "files": [], "skipped": 0, "truncated": false}}
```

### tools.get_settings

Read installed tool settings

scope `read` · `sync` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.get_settings", "params": {"kind": "skill", "name": "installed-tool"}}
{"jsonrpc": "2.0", "id": 1, "result": {"schema": {}, "values": {"enabled": true}}}
```

### tools.install

Install a registry tool; task result reports the actual outcome

scope `write` · `task` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.install", "params": {"kind": "skill", "name": "registry-tool"}}
{"jsonrpc": "2.0", "id": 1, "result": {"task": {"status": "working", "task_id": "t_example"}}}
```

### tools.list

Installed tools with settings and known function schemas

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.list", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": []}
```

### tools.open_file

Ask the local file editor to open a tool file (owner only)

scope `read` · `sync` · owner_only · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string | yes |  |
| `path` | string \| null | no |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.open_file", "params": {"kind": "skill", "name": "installed-tool"}}
{"jsonrpc": "2.0", "id": 1, "result": {"opened": true}}
```

### tools.read_file

Read a UTF-8 file inside an installed tool package

scope `read` · `sync` · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string | yes |  |
| `path` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.read_file", "params": {"kind": "skill", "name": "installed-tool", "path": "SKILL.md"}}
{"jsonrpc": "2.0", "id": 1, "result": {"path": "SKILL.md", "text": "Tool instructions"}}
```

### tools.remove

Remove an installed tool and update its registry

scope `write` · `sync` · destructive · MCP tool

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.remove", "params": {"kind": "skill", "name": "installed-tool"}}
{"jsonrpc": "2.0", "id": 1, "result": {"kind": "skill", "name": "installed-tool"}}
```

### tools.set_settings

Validate and save a tool settings patch

scope `write` · `sync` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string | yes |  |
| `patch` | Patch | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.set_settings", "params": {"kind": "skill", "name": "installed-tool", "patch": {"enabled": true}}}
{"jsonrpc": "2.0", "id": 1, "result": {"schema": {}, "values": {"enabled": true}}}
```

### tools.write_file

Write SKILL.md or package.json in an installed tool package

scope `write` · `sync` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `kind` | Kind | yes |  |
| `name` | string | yes |  |
| `text` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "tools.write_file", "params": {"kind": "skill", "name": "installed-tool", "text": "Updated instructions"}}
{"jsonrpc": "2.0", "id": 1, "result": {"bytes": 20, "path": "SKILL.md"}}
```

## workspace

### workspace.get

Returns the current workspace directory

scope `read` · `sync` · MCP tool

No params.

```json
{"jsonrpc": "2.0", "id": 1, "method": "workspace.get", "params": {}}
{"jsonrpc": "2.0", "id": 1, "result": {"path": "/project"}}
```

### workspace.set

Changes the workspace to an existing directory

scope `write` · `sync` · not on MCP

| Param | Type | Required | Description |
|---|---|---|---|
| `path` | string | yes |  |

```json
{"jsonrpc": "2.0", "id": 1, "method": "workspace.set", "params": {"path": "/project"}}
{"jsonrpc": "2.0", "id": 1, "result": {"path": "/project"}}
```
