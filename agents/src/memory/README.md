<!-- name: agents/src/memory
description: Chat memory on disk without a database: event logs, durable notes, embeddings of notes and facts, execution records with secrets removed. -->

# Component: `agents/src/memory/`

## Technical Specification

- **Purpose:** Chat memory on disk, with no database: each chat's **event log** (what was said
  and done, in order, append-only JSON lines), from which the Chat Map (cards, index, recall) is
  built; **durable notes** (facts and preferences that outlive a chat); **embeddings** of notes
  and project facts made once and kept; and **execution records** with secrets taken out. The API
  chat (`api/src/handlers/chat/`), the CEP `memory.*` and `notes.*` methods and the terminal agent
  read and write through this folder.
- **Platform Support:** Windows, Linux, macOS (plain files, `zstd` for closed segments).
- **Reusability Level:** Internal library of `cluaiz-agents`, public to the API and the CLI.

## Architectural Flow

![A chat request is compared with the log (new, edited, retracted), secrets are redacted, and events are appended to events.jsonl; large payloads go to blobs; notes and vectors are separate files](../../../docs/assets/engine/memory-store.png)


```mermaid
flowchart TD
    REQ["Chat request<br/>(client messages)"]
    TR["transcript.rs<br/>diff: new / edited / retracted"]
    EXT["external.rs<br/>storage: external → RAM only"]
    STORE["store.rs · EventStore<br/>JsonlStore"]
    DISK["CLUAIZ_HOME/memory/chats/&lt;chat&gt;/<br/>events.jsonl · events-NNNN.jsonl.zst · ledger.json"]
    BLOBS["memory/blobs/&lt;sha256&gt;<br/>payloads over 8 KB"]
    REC["records.rs<br/>redact secrets"]
    NOTES["notes.rs · NoteStore<br/>memory/notes/notes.jsonl"]
    VEC["vectors.rs · VectorStore<br/>memory/vectors/&lt;model&gt;.json"]
    MAP["Chat Map, recall<br/>api/src/handlers/chat/recall.rs"]

    REQ --> TR --> STORE
    REQ -->|"storage: external"| EXT
    EXT -.->|"instead of disk"| STORE
    REC --> STORE
    STORE --> DISK
    STORE --> BLOBS
    STORE --> MAP
    NOTES --> VEC
```

## API Contract (Interface)

- **`mod.rs`:** `memory_root()` (`<CLUAIZ_HOME>/memory`), `chat_stem(chat_id)` (safe folder
  name of a chat), re-exports.
