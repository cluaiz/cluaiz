# Long prompt returns an empty answer

Status: open observation, severity P2 for answer quality. This does not prove an Engine root cause.

## Reproduce

Run `python -B test/release_soak/soak.py --exe target/debug/deps/cluaiz.exe --home <isolated-home> --out <run>/raw --model qwen3.5-0.8b-gguf-Q4_K_M --minutes 4 --seed 5204` with the model installed in the isolated home. The fixed seed puts `long_input` at turns 3 and 26. The request sends `The marker is 4917. `, then `word ` repeated 1,500 times, then `What is the marker?`. `think_mode` is `off`, `max_tokens` is 48, temperature is 0.

Expected: a nonempty answer containing `4917`, or an explicit error if the input cannot be handled.

Actual: both requests returned HTTP 200 with `choices[0].message.content` empty. `reasoning_content` was `{"arguments":{"name":"cluaiz/compare/cluaiz/"},"name":"read_item"}`. The check failed twice. See `../raw/checks.jsonl`, the two matching requests in `../raw/requests.jsonl`, and the responses immediately following their request times in `../raw/responses.jsonl`.

The model might have produced a tool-shaped completion because of its template, the prompt, or Engine handling. Cause and source file: ?. A shorter exact marker prompt was not run as a control in this session.
