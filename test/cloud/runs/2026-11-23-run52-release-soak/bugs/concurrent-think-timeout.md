# One concurrent thinking chat exceeded the client timeout

Status: open observation, severity P2 for latency under this setup. Engine root cause is unknown.

## Reproduce

Run `python -B test/release_soak/soak.py --exe target/debug/deps/cluaiz.exe --home <isolated-home> --out <run>/raw --model qwen3.5-0.8b-gguf-Q4_K_M --minutes 3 --seed 5201`. Seed 5201 puts `concurrent` at turn 3. Three clients each ask `What is 2+2?` at the same time, with the then-current client default thinking behavior, 48 output tokens and a 180-second HTTP timeout.

Expected: three valid replies before the client timeout. Actual: one response entry says `TimeoutError('timed out')`, `elapsed_ms: 180009.41`; the scenario failed. Two other concurrent replies included 200, one after 46,252.57 ms. See `../raw/checks.jsonl`, `../raw/requests.jsonl`, `../raw/responses.jsonl`, `../raw/engine.log` and `../raw/memory.jsonl`. The Engine process was stopped by the test client after the scenario.

Control: with `think_mode: off`, seed 5202 finished 31/31 checks and four concurrent scenarios passed. This limits the observation to the thinking/default path on this shared laptop. No same-settings repeat of the timeout was made. Cause and source file: ?.
