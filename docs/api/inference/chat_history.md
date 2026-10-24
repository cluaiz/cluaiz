# Chat History and Memory API

The Engine keeps an event log for every chat sent with a `session_id` (not `temporary_chat`), and
builds a Chat Map from it: one card per message, an index, and recall. See
[Chat Completions → Chat event log](./chat_completions.md#-chat-event-log) and
[Recall](./chat_completions.md#-recall).

| Endpoint | What it does |
| --- | --- |
| `GET /v1/chat/{session_id}/events` | The chat's event log, oldest first |
| `GET /v1/chat/{session_id}/index` | The chat's cards (no message text), episodes, tasks and graph |
| `POST /v1/recall/search` | Messages of a chat that fit a query |
| `GET /v1/recall/item/{ref}?chat_id=…` | The exact message behind a reference (`m:<position>`) |
| `GET /v1/chat/{session_id}/export` | Events and cards as one JSON document |
| `DELETE /v1/chat/{session_id}/history` | Removes the chat's log, ledger, saved KV, kept reports and Chat Map |
| `POST /v1/chat/{session_id}/import` | An app that keeps the chat itself sends it back (RAM only) |
| `POST /v1/chat/import/app` | Imports the App's existing chats into event logs (read-only on the App's files) |
| `GET /v1/chat/{session_id}/grants` | Tools allowed for this chat without asking, with minutes left |
| `DELETE /v1/chat/{session_id}/grants` | Asks again for this chat's tools (optional `?tool=`) |

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

A closed episode of at least 4 messages gets a `summary` (`title`, `summary`, `by`) written by
the model at idle time, like the cards; with `history_mode: "none"` the outline of earlier topics
uses it.

`tasks` lists work over several turns. Implicit tasks come from episodes in which tools changed
something (write, edit, create, move, delete, run, execute); their `checkpoint` says how they
started and what was done. The model can keep a task itself with the `task_update` function
(`goal`, `stage`: open, in_progress, blocked, done, abandoned, `checkpoint`), offered while a task
is open; its word wins over the implicit task, also when it says the work is done. While a task is
open and the prompt does not show the whole chat (or was just compacted), a `<task>` block with the
checkpoint goes in front of the current message (`context_report.plan.task`).

`graph` has `nodes` (`m:<pos>` messages, `ep:<pos>` episodes, `tool:`, `file:`, `value:`) and
`edges` (`next`, `part_of`, `uses_tool`, `touches_file`, `mentions`, `corrects`), built by rules,
for apps that keep the Chat Map in their own graph store. Recall uses it to bring along one hop:
a later correction of a recalled message, or a message with the same file or value.

## POST /v1/recall/search

Body: `chat_id`, `query`, optional `limit` (default 5, at most 50). Every message of the chat is a
candidate; time words in the query ("kal", "parso", "Monday", "23 Oct", "kal raat", "3 ghante pehle", "last week") limit the search to that window, in the user's own days.

With `scope: "workspace"` or `"all"` the search runs over the other chats instead (written to in
the last 90 days, at most 50, newer chats first; `workspace` keeps chats used in the current
workspace) and returns `text` with one line per message (`[chat <id> · m:<pos> · time · role]`).
With the policy `cross_chat: "deny"` this returns 403.

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

## Where a chat is kept (`storage`)

A chat request's `storage` decides where the Engine keeps the chat:

| `storage` | Event log, ledger, saved KV | Use |
| --- | --- | --- |
| `engine` (default) | On disk under `<CLUAIZ_HOME>` | The Engine is the memory |
| `external` | In RAM only; nothing of the chat is written to disk | The app keeps the chat (its own files or database) |
| `none` | Nothing (same as `temporary_chat: "lite"`) | Private chats |

With `external`, every answer carries the events it added to the chat: `memory_events` in the
non-streaming response, and in the closing stream chunk (the one with `engine`). The app stores
them. After an Engine restart (RAM is empty) it sends them back with
`POST /v1/chat/{session_id}/import` (`{"events": [...]}`, at most 200,000), and recall, the Chat Map
and the event endpoints work again. No idle-time cards are made for such chats (they would be
written by the Engine), and the chat's KV is not saved to disk.

Measured (2026-10-24, isolated server): two turns with `storage: "external"` left no file of the
chat on disk (no event log, ledger or saved KV); after a restart and an import of the 4 collected
events, `history_mode: "none"` recalled "vega" and port 6060 from them.

## Export format (for apps and their databases)

`GET /v1/chat/{session_id}/export` and `memory_events` use the same event shape, so an app can
keep a chat in any store (files, SQLite, cluaizdb, …) on its side; the Engine itself never links
a database.

```json
{ "format": "cluaiz-chat-export", "version": 1, "chat_id": "my-chat",
  "events": [ { "v": 1, "id": "ev_…", "chat": "my-chat", "seq": 1, "ts": "2026-10-24T05:23:01.000Z",
                "kind": "message", "role": "user", "text": "…", "pos": 0, "key": "h:…", "source": "client" } ],
  "cards": [ { "…": "derived; can always be rebuilt from the events" } ] }
```

| Field | Meaning |
| --- | --- |
| `v` | Event format version (1) |
| `id`, `seq`, `ts` | Unique id, order in the chat (from 1), time (RFC 3339, UTC) |
| `kind` | `message`, `message.edited`, `message.retracted`, `tool_call`, `tool_result`, `approval`, `context_edit`, `card_enrichment`, `card_enrichment_failed`, `episode_summary`, `episode_summary_failed`, `task`, `grant` |
| `pos` | Position of a message in the visible transcript (message kinds) |
| `source` | `client`, `engine` or `app-import` (message kinds) |

Keep every event in order; `cards`, episodes, tasks and the graph are derived and are rebuilt from
the events after an import. Fields an event kind does not know are kept as they are.

## Engine mode for transcripts (App migration notes)

With `history: "engine"` (and `storage: "engine"`) the client sends only the new message and the
Engine keeps the transcript, so the App does not need its own copy:

1. Send each new user message with `session_id` and `history: "engine"`.
2. Show a chat from `GET /v1/chat/{session_id}/events`: fold `message`, `message.edited` (replaces
   the message at the same `pos`) and `message.retracted` (removes it), in `seq` order; tool steps
   are the `tool_call` / `tool_result` events just before an answer.
3. Existing App chats: `POST /v1/chat/import/app` (`all: true`) copies them into Engine logs once
   (the App's files are only read).
4. Deleting a chat: `DELETE /v1/chat/{session_id}/history`.