- **`events.rs`:** `Event` (`v`, `id`, `chat`, `seq`, `ts`, `kind` + the kind's fields),
  `NewEvent`, `EVENT_VERSION` (1). Kinds: `message`, `message.edited`, `message.retracted`,
  `tool_call`, `tool_result`, `approval`, `context_edit` (load, unload, compaction).
- **`store.rs`:** trait `EventStore` (the backend contract), `JsonlStore` (built-in), `ReadLog`,
  `DeleteReport`, `SEGMENT_BYTES` (8 MiB), `BLOB_BYTES` (8 KiB).
- **`external.rs`:** `MemStore`, `register_external`, `forget_external`, `is_external`,
  `mem_store`, `store_for(chat)` (the store a chat uses).
- **`transcript.rs`:** `ClientMessage`, `Logged`, `message_key`, `transcript`, `diff`,
  `Change`, `changes`, `change_events`.
- **`notes.rs`:** `Note`, `Extra`, `NoteStore` (`replace`, …), `KINDS` (`preference`, `fact`,
  `person`, `instruction`, `other`), `FACT_PREFIX`, `normalize_id`, `word_overlap`,
  `same_subject`, `similar`, `SIMILAR_WORDS` (1/3).
- **`vectors.rs`:** `VectorStore`, `model_key`, `text_hash`, `cosine`, `forget_model`.
- **`records.rs`:** `REDACTED`, `call_json`, `files_touched`, `redact`, `redact_call`.
- **Export Type:** Public Rust API of `cluaiz-agents`.
- **Dependencies:** `serde_json`, `zstd`, `sha2`, `chrono`, `cluaiz_core` (home folder).

## Deep File Breakdown

- `store.rs`:
  - **Logic:** `JsonlStore` writes one JSON line per event to the chat's current segment; at
    8 MiB the segment is closed and compressed with zstd (`events-0001.jsonl.zst`, …). A payload
    over 8 KiB (a long tool output) goes to a content-addressed blob and the event keeps its
    hash.
  - **Flow:** Every append is written and flushed when it happens; `sync`, at the end of a turn,
    asks the OS to put it on disk.
  - **Why:** Append-only plain files survive crashes and are readable by any tool; a line cut
    short by a crash is skipped and reported, the rest of the chat stays readable.
- `events.rs`: the event shapes and the version number (bump it when a kind changes shape).
- `transcript.rs`:
  - **Logic:** The visible transcript from the events, and the events a request adds. Client
    messages are matched by position and key (the client's message `id`, else a hash of role and
    text): new → `message`, changed → `message.edited` (supersedes the old), no longer sent →
    `message.retracted`.
  - **Why:** Clients resend the whole conversation; nothing is deleted, so edits are history.
- `external.rs`:
  - **Logic:** `storage: "external"`: the App keeps the chat, the Engine keeps it in RAM only
    (no event log, ledger or saved KV on disk). The App gets the events with each answer and
    sends the chat back with `POST /v1/chat/{id}/import` after a restart.
- `notes.rs`:
  - **Logic:** `<CLUAIZ_HOME>/memory/notes/notes.jsonl` holds the current notes only; a change
    rewrites the file. A note is `proposed` (the model's `remember`, waiting for the user),
    `confirmed` or `active`; scope is one chat, a workspace, or every chat.
  - **Flow:** A newer note replaces an older one only when the model or the user names the older
    one (`NoteStore::replace`); `similar` finds notes that may be about the same subject (word
    overlap ≥ 1/3) so they can be shown first. With `history_days`, replaced or forgotten notes
    stay as `superseded` / `invalidated` for that many days, never shown to the model.
  - **Why:** Nothing is replaced by guessing.
- `vectors.rs`:
  - **Logic:** One file per embedding model (`memory/vectors/<model>.json`), read into RAM on
    first use; a vector is keyed by note id (or `fact:<path>`) and the hash of the text it was
    made from. A note is embedded when new or changed, never again per message.
  - **Flow:** `model_key` includes the model file's size, so a new file of the same model gets
    its own vectors; `forget_model` deletes them when the model is removed.
- `records.rs`:
  - **Logic:** What a tool call touched (`files_touched`) and the call with secrets replaced by
    `[redacted]` before it is logged.

## Failure & Recovery Logic

- **Crash mid-write:** the cut line is skipped and reported on the next read
  (`ReadLog`); later events are kept.
- **Disk full / write error:** the append fails and the error goes up to the caller; the chat
  answer is not lost, only its log line.
- **Corrupt notes file:** the notes that parse are read; a change rewrites the file whole.
- **Embedding model removed or changed:** its vector file is dropped; notes fall back to word
  matching (`api/src/handlers/chat/semantic.rs`).
- **Temporary chats (`temporary_chat`):** nothing is logged.
- **Deleting a chat (`memory.delete`):** its log, segments, ledger and everything derived from it
  go; `DeleteReport` says what was removed.

## Decisions and why

| Decision | Why | Alternative not taken |
|---|---|---|
| Append-only JSON-lines files | they survive crashes and any tool can read them; a line cut by a crash is skipped and reported (test below) | a database |
| Edits and deletions are new events, nothing is removed | clients resend the whole conversation; history keeps what changed | rewrite the log |
| Notes proposed by the model wait for the user | nothing is stored about the user by guessing (`test_proposals_wait_for_the_user`) | store at once |
| Secrets are redacted before an event is written | the log must not keep keys (`test_secrets_are_redacted`) | redact on read |

## Tested

21 unit tests, among them `test_jsonl_store_conformance`, `test_mem_store_conformance`,
`test_a_line_cut_by_a_crash_is_skipped_and_reported`, `test_full_segments_are_compressed_and_still_read`,
`test_an_old_log_file_reads_and_is_cleaned_on_the_next_write`, `test_new_edited_and_retracted_messages`,
`test_changes_from_events_match_diff`, `test_proposals_wait_for_the_user`,
`test_a_named_replacement_takes_over_and_the_old_note_leaves`, `test_notes_of_an_app_user_stay_theirs`,
`test_secrets_are_redacted`, `test_reserved_fields_cannot_overwrite_the_event`,
`vectors_are_kept_per_text_and_dropped_with_their_model`. Part of `cargo test -p cluaiz-agents`: 161 passed, 1 ignored on 2026-11-21 (Windows 11).

Long-chat memory through the Engine: `test/context-runtime/memory_suite.py` and `memory_journeys.py`
(results from October 2026, not rerun since).

## Pending

| Item | Why it matters | Needs |
|---|---|---|
| Rerun the memory journeys on the current Engine | their results predate later memory changes | a recorded run |
