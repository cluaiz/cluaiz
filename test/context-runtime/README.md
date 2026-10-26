# Context runtime: routing set

`routing_set.jsonl` holds 165 labelled user messages for the tool matcher
(`agents/src/tool_search.rs`): English, Hinglish and Devanagari; direct requests, paraphrases,
several tools at once, follow-ups (with `previous`), false-positive traps, chit-chat, and
requests no installed tool can do. `expect` lists the tool ids that should be suggested, or is
empty when none should be.

`calibrate.py` sends every message to a running Engine's CEP `tools.discover` and reports:

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

## Results without word lists (2026-10-27)

The Engine no longer has `words.json` (Hinglish aliases, stopwords, action, follow-up, time and
signal words): it runs models in every language, and the list covered two. The matcher now uses
the words as written in any script, IDF over the catalog, a file/link/code check, message length
for follow-ups (what a short message borrows earns at most a title), and description-only words
earn nothing. The 2026-10-24 numbers below were measured with the list, which was grown on this
same set, so they are not a fair baseline.

| | hit | card | chit-chat clean | trap cards | wrong cards | noise |
| --- | --- | --- | --- | --- | --- | --- |
| All rows | 78.7% | 52.8% | 100% | 1 | 8 | 25 |
| Held-out half | 76.0% | 52.0% | 100% | 0 | 4 | 13 |

By kind: direct 83.3%, follow-up 83.3%, paraphrase 57.9%, chit-chat clean 100%. By language:
English 82.9%, Hinglish (Latin) 85.7%, Devanagari 30.0%. A missed suggestion is not a missed
tool: the model searches the rest with `find_tools`. The one trap card is r124 ("move on with
your life": `move` is a filesystem function).

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

`--cases` picks the cases (`where:kv_reuse`, e.g. `start:chunk`); the output also shows the
prompt tokens processed.

**Selective recompute (P6.3, CacheBlend-like), tried 2026-10-24 and dropped.** The idea: move the
chunks, then compute the KV of a share of the moved tokens again where they now are, so they see
the text that is before them now. llama.cpp does not allow it: a batch must start right after the
last position the KV holds ("it is required that the sequence positions remain consecutive"), so a
token in the middle of the cache cannot be computed again. Every try failed at `llama_decode`
(-1) and fell back to processing the prompt. Doing it would need a change inside llama.cpp, which
the Engine does not make (llama.cpp parity).

**Recurrent and hybrid models (P6.3).** Their state cannot drop a tail, so before this every
turn whose prompt differed from the last generation even near the end processed the whole prompt
again. The Engine now takes llama-server's context checkpoints for them too (4 tokens before the
prompt end) and restarts there. Bonsai 27B (Qwen3.5, hybrid; 2026-10-24): the second turn of a
710-token chat took 706 of 742 tokens from the cache, against 0 of 747 before.

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

## Optional embeddings (`embed_eval.py`, P5.3 step 1)

Do installed embedding models help? Tools: is the expected tool the nearest by cosine (message vs
tool name and description), against the word matcher? Memory: is the planted message in the top 3
of a 200-message chat, against BM25 recall? Uses `/v1/embeddings` with models the user installed.

```bash
python test/context-runtime/embed_eval.py --url http://127.0.0.1:8011 --token <token> --models embeddinggemma-2-GGUF,bge-m3-GGUF
```

Results (2026-10-24):

| | Tools, all (108) | English (70) | Hinglish (28) | Devanagari (10) | Paraphrase (19) | Memory top 3 (4) |
| --- | --- | --- | --- | --- | --- | --- |
| Word matcher / BM25 (today) | 106 | 68 | 28 | 10 | 17 | 4 |
| EmbeddingGemma (Q8_0) | 75 | 47 | 19 | 9 | 11 | 4 |
| bge-m3 (Q8_0) | 55 | 36 | 14 | 5 | 7 | 4 |

The best tool's similarity hardly separates real requests from traps and chit-chat (EmbeddingGemma
median 0.687 against 0.625; bge-m3 0.495 against 0.390), so a threshold would either miss tools or
suggest them for chit-chat. The plan's gate ("the paraphrase subset improves by a clear margin")
is not met, so hybrid scoring is not added; vectors stay off. A paraphrase-heavy memory set (facts
asked in other words or another language) is the next thing to measure before trying again.

## Tool-call format (`tool_calls.py`, P6.1)

