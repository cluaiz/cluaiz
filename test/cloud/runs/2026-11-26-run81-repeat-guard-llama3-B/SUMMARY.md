# Run 81: repeat guard, Llama 3.2 3B through run 77's setup B on the laptop (2026-11-26)

## Setup

| Item | Value |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 Laptop 4 GB (run 77 ran on the L4 VM) |
| Build | debug `cargo build -p cmd` of `5fd0977d` ("stop repeated identical tool calls: reuse on the second, end steps on the third") |
| Probe | run 77's `matrix_probe.py`, unchanged, from its folder: `--setup B --think off --model llama-3.2-3b-instruct-gguf-Q4_K_M`, its 40 prompts, temperature 0; it starts its own server on port 8013 |
| Home | isolated test home `run81home`: config and tools (math plugin, filesystem MCP) copied from the run 79 home, the Llama 3.2 3B Q4_K_M registry entry from the run 58 home (same file as run 77: `bartowski/Llama-3.2-3B-Instruct-GGUF`); no session token copied, auth untouched |
| Rerun | `python -B test/cloud/runs/2026-11-25-run77-small-model-tool-count/matrix_probe.py --exe target/debug/cluaiz.exe --home <isolated-home> --out <dir> --model llama-3.2-3b-instruct-gguf-Q4_K_M --setup B --think off`; counts with run 77's `analyze.py` `score()` |

## Results (counted with run 77's `analyze.py`)

| | Run 77, L4, build `f8d6b4b2` | Run 81, laptop, build `5fd0977d` |
|---|---|---|
| Correct / 40 | 23 | 33 |
| Loop prompts (same Engine call and payload twice or more) | 11 | 0 |
| Repeated calls | 163 | 0 |
| Tool steps, total / max per request | 248 / 99 | 9 / 4 |
| Requests at the step or repeat limit | 6 | 1 |
| HTTP errors | 0 | 2 (requests 34 and 39: prompt 3,490 and 3,691 tokens over the 3,328-token context the planner chose for 4 GB VRAM) |

The new guard fired once (`raw/server.log`): `read_tool_file` with the same arguments was not run again at step 3
("its earlier result is returned") and tool steps stopped at step 4 ("came 3 times").

## What this proves

- The guard works live: a third identical call ends the tool steps and the request still gets an answer.
- On this build and home, Llama 3.2 3B in setup B no longer loops (0 loop prompts, 9 steps in 40 requests).

## What this does not prove

- **The run 77 loop is not reproduced here.** Run 77's loops were `read_item` calls (517 lines in its Llama 3B log);
  in this home `read_item` was never offered (0 lines), because the home had no project items for it (`read_item`
  is offered only when project items are indexed or cut, `api/src/handlers/chat/project.rs:33`). So most of the
  difference comes from the different tool set, not from the guard. The run 77 call orders were replayed through the
  guard in unit tests instead (`agents/src/turn.rs`, `calls_that_alternate_are_caught_too`).
- Different machine, context size and build than run 77; one pass; one model.

## Files

`raw/raw.jsonl` (40 requests: prompt, reply, grade, engine calls, log lines), `raw/math.json`, `raw/server.log`.
User name replaced with `<user>`.

## Pending

| Item | Needs |
|---|---|
| Live proof on `read_item` loops | rerun setup B in a home with run 77's project items (skills, rules, memory), Llama 3.2 1B and 3B |
