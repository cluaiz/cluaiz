# Context Runtime API

How the Engine shares a model's context between the system prompt, the conversation, tools and
the answer, and what it did for each request. The plan itself is described in
[Chat Completions → Context plan and compaction](./chat_completions.md#-context-plan-and-compaction).

| Endpoint | What it does |
| --- | --- |
| `GET /v1/context/settings` | The planner's default policy |
| `POST /v1/context/settings` | Changes fields of the default policy |
| `POST /v1/chat/{session_id}/context/preview` | The plan a chat request would get, without running it |
| `GET /v1/chat/{session_id}/context/report` | Ids of the chat's kept context reports |
| `GET /v1/chat/{session_id}/context/report/{request_id}` | One kept context report |
| `GET /v1/context/jobs` | The idle-time scheduler: queue, counts and recent results |

## Policy

| Field | Default | Meaning |
| --- | --- | --- |
| `recent_turns_min` | 2 | Newest whole turns always kept (when they fit), and never compacted |
| `evidence_cap_pct` | 15 | Most of the room recalled evidence may use (with recall, P3) |
| `suggestions_cap_pct` | 5 | Most of the room tool suggestions may use |
| `margin_pct` | 2 | Share of the context kept free for template tokens |
| `compact_at_pct` | 85 | The chat is compacted when its history uses this share of the room |
| `compact_to_pct` | 60 | A compaction frees enough to bring the history down to this share, so the next one is many turns away |
| `indexer` | `"auto"` | Idle-time cards written by the model (title, motive, summary, keywords): `auto` when the model has GPU layers, `always`, `rules_only` (cards by rules only) or `off` |
| `notes` | `"ask"` | Notes the model proposes with `remember`: `ask` (proposed until the user confirms), `auto` (confirmed at once), `off` (no `remember`). See [Notes](./notes.md) |
| `tool_format` | `"inline"` | How tools reach the model: `inline` (their schemas in the system prompt, calls as `<tool_call>` blocks) or `native` (the model's own chat template and call format, with llama.cpp's grammar and parser). See [Native tool calls](#native-tool-calls) |
| `cross_chat` | `"ask"` | Searching other chats with `recall` (scope `workspace` or `all`): `ask` (the user approves each search, like a tool call: `permission_request` with capability `memory:other_chats`), `allow`, or `deny` (nothing from other chats is ever returned) |

Percentages are whole numbers from 0 to 100; `recent_turns_min` is at most 50.

**Idle-time cards.** After an answer in a chat with `session_id`, when no chat request is in
flight, the model that answered writes a short card for the turn (at most 80 tokens, thinking
off), reusing the chat's KV prefix. A request that arrives stops the job at once; a job that has
not started yet is dropped without touching the KV. It never loads a model. Cards appear in
`GET /v1/chat/{id}/index` (`enrichment`), their titles and keywords are searched by recall, and
recalled lines show the title. Measured on this machine (Qwen3-4B): about 3-4 s per card; the next
request kept the whole conversation in the cache (`cache_n` 71 of 90, the rest being its own new
message).

**Nothing is lost on a restart or a crash.** The chat's event log is the job list: a card is
written only when it is done (`card_enrichment`, or `card_enrichment_failed` when the reply held
no card, so it is not retried forever), and turns without one are found again. After 60 s
without chat requests, the scheduler scans chats written to in the last 7 days and queues up to
20 such turns as "cold" jobs: only that turn's question and answer are sent. Cold jobs run only
when chat memory is on, because they replace the KV of the chat you were in (it comes back from
disk on your next message). A turn whose job was stopped or replaced by a newer turn is picked up
the same way.

## Native tool calls

With `tool_format: "native"` the bound tools go to the model's chat template as a `tools` list
(llama-server's path): the template writes them, and the earlier calls and results, the way the
model was trained on, and llama.cpp gives a lazy grammar for the model's call format and the parser
that reads its answer. The grammar sleeps while the model thinks and wakes up when thinking ends.
The answer streams as the parser reads it, without the call markup; each call then arrives as a
`<tool_call>` block, so clients, the chat's ledger and `tool_calls` events stay the same in both
modes. A template that does not render tools gets them inline. Media prompts stay inline.

Measured with `test/context-runtime/tool_calls.py` (14 rows: 11 that need a tool, 3 that need
none; isolated server, temperature 0, 2026-10-24, RTX 3050 4 GB):

| Model | `inline` | `native` |
| --- | --- | --- |
| Gemma 4 E4B (Q4_K_M) | 6/14 (tool called right 11/11, right answer 6/14) | 14/14 |
| Qwen3-4B (Q4_K_M) | 13/14 | 14/14 |
| Llama 3.2 3B (Q4_K_M) | 4/14 | 4/14 (its template has no tools: inline) |

Gemma 4 called the tools in both modes, but inline it often wrote no answer after the result or
answered from an invented one. Qwen3's own format is the `<tool_call>` block, so both modes are
close for it.

## GET /v1/context/jobs

```json
{ "indexer": "auto", "requests_in_flight": 0, "running": false,
  "queue": [ { "chat_id": "resume1", "answer_pos": 3, "kind": "cold" } ],
  "counts": { "done": 12, "failed": 0, "stopped": 0, "skipped": 0 },
  "recent": [ { "chat": "resume2", "answer_pos": 5, "cold": false, "outcome": "done", "ms": 8017 } ] }
```

`outcome` is `done`, `stopped` (a request arrived), `failed: …` or `skipped: …` (policy, no GPU
layers, chat memory off, no model or another model loaded).

A chat request can change fields for itself with `context_policy`, for example
`"context_policy": {"recent_turns_min": 3}`. There, unknown fields and invalid values are ignored
with a warning in the Engine log, and the request still runs. The policy used is reported in
`context_report.plan.policy`.

## GET /v1/context/settings

```json
{ "settings": { "recent_turns_min": 2, "evidence_cap_pct": 15, "suggestions_cap_pct": 5, "margin_pct": 2, "compact_at_pct": 85, "compact_to_pct": 60, "indexer": "auto", "notes": "ask", "cross_chat": "ask", "tool_format": "inline" } }
```

## POST /v1/context/settings

Send only the fields to change. An unknown field or an invalid value is refused with HTTP 400.
Saved in `settings.json` (`context_runtime`); the first time the section is written, the file is
copied to `settings.json.bak-<date>-<time>` next to it.

```bash
curl -X POST http://localhost:8000/v1/context/settings \
  -H 'Authorization: Bearer <token>' -H 'Content-Type: application/json' \
  -d '{"compact_at_pct": 80}'
```

## POST /v1/chat/{session_id}/context/preview

The body is a chat request (`messages`, `model`, `tools`, `context_policy`, `history_mode`, …).
The response is the `plan` and `tools` parts of the context report the same request would get.
Nothing is generated, a compaction is reported but not saved, and the chat's tools do not change.
The model is loaded first if it is not, because the counts use its tokenizer.

```json
{
  "plan": { "history_mode": "recent", "n_ctx": 6144, "available_tokens": 5970, "used_tokens": 0, "needs_compaction": false, "policy": { "recent_turns_min": 3, "…": "…" }, "…": "…" },
  "tools": { "suggested": [], "meta_tools": false, "…": "…" }
}
```

## GET /v1/chat/{session_id}/context/report[/{request_id}]

The Engine keeps the context report (`tools` and `plan`) of the newest 20 requests of each chat,
in memory (gone after a restart). Without `request_id` the response lists their ids, newest last;
with it, the report. An unknown id gives HTTP 404.

```json
{ "chat_id": "p26", "request_id": "chatcmpl-2605…", "context_report": { "tools": { "…": "…" }, "plan": { "…": "…" } } }
```