Does a model call the bound tools well-formed, and is its answer right, with tools inline
(`<tool_call>` blocks) or native (`context_policy: {"tool_format": "native"}`)? 14 rows: 11 need a
tool (`filesystem`, `pynotes`, `everything`, `code-interpreter`; one asks for two tools in turn), 3
need none. Every row binds the tools with `tools`, turns suggestions off, approves every request
and uses `storage: "none"`. A row passes when the expected functions were called, the answer holds
the expected text, no call markup is left in it, and no error came.

```bash
python test/context-runtime/tool_calls.py --token <token> --model <model id> [--extra '{"context_policy":{"tool_format":"native"}}'] --out results.jsonl
python test/context-runtime/tool_calls.py --rescore results.jsonl
```

Results (2026-10-24, temperature 0, thinking as the model defaults, RTX 3050 4 GB):

| Model | inline: pass / called right / right answer | native |
| --- | --- | --- |
| Gemma 4 E4B (Q4_K_M) | 13/14, 11/11, 13/14 (was 6/14, see below) | 14/14, 11/11, 14/14 |
| Qwen3-4B (Q4_K_M) | 13/14, 10/11, 13/14 | 14/14, 11/11, 14/14 |
| Llama 3.2 3B (Q4_K_M) | 4/14, 6/11, 4/14 | same (its GGUF template has no tools, so they stay inline) |

Third family (2026-10-24): Llama 3.2 3B's GGUF carries a 348-character template with no tools, so
for one run the test home's model registry got llama.cpp's official
`models/templates/meta-llama-Llama-3.2-3B-Instruct.jinja` (the same as llama-server's
`--chat-template-file`). `template_check.py` then passed native too, but `tool_calls.py` native gave
3/14 (5/11 called right) against 4/14 inline: the model called the same function again and again
(up to 15 times) although each result was in its prompt. No gain, so nothing ships for it; the
registry was set back.

Inline, Gemma 4 first scored 6/14: it called the tools but then often wrote no answer, or answered
from a tool result it made up. `template_check.py` found why: Gemma 4's template has no `tool` role
and dropped every tool result without an error, so the model never saw one. Tool results now go to
such a template as `<tool_response>` user turns; the same run then gave 13/14 (2026-10-24). The one
fail, note-groceries, called `read_note` with the Hinglish words "groceries wala" as the name. The plan's gate asks for three families; only two of the installed models have templates
that render tools, so the third is still open.

With `--suggest` the rows bind no tools: the model sees the Engine's suggestions and loads what it
needs with `load_tool` (the App's usual case). `--rescore` scores a results file again after a
change to the rows.

`calibrate.py` also reports the `auto_load` rule (exactly one card, ahead of every other candidate
by a margin). On the routing set (2026-10-24): margin 2 loads a tool for 67 of the 108 rows that
need one, 66 of them right (r016 "test this node.js function by running it" gets `everything`),
and never for a row that needs no tool. See `docs/api/cep/ai-inference/context/README.md` for the
`auto_load` and `mcp_prewarm` timings.

## Journeys (`journeys.py`, plan §10.2 J-suite)

The context runtime end to end, against an isolated Engine it starts (`--exe`, `--home`): what
reaches the model for chit-chat, suggestions and `load_tool`, three MCP servers in one answer,
approve and reject, a restart in the middle of a chat, `tool_choice: "none"`, `read_tool_file`,
idle unload with a pinned tool, a killed MCP server, a wrong load then unload, a skill too big for
the room left. The fixture folder (`--workspace`) holds `notes.txt` ("The secret word is MANGO-42.")
and `calc.py` (`print(6 * 7)`).

```bash
python test/context-runtime/journeys.py --exe target/release/cluaiz.exe --home <test home> \
    --workspace <fixture> --log <log file> --model gemma-4-e4b-it-gguf-Q4_K_M --think off [--only J4,J6]
```

A journey that depends on the model can fail on a small model while the Engine is right; the
detail line says what happened. Results (2026-10-24, Gemma 4 E4B Q4_K_M, thinking off, RTX 3050
4 GB): 15 of 15 pass (16 of 16 with J9T, rerun after the PERF2 and PERF5 changes).

| Journey | Time | What it saw |
| --- | --- | --- |
| J1 "Hi" | 9.5 s | no suggestion, 0 hint tokens, no call |
| J2 "I trust you" | 9.5 s | no suggestion |
| J3 "protocol kya hota hai" | 89.7 s | no tool-creator suggestion, no call |
| J4 "notes.txt padho" | 24.8 s | `load_tool`, then `read_text_file` after one approval; MANGO-42 |
| J5 "run this python: print(2+2)" | 28.6 s | `load_tool`, `execute_code`; 4 |
| J6 wrong load, then unload | 69.1 s | load code-reviewer, unload it, load code-interpreter, run; next turn shows `[Tool 'code-reviewer' was unloaded.]` and none of its instructions; it processed 2906 of 2948 tokens again (cause `edit`) |
| J7 three MCP servers | 56.7 s | filesystem, pynotes and everything in one answer |
| J8 / J9 non-streaming approve / reject | 9.3 / 8.7 s | answer from the file / the model says it was not allowed |
| J10 restart mid-chat | 53.8 s | filesystem still loaded, no second `load_tool`, 3234 tokens from the saved KV |
| J11 skill too big | 24.4 s | a 400 KB skill is refused with its size; no line of it in the prompt |
| J12 `tool_choice: "none"`, `tool_search: false` | 22.4 s | no suggestion, no meta-tools |
| J13 `read_tool_file` | 17.6 s | read `references/LANGUAGES.md` whole; "go run" |
| J14 idle unload | 14.1 s | filesystem gone after the 3rd answer without use; pinned pynotes stays |
| J15 killed MCP server | 26.6 s | 4 processes killed; the next call starts it again and reads calc.py |

J6's next turn processing nearly the whole prompt again is the cost of a wrong load: the load and
unload change the tools part near the start of the prompt.

## Speed and memory (`perf.py`, plan §10.2 PERF)

Times come from the context report's `timing` (the Engine's own clock). PERF5 (how fast a request
stops an idle job) is in `memory_journeys.py`. Results (2026-10-24, Qwen3-4B Q4_K_M, RTX 3050 4 GB):

