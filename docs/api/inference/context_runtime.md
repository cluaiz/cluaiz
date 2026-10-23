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

## Policy

| Field | Default | Meaning |
| --- | --- | --- |
| `recent_turns_min` | 2 | Newest whole turns always kept (when they fit), and never compacted |
| `evidence_cap_pct` | 15 | Most of the room recalled evidence may use (with recall, P3) |
| `suggestions_cap_pct` | 5 | Most of the room tool suggestions may use |
| `margin_pct` | 2 | Share of the context kept free for template tokens |
| `compact_at_pct` | 85 | The chat is compacted when its history uses this share of the room |

Percentages are whole numbers from 0 to 100; `recent_turns_min` is at most 50.

A chat request can change fields for itself with `context_policy`, for example
`"context_policy": {"recent_turns_min": 3}`. There, unknown fields and invalid values are ignored
with a warning in the Engine log, and the request still runs. The policy used is reported in
`context_report.plan.policy`.

## GET /v1/context/settings

```json
{ "settings": { "recent_turns_min": 2, "evidence_cap_pct": 15, "suggestions_cap_pct": 5, "margin_pct": 2, "compact_at_pct": 85 } }
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
