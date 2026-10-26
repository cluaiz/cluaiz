# AI & Inference Core

<!-- Generated from the CEP registry by api/tests/cep_docs.rs; do not edit. -->

How it works: [`memory.*` overview](./memory/README.md), [`context.*` overview](./context/README.md).

| Method | Scope | Runs as | Summary |
|---|---|---|---|
| [`audio.languages`](./audio/audio.languages.md) | read | sync | Languages declared by the selected transcription model |
| [`audio.run`](./audio/audio.run.md) | read | task | Run the existing speech/transcription executor; chunks are task events |
| [`audio.voices`](./audio/audio.voices.md) | read | sync | Voice inventory and speech capabilities without loading weights |
| [`chat.cancel`](./chat/chat.cancel.md) | write | sync | Stops a running chat stream |
| [`chat.import_app`](./chat/chat.import_app.md) | write | task | Imports the App's old chats (read-only) into the Engine's event logs, as a task |
| [`chat.set_tools`](./chat/chat.set_tools.md) | write | sync | Attaches, updates or detaches a chat's tools |
| [`chat.skip_reasoning`](./chat/chat.skip_reasoning.md) | write | sync | Ends the model's reasoning; the same stream continues with the answer |
| [`chat.telemetry`](./chat/chat.telemetry.md) | read | sync | How a chat's context is used by its tools, for a model |
| [`chat.tools`](./chat/chat.tools.md) | read | sync | The tools attached to a chat, with their turns left |
| [`context.jobs`](./context/context.jobs.md) | read | sync | The idle-time scheduler's queue, counts and recent results |
| [`context.preview`](./context/context.preview.md) | read | sync | The plan and tool choices a chat request would get, without generating (loads the model for its tokenizer) |
| [`context.report`](./context/context.report.md) | read | sync | One of a chat's last 20 context reports (kept in memory) |
| [`context.reports`](./context/context.reports.md) | read | sync | Ids of a chat's kept context reports, newest last |
| [`kv.clear`](./kv/kv.clear.md) | write | sync | Forgets every chat's saved KV |
| [`kv.delete`](./kv/kv.delete.md) | write | sync | Forgets one chat's saved KV |
| [`kv.status`](./kv/kv.status.md) | read | sync | Where saved KV caches are kept, the disk they use, and the saved chats (most recently used first) |
| [`memory.delete`](./memory/memory.delete.md) | write | sync | Deletes a chat's event log, ledger, saved KV, context reports and Chat Map |
| [`memory.events`](./memory/memory.events.md) | read | sync | A chat's event log, oldest first |
| [`memory.export`](./memory/memory.export.md) | read | sync | A chat's events and cards, for an app's own store |
| [`memory.grants`](./memory/memory.grants.md) | read | sync | Tools the user allowed for this chat, with minutes left |
| [`memory.import`](./memory/memory.import.md) | write | sync | Gives back a chat an app keeps itself (storage external), held in RAM only |
| [`memory.index`](./memory/memory.index.md) | read | sync | A chat's cards (one per visible message, no text), episodes, tasks and graph |
| [`memory.item`](./memory/memory.item.md) | read | sync | The exact message behind a reference (m:<position>) |
| [`memory.revoke_grants`](./memory/memory.revoke_grants.md) | write | sync | Asks again for this chat's tools (or one tool) |
| [`memory.search`](./memory/memory.search.md) | read | sync | Messages of a chat that fit a query, or text from the other chats |
| [`notes.confirm`](./notes/notes.confirm.md) | write | sync | A proposed note becomes confirmed (and supersedes a live note on the same subject) |
| [`notes.create`](./notes/notes.create.md) | write | sync | A note written by the user, confirmed at once; it supersedes a live note on the same subject |
| [`notes.invalidate`](./notes/notes.invalidate.md) | write | sync | The note no longer holds (it is kept, with the time) |
| [`notes.list`](./notes/notes.list.md) | read | sync | Notes, newest last |
| [`notes.pin`](./notes/notes.pin.md) | write | sync | Pins or unpins a note; pinned notes go into the system prompt of new chats |

All categories: [CEP reference](../README.md). Folder: `docs/api/cep/ai-inference/`.