| Test | Result |
| --- | --- |
| PERF1 tools part of preflight (median of 5) | 2 ms with the installed tools, 4 ms with 100 installed; catalog and matcher alone 0.21 ms and 1.54 ms (P1 gate: under 2 ms) |
| PERF2 chat of 20012 messages | first request (logs and indexes the chat) 2-4 s; 20 requests after it: steps B-H 13 ms median, 14 ms p95 (ingest 8, render 2, recall and plan 3); whole preflight 23 ms (it was 939 ms: ingest 217, recall 209, a Chat Map rebuild of about 300 ms) |
| PERF3 Engine memory | 3206.5 MB with the model loaded, 3242.7 MB with the chat indexed (+36 MB) |
| PERF4 tokens processed per turn | 19 of 20, 33 of 49, 36 of 81, 47 of 124 (cause `edit` each time) |
| PERF5 a request during an idle-time job | see `memory_journeys.py` below: +81, +228 and +483 ms to the first text |
| PERF6 cold start | `/health` answers 1.5 s after start; first chat 3.2 s, of which model load 2.2 s |

In PERF4 the reuse stops a few tokens before the end of the previous prompt (16 of 20 on turn 2),
so each turn processes the previous answer again. Checked with `return_prompt` (2026-10-24): the
generation prompt ends in `<think>

</think>

` (thinking off), and Qwen3's template renders
an earlier answer without it, so the cache and the new prompt part there. llama-server does the
same. Moving the answer's KV over the gap (`kv_reuse: "chunk"`) would keep KV computed after text
the prompt no longer has, so it stays off by default. The cost is small: after a 417-token answer
the next turn processed 436 tokens in 647 ms; after a short one, 25 tokens in 838 ms.

## Chat templates (`template_check.py`, plan P2.3)

A conversation with a system rule, a tool call, its result, an answer and a new question, rendered
by each installed model's template inline and native (`return_prompt`). A model fails when a part
is not in its prompt, or when the Engine's own check (`template_dropped`) finds a dropped message.

Results (2026-10-24): Qwen3-4B, Gemma 4 E4B and Llama 3.2 3B pass inline and native (Llama's
template renders no tools, so native falls back inline). The first run found Gemma 4's template
dropping tool results inline; see the tool-call format section above.

PERF2 went from 939 ms to 32 ms of preflight on 2026-10-24: the event logs of the 4 most recent
chats stay parsed in RAM (every request used to parse the whole 20k-event log twice), the client's
messages are matched against the log without copying it, and a request uses the last Chat Map
while a new one is built after the answer, and the 20k messages the client sends are compared
with the log borrowed, not copied. Steps B-H of the design (ingest, recall, plan, render) now take
13 ms median and 14 ms p95 at 20k messages (target 15 ms p95); this run had no tool matching, which
adds at most 1.5 ms with 100 tools (PERF1).

**Speed numbers on this machine are low for a reason outside the Engine.** The test Engine runs
next to the user's own server, which holds most of the 4 GB of VRAM: the planner gave the test
Engine 1.21 GB, so Gemma 4 E4B had 5 of 42 layers on the GPU. Every prompt then costs about 650 ms
however short (19 tokens 670 ms, 214 tokens 678 ms, 415 tokens 815 ms), a token about 85 ms, and
an idle-time card 22-30 s.

