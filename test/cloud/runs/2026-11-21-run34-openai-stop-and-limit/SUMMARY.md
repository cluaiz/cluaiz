# Run 34: `stop` and `max_completion_tokens` on an installed model (after the fix)

Windows 11 (build 26300), Engine 0.1.0 debug build (`cargo build -p cmd`), Qwen3-0.6B Q4_K_M, isolated
`CLUAIZ_HOME` on port 8011, `think_mode: "off"`, `temperature: 0`. The same kind of requests as run 32,
after the fix. Script: `probe.py`; its output: `results.txt`.

| Request | Run 32 (before) | Run 34 (after) |
|---|---|---|
| `"stop": "5"` (a string), count 1 to 10 | HTTP 422 | HTTP 200, answer `1..4`, `finish_reason: stop`, 8 tokens |
| `"stop": ["5"]` | answer went on to 10 | answer `1..4`, `finish_reason: stop` |
| `"stop": ["7", "3"]` | - | answer `1, 2`: the first match in the text wins |
| `"stop": ["5"]`, streaming | - | streamed text `1..4`, `finish_reason: stop` |
| `"max_completion_tokens": 5` | 576 tokens | 5 tokens, `finish_reason: length` |
| `max_tokens: 8` and `max_completion_tokens: 3` | - | 8 tokens: `max_tokens` wins |
| no `stop` (control) | - | `1..10`, 20 tokens |

## What changed (code)

- `api/src/handlers/chat/types.rs`: `stop` is a string or a list (`StopList`); `max_completion_tokens` is
  read, and `answer_limit()` uses it when `max_tokens` is missing.
- `engine/core/src/chat.rs`: `ChatEnvelope.stop` carries the strings to the runtime.
- `engine/llama/src/native/stream.rs`: answer text that could begin a stop string is held back; at a
  match the text before it is sent and generation ends (`stop_match`, unit test `stop_tests`, run on its
  own with `rustc --test`: 1 passed).

## Not proven

One model, one platform. Media requests (images, audio) with `stop` were not sent. The `api` and `llama`
crate test suites were not run (they rebuild llama.cpp with CUDA); the change was checked by this run.
