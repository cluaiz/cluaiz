# Run 78: max_tokens as one budget for the whole tool loop (2026-11-25)

Fix: 95476879 fix(api): max_completion_tokens wins and the limit is one budget for the whole tool loop.

| Item | Value |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 4 GB |
| Server | target/debug cluaiz.exe serve (cargo build -p cmd, code = 95476879), isolated CLUAIZ_HOME (copy of run 73's home: math plugin), CLUAIZ_PORT=8013, 127.0.0.1; auth untouched |
| Model | Qwen3-0.6B Q4_K_M |
| Script | budget_probe.py (uses run 73's loop_probe.Server) |
| Rerun | python -B budget_probe.py --exe <cluaiz.exe> --home <home> --port 8013 --out <dir> --label after_thinkoff --think-off --model qwen3-0.6b-gguf-Q4_K_M --n 5 |

## Results (raw/after_thinkoff.jsonl, think_mode off, 5 requests per case)

| Case | Before (run 72 soak, Linux) | After |
|---|---|---|
| max_tokens 48 + tools | completion_tokens 192, finish_reason length (19 / 19 failures) | completion_tokens 48, length, 5 / 5 (0-2 tool steps) |
| max_tokens 48 + max_completion_tokens 5 | limit 48 used (max_tokens won) | completion_tokens 5, length, 5 / 5 |
| no limit | - | 142-308 tokens, stop, 5 / 5, 3-5 tool steps (unchanged path) |
| streaming max_tokens 48 | - | completion_tokens 48, length, 5 / 5 (1-2 tool steps) |

No local before build: target/debug/cluaiz.exe was held by a server on port 8000 started at 09:26 (not
this run's), so it could not be rebuilt and was not touched. Before numbers are run 72's.

## Findings

- In all limited cases the content is empty: the 48 tokens went into tool calls (the call cut by the
  limit is not run, existing rule).
- An intermediate build counted only streamed tokens: the native path does not stream a call's tokens,
  so usage showed 0-1 tokens with tool steps run (raw/intermediate_thinkon.jsonl is that build). The
  final code counts each step from the engine report (n_tokens + shifted_tokens - prompt_tokens), at
  least the streamed tokens.
- Thinking on (intermediate build, raw/intermediate_thinkon.jsonl): 157-484 tokens for max_tokens 48:
  the first step's thinking has its own ceiling in engine/llama native/stream.rs (by design), so the
  limit does not cover thinking. Not changed in this run.

## Not done

- No before build on this laptop; no thinking-on run with the final build; no unit test for the budget
  (api crate suite not run). The report is read after a step that stopped early on a complete call;
  if the worker had not written it yet, the streamed count is used (not reproduced).
