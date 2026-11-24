# Long input gives an empty answer after 152 seconds

Status: open, P2 answer correctness and latency under this setup. Engine root cause is unknown.

Repro: `python -B test/release_soak/soak.py --exe target/debug/deps/cluaiz.exe --home <isolated-home> --out <run>/smoke --model qwen3.5-0.8b-gguf-Q4_K_M --minutes 2 --seed 5500 --restart-every 2`. The seed places `long_input` at turn 3. It asks for marker 4917 after 1,500 repetitions of `word`, with `think_mode: off`, temperature 0 and 48 output tokens.

Expected: a nonempty answer containing 4917, or an explicit refusal/error. Actual: HTTP 200 after 151,910.94 ms, empty `choices[0].message.content`, and `reasoning_content` containing a `read_item` call shape. See `../smoke/requests.jsonl`, `../smoke/responses.jsonl`, `../smoke/checks.jsonl`, `../smoke/engine.log` and the resource samples. The same answer pattern was recorded in Run 52 using a binary with a different SHA-256 hash. The setup was a shared laptop with variable available GPU memory, so the exact latency is not a stable benchmark.

No Engine source was changed. The next diagnostic is a controlled same-binary comparison of the constructed prompt and model output on a quiet machine.