## Memory journeys (`memory_journeys.py`)

The plan's memory tests that the long-chat suite does not cover, plus ENR1, PERF5 and the C2
J-variant. MEM3 and MEM5 import a chat with its own times and tool steps, so their checks do not
depend on the model.

```bash
python test/context-runtime/memory_journeys.py --exe target/release/cluaiz.exe --home <test home>     --workspace <fixture> --log <log file> [--model gemma-4-e4b-it-gguf-Q4_K_M] [--only MEM3,MEM5]
```

Results (2026-10-24, Gemma 4 E4B Q4_K_M, thinking off): 6 of 6 pass, and `journeys.py` 16 of 16.

| Test | Result |
| --- | --- |
| MEM3 time questions | "kal", "parso", a weekday and "aaj" each found only that day's port (7070, 5050, 5050, 9090) |
| MEM5 recalling a tool output | the run's result came back at level `tool_only` (`code-interpreter returned: stdout: checksum ZEBRA-7781 …`); the answer gave ZEBRA-7781, which no message had |
| MEM6 cross-chat | `allow`: the model called `recall` and found the codename; `deny`: CEP `memory.search` with `scope: all` gave `-32002` and the answer had no codename; `ask` with a reject: no codename |
| ENR1 card on the hot KV | the card was written, and the next request processed 9 of 418 tokens (14 of 415 with no job) |
| PERF5 preemption | first text +81, +228 and +483 ms in three runs (at most the rest of one 64-token chunk, about 650 ms on this machine); before the fix +1,389 ms |
| C2 earlier tool step in client mode | the 4th turn's prompt had the first turn's `read_text_file` call and its result; the answer quoted the file's first line |
| J9T (in `journeys.py`) | nobody answered the approval: the request waited 140 s and the tool did not run |
| EXT1 `storage: "external"` (2026-10-25) | the first turn's model card and a `remember` note came in the second answer's `memory_events`, each event once; no chat folder and no note in the Engine's files |

MEM5 found that `tool_only` (design §11.7) was missing: recall showed the answer around a tool
run, never its output. PERF5 found that a job could not be stopped while its prompt was processed
(only between tokens): a request waited up to the whole prompt. Both are fixed. The plan asks for
50 ms; on this machine one decode alone takes longer (see above).

## Faster grammar sampling and card speed (2026-10-25)

With the founder's server stopped (3.3 GB VRAM free), the planner put 22 of 42 Gemma 4 blocks and
21-22 of 36 Qwen3 blocks on the GPU. Card jobs were still slow: Gemma 4 wrote 7.5 tokens/s under
the card's JSON grammar against 15 without it, because the grammar was first in the sampler chain
and went over the whole vocabulary (262k tokens) for every token. A request's own grammar (cards,
`grammar`, `response_format`) now checks only the token the sampler picked, as llama.cpp's
`common_sampler_sample` does, and goes over the vocabulary only when it rejects that token. The
native tool-call grammar is not changed.

| | before | after |
| --- | --- | --- |
| Gemma 4 E4B card (84 tokens) | 27.2 s, 7.5 tokens/s | 7.0 s, 14.6 tokens/s |
| Qwen3-4B card (56 tokens) | 9.8 s, 11.6 tokens/s | 4.1 s, 16.2 tokens/s |
| `response_format` JSON schema | — | Gemma and Qwen: valid JSON with every required key |

ENR1 passes with both models; `tool_calls.py` native on Gemma 14/14 after the change.

**A run-to-run trap found on the way.** The planner sizes `n_ctx` from the free VRAM, which moved
between runs (9216, 8704, 8192 tokens for Qwen3). Qwen3 with thinking failed 4 of 5 tool rows at
8704 and 8192 and passed 5 of 5 at 9216, with the old and the new build alike (A/B with
`"llama": {"n_ctx": 9216}` in every request: 5/5 and 5/5). Compare tool-call runs at a fixed
`n_ctx`.

**J15 now restarts the Engine first and checks the Engine's part.** Run back to back in one
Engine, its third kill hit the crash limit (3 in 10 minutes), so the call failed by design. With
the restart working, Gemma 4 wrote no answer after the `calc.py` result (its first token was
`<eos>`; at temperature 0 every time, with the old build too), although the result was in its
prompt. J15 therefore checks that the server started again and that its result reached the model
(`return_prompt`), and shows the answer in its detail line.
