---
method: "memory.index"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "memory"
summary: "A chat's cards (one per visible message, no text), episodes, tasks and graph"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "chat_id"
    type: "string"
    req: true
    desc: "The chat: the same value as `session_id` in chat completions"
    example: "c1"
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "memory.index",
    "params": {
      "chat_id": "c1"
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "cards": [
        {
          "pos": 0,
          "role": "user",
          "title": "Greeting"
        }
      ],
      "chat_id": "c1",
      "episodes": [],
      "graph": {},
      "tasks": []
    }
  }
errors:
  - code: -32010
    type: "event_log_unreadable"
    when: "The chat's event log could not be read"
---

# memory.index

A chat's Chat Map: one card per visible message (title, keywords, motive and summary, no message text), the episodes the chat falls into, open tasks, and a graph linking them. Recall searches these cards; a UI can draw a chat's outline from them.

A chat is named by `chat_id`: the same string a client sends as `session_id` in its `/v1/chat/completions` requests. Chats without a `session_id` have no memory.

The Chat Map is built from the event log in the background after each answer; this call answers the newest one.

## Cards

```json
{ "id": "ev_aef3…", "pos": 0, "role": "user", "ts": "…", "tokens": 18, "trivial": false,
  "keywords": ["deploy", "naam", "orion", "port", "server"], "entities": { "files": [], "urls": [], "numbers": ["8011"], "quoted": [] },
  "tools": [], "files": [], "signals": [], "importance": 0.3 }
```

A card the model wrote at idle time carries `enrichment`: `title`, `motive`, `summary`,
`keywords` and `by` (`model:<file>`); see the `indexer` policy in [the context runtime](../context/README.md).

## Episodes

`episodes` lists stretches of the chat about one thing, found by rules: a new episode starts after
a pause of more than 30 minutes, when the user says so ("new topic", "naya topic"), or when a
message shares almost no keywords with the current one (short follow-ups such as "phir se" stay).
Each has `id` (`ep:<first position>`), `first_pos`, `last_pos`, `start`, `end`, `messages`,
`keywords`, a rule `title`, `files`, `tools` and `key_moments` (corrections and decisions).

A closed episode of at least 4 messages gets a `summary` (`title`, `summary`, `by`) written by
the model at idle time, like the cards; with `history_mode: "none"` the outline of earlier topics
uses it.

## Tasks

`tasks` lists work over several turns. Implicit tasks come from episodes in which tools changed
something (write, edit, create, move, delete, run, execute); their `checkpoint` says how they
started and what was done. The model can keep a task itself with the `task_update` function
(`goal`, `stage`: open, in_progress, blocked, done, abandoned, `checkpoint`), offered while a task
is open; its word wins over the implicit task, also when it says the work is done. While a task is
open and the prompt does not show the whole chat (or was just compacted), a `<task>` block with the
checkpoint goes in front of the current message (`context_report.plan.task`).

## Graph

`graph` has `nodes` (`m:<pos>` messages, `ep:<pos>` episodes, `tool:`, `file:`, `value:`) and
`edges` (`next`, `part_of`, `uses_tool`, `touches_file`, `mentions`, `corrects`), built by rules,
for apps that keep the Chat Map in their own graph store. Recall uses it to bring along one hop:
a later correction of a recalled message, or a message with the same file or value.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"memory.index","params":{"chat_id":"c1"}}'
```

Leave out the `Authorization` header when API authentication is off.
