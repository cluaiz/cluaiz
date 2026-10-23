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
