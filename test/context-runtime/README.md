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

# KV correctness after an edit (`kv_equality.py`)

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
