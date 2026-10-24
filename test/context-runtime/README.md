# Context runtime: routing set

`routing_set.jsonl` holds 165 labelled user messages for the tool matcher
(`agents/src/tool_search.rs`): English, Hinglish and Devanagari; direct requests, paraphrases,
several tools at once, follow-ups (with `previous`), false-positive traps, chit-chat, and
requests no installed tool can do. `expect` lists the tool ids that should be suggested, or is
empty when none should be.

`calibrate.py` sends every message to a running Engine's `POST /v1/tools/discover` and reports:

- **hit**: rows with tools where every expected tool is suggested (card or title)
- **card**: the same, as cards
- **chit-chat clean**: chit-chat rows with no tool text at all
- **trap cards**: rows that need no tool but got a card (must be 0)
- **wrong cards**: rows with tools that also got a card for a tool not expected
- **noise**: suggestions shown with rows that need no tool

It then tries other level rules on the returned evidence, fitted on one half of the rows and
reported on the other half (the split is a hash of the row id).

The labels assume the tools of the isolated test home: `filesystem`, `pynotes`, `everything`,
`git` and `fetch` (MCP), and `code-interpreter`, `code-reviewer`, `learn-cluaiz`, `learn-ai`
and `tool-creator` (skills). Run it against such a home, never the user's own server:

```bash
CLUAIZ_HOME=<test home> CLUAIZ_PORT=8011 target/release/cluaiz.exe serve
CLUAIZ_HOME=<test home> python test/context-runtime/calibrate.py --url http://127.0.0.1:8011
```

The exit code is 0 when the held-out half meets P1.3: hit ≥ 90%, chit-chat clean ≥ 98%, no
trap cards.

## Results (2026-10-24, matcher only, no model involved)

| | hit | card | chit-chat clean | trap cards | wrong cards | noise |
| --- | --- | --- | --- | --- | --- | --- |
| Before (all rows) | 89.0% | 67.0% | 100% | 3 | 7 | — |
| After, fit half | 96.6% | 74.1% | 100% | 0 | 0 | 8 |
| After, held-out half | 100% | 76.0% | 100% | 0 | 1 | 12 |

The fit preferred a looser title rule (one description word is enough). It gains nothing on the
held-out half and nearly doubles the noise there (22 against 12), so the Engine keeps the
current rule. Still missed: "look at the diff between the main and dev branches" and "find bugs
and vulnerabilities in my function"; both need the `git` function list (its server cannot start
without `uvx` here) or a trigger on `code-reviewer`.

## KV correctness after an edit (`kv_equality.py`)

A chat carries a "skill" that says to answer only in French; then the skill is unloaded and the
same chat comes back without it. The answer computed on the cached KV must equal the answer from
a clean prefill of the same prompt (KV1) and must not be in French (KV2). Temperature 0, seed 42,
thinking off; every run also does the clean prefill.

- `start`: the skill is the system prompt; unloading deletes it, a deep edit at position 1.
- `tail`: the skill was loaded in the last turn; the long history before it stays cached.
- `start, chunk`: the same edit with `kv_reuse: "chunk"`, which was the behaviour for every
  request before 2026-10-24.

```bash
CLUAIZ_MODEL=<model id> python test/context-runtime/kv_equality.py --url http://127.0.0.1:8011 --token <token> --runs 10
```

Results (2026-10-24, Qwen3-4B Q4_K_M, isolated 8011 server, 10 runs each):

| Case | KV1 equal to clean | KV2 not French | cache_n |
| --- | --- | --- | --- |
| start, default (prefix) | 10/10 | 10/10 | 1 |
| start, chunk | 0/10 | 0/10 | 1371 (all moved chunks) |
| tail, default (prefix) | 10/10 | 10/10 | 1347 |

With chunk reuse the unloaded instruction keeps its effect: the answer stays French and is wrong
("La Terre est la plus grande planète…", where the clean answer is "Jupiter is the largest
planet…"). That is why chunks are now moved only after the Engine dropped old turns to fit.

## Long tool session and equality after compaction (`long_session.py`)

LONG1: one chat of 24 turns on `n_ctx` 4096, each turn asks about a file and usually reads it
with the filesystem tool, sent like the App sends it (`session_id`, plain-text history). KV3: a
short chat with tool steps; the same request is compacted and answered on the cached prefix,
then answered again on the compacted prompt from a clean KV. A control does the same without any
edit, because llama.cpp's results can differ slightly with the batch shape.

```bash
CLUAIZ_MODEL=<model id> python test/context-runtime/long_session.py --url http://127.0.0.1:8011 --token <token> --turns 24 --n-ctx 4096
```

Results (2026-10-24, Qwen3-4B Q4_K_M, isolated 8011 server):

| | First run (stop at the 512-token minimum) | With `compact_to_pct` 60 |
| --- | --- | --- |
| Context errors | 0 of 24 turns | 0 of 24 turns |
| Compactions | 12 | 10 |
| First turn the engine dropped old messages | 18 | 17 |
| Answers | correct every turn | correct every turn |
| KV3 compaction: incremental equals clean | no compaction (nothing left to compact at turn 24) | equal (1,600 tokens freed, edit at position 3) |
| KV3 control (no edit): cached equals clean | differed in one word ("secret" / "गुप्त"), 3,880 cached tokens vs clean | equal (2,820 cached tokens) |

The first run's no-edit difference shows that a long cached prompt and a clean prefill of the
same prompt can give different greedy text on this GPU (different batch shapes), so KV3 compares
against the control, not against an exact-match promise.

After about 16 turns the history is mostly the client's own text (questions and answers), which
compaction never changes, so the engine drops the oldest turns as before; no request failed.

## Long-chat memory (`memory_suite.py`)

Synthetic chats of 200, 1,000 and 2,000 messages with facts planted at 5%, 50% and 90% depth, a
fact corrected later ("Monday" then "Wednesday"), and a question about a fact never given. Each
question is a new chat sent like the App sends it; `recent` against `adaptive` (with recall).

```bash
CLUAIZ_MODEL=<model id> python test/context-runtime/memory_suite.py --url http://127.0.0.1:8011 --token <token> --sizes 200,1000,2000
```

Results (2026-10-24, Qwen3-4B Q4_K_M, `n_ctx` 4096):

| Size | recent: correct | recent: prompt tokens | adaptive: correct | adaptive: prompt tokens |
| --- | --- | --- | --- | --- |
| 200 | 4/5 | 3,982 | 4/5 | 466 |
| 1,000 | 4/5 | 3,985 | 4/5 | 474 |
| 2,000 | 3/5 | 3,992 | 4/5 | 475 |

The absent fact failed in every mode (the model invented a passport number). The first run of
this suite found two bugs, fixed before these numbers: the engine dropped almost the whole
history of a long chat (template tokens were not counted per message, so 2,010 of 2,011
messages went and prompts fell to 30-100 tokens), and recalled corrections came in score order,
so the model picked the corrected-away value.

With `history_mode: "none"` (only the system prompt and the current message, plus recall),
2026-10-24: 200 messages 5/5 at 603 prompt tokens, 2,000 messages 5/5 at 575; the absent fact
was answered "not known" in this mode.
