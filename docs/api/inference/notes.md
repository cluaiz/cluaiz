# Notes API

Durable notes: facts and preferences that outlive a chat ("mera deploy server ka port 9090 hai",
"answers in Hinglish"). Stored in `<CLUAIZ_HOME>/memory/notes/notes.jsonl`, one line per change,
so nothing is overwritten. In a chat sent with `storage: "external"` the Engine keeps no note: a
`remember` call becomes a `note` event of the chat in `memory_events`, for the app to keep (see
[Chat History](./chat_history.md#where-a-chat-is-kept-storage)).

| Endpoint | What it does |
| --- | --- |
| `GET /v1/notes` | Notes (filters: `scope`, `status`, `chat_id`) |
| `POST /v1/notes` | A note written by the user (confirmed at once) |
| `POST /v1/notes/{id}/confirm` | A proposed note becomes confirmed |
| `POST /v1/notes/{id}/invalidate` | The note no longer holds (kept, with the time) |
| `POST /v1/notes/{id}/pin` | `{"pinned": true}`: the note goes into the system prompt of new chats |

## A note

```json
{ "id": "n:2", "scope": "workspace", "owner": "D:\\projects\\site", "text": "Deploy server orion ka port ab 9090 hai.",
  "status": "confirmed", "pinned": true, "valid_from": "2026-10-24T05:12:40Z", "invalid_at": null,
  "supersedes": "n:1", "sources": [], "by": "user", "updated_at": "2026-10-24T05:12:40Z" }
```

- `scope`: `chat` (one chat; `owner` is the chat id), `workspace` (the project folder open in
  the Engine; `owner` is its path) or `global` (every chat).
- `status`: `proposed` → `confirmed` (or `active`) → `superseded` or `invalidated`.
- `by`: `user`, `model` (the `remember` function) or `rule`.
- **Corrections:** a new live note of the same scope and owner on the same subject (most of the
  shorter note's words shared) supersedes the older one; both are kept and listed.

## In chats

- **Pinned notes** go into the system prompt. They are chosen when a chat starts and kept with
  the chat, so its system prompt stays the same; a note pinned later travels with the next message
  instead, until the chat starts over.
- **Other live notes** that fit the message (two shared words, or a third of a short note's
  words) travel with it in a `<notes>` block, at most 3.
- **`remember`**: when the user asks to remember something ("yaad rakhna", "remember that",
  "note kar lo"), or with `history_mode` `adaptive` / `none`, the model gets a `remember` function
  (`text`, `scope`: `workspace` or `global`). With the policy `notes: "ask"` (default) the note is
  `proposed` until the user confirms it; with `"auto"` it is confirmed at once; `"off"` removes
  the function. See [Context Runtime](./context_runtime.md#policy).
- `context_report.plan.notes` lists the notes in the system prompt (`system`) and with the message
  (`with_message`).

Measured (Qwen3-4B, isolated server): a pinned workspace note was used in a new chat ("port
8011"); after a correction note the next new chat answered 9090; "Yaad rakhna: mera favourite
colour neela hai" made the model call `remember`, and after confirming, a new chat asked "Mera
favourite colour kaunsa hai?" and got "blue".

## POST /v1/notes

Body: `text` (1–2000 characters), `scope` (default `workspace`), `chat_id` (for `chat`),
`pinned`. Response: `{ "note": {…}, "superseded": {…} | null }`.
