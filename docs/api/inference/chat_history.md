# Chat History and Memory API

The Engine keeps an event log for every chat sent with a `session_id` (not `temporary_chat`), and
builds a Chat Map from it: one card per message, an index, and recall. See
[Chat Completions → Chat event log](./chat_completions.md#-chat-event-log) and
[Recall](./chat_completions.md#-recall).

| Endpoint | What it does |
| --- | --- |
| `GET /v1/chat/{session_id}/events` | The chat's event log, oldest first |
| `GET /v1/chat/{session_id}/index` | The chat's cards (no message text), episodes and graph |
| `POST /v1/recall/search` | Messages of a chat that fit a query |
| `GET /v1/recall/item/{ref}?chat_id=…` | The exact message behind a reference (`m:<position>`) |
| `GET /v1/chat/{session_id}/export` | Events and cards as one JSON document |
| `DELETE /v1/chat/{session_id}/history` | Removes the chat's log, ledger, saved KV, kept reports and Chat Map |
| `POST /v1/chat/import/app` | Imports the App's existing chats into event logs (read-only on the App's files) |

## GET /v1/chat/{session_id}/events

Query: `from_seq` (default 1), `limit` (default 500, at most 5000). `next_from_seq` is set when
more events follow; `skipped_lines` counts lines that could not be read (a crash during a write).

```json
{ "chat_id": "mem1", "total": 20, "skipped_lines": 0, "next_from_seq": 4,
  "events": [ { "v": 1, "id": "ev_aef3…", "chat": "mem1", "seq": 1, "ts": "2026-10-24T09:00:00.000Z",
                "kind": "message", "role": "user", "text": "Mera deploy server ka port 8011 hai…", "pos": 0, "key": "h:…", "source": "client" } ] }
```

## GET /v1/chat/{session_id}/index

```json
{ "chat_id": "mem1", "cards": [ { "id": "ev_aef3…", "pos": 0, "role": "user", "ts": "…", "tokens": 18, "trivial": false,
  "keywords": ["deploy", "naam", "orion", "port", "server"], "entities": { "files": [], "urls": [], "numbers": ["8011"], "quoted": [] },
  "tools": [], "files": [], "signals": [], "importance": 0.3 } ] }
```

A card the model wrote at idle time carries `enrichment`: `title`, `motive`, `summary`,
`keywords` and `by` (`model:<file>`); see the `indexer` policy in [Context Runtime](./context_runtime.md).

`episodes` lists stretches of the chat about one thing, found by rules: a new episode starts after
a pause of more than 30 minutes, when the user says so ("new topic", "naya topic"), or when a
message shares almost no keywords with the current one (short follow-ups such as "phir se" stay).
Each has `id` (`ep:<first position>`), `first_pos`, `last_pos`, `start`, `end`, `messages`,
`keywords`, a rule `title`, `files`, `tools` and `key_moments` (corrections and decisions).

`graph` has `nodes` (`m:<pos>` messages, `ep:<pos>` episodes, `tool:`, `file:`, `value:`) and
`edges` (`next`, `part_of`, `uses_tool`, `touches_file`, `mentions`, `corrects`), built by rules,
for apps that keep the Chat Map in their own graph store. Recall uses it to bring along one hop:
a later correction of a recalled message, or a message with the same file or value.

## POST /v1/recall/search

Body: `chat_id`, `query`, optional `limit` (default 5, at most 50). Every message of the chat is a
candidate; time words in the query ("kal", "last week") limit the search to that window.

```json
{ "chat_id": "mem1", "query": "deploy server port",
  "results": [ { "ref": "m:0", "score": 2.84, "why": ["deploy", "server", "port"], "card": { "…": "…" } } ] }
```

## GET /v1/recall/item/{ref}?chat_id=…

Returns the card and the exact text. A malformed reference gives 400; no visible message at
that position gives 404.

## GET /v1/chat/{session_id}/export

`{ "format": "cluaiz-chat-export", "version": 1, "chat_id": "…", "events": [ … ], "cards": [ … ] }`,
for an app that keeps chats in its own store.

## DELETE /v1/chat/{session_id}/history

Removes the event log (all segments), the ledger, the saved KV (chat memory), the kept context
reports and the Chat Map in RAM, then the chat's folder when it is empty. Blobs are shared by
content and stay.

```json
{ "chat_id": "mem1", "events": { "files": 1, "bytes": 6811 }, "ledger": true, "saved_kv": true, "folder_removed": true }
```

## POST /v1/chat/import/app

Imports chats the App saved before the event log existed, so recall works for them too. Body:
`chat_id` (one App chat, its folder name) or `all: true`, and optional `dry_run: true` (count
only). The App's files are only read: `%APPDATA%\cluaiz\data\chats` and
`<home>/.cluaiz/data/chats`, `<chat>/messages.jsonl` (with `toolCalls` and their results). Each
message keeps its original time. A chat that already has an event log is skipped.

```json
{ "roots": ["C:\\Users\\me\\.cluaiz\\data\\chats"],
  "chats": [ { "chat_id": "chat_20261020_195120_uz2o", "messages": 4, "tool_calls": 1, "events": 6, "status": "imported" } ] }
```
