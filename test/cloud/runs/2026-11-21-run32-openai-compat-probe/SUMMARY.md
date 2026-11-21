# Run 32: OpenAI request fields on a local model (probe)

Windows 11 (build 26300), Engine 0.1.0 debug build, Qwen3-0.6B Q4_K_M, isolated `CLUAIZ_HOME` on port
8011, `think_mode: "off"`. Four requests to `/v1/chat/completions`, written while auditing the READMEs
to check fields OpenAI clients send. Raw results below are the script's printed lines.

| Request | Result | OpenAI behaviour |
|---|---|---|
| `"stop": "5"` (a string) | HTTP 422: `stop: invalid type: string "5", expected a sequence` | a string or an array is accepted |
| `"stop": ["5"]`, "Count from 1 to 10, one number per line." | HTTP 200; the answer contains `5` and continues to `10` (finish `stop` = end of the answer) | generation stops before `5` |
| `"max_completion_tokens": 5` | HTTP 200; 576 completion tokens | at most 5 tokens |
| `"n": 2` | HTTP 200; 1 choice | 2 choices |

## Cause (code)

- `stop` and `logit_bias` are forwarded only to provider models (`api/src/handlers/chat/preflight.rs:192-200`);
  the local llama.cpp path never reads them.
- `ExternalChatRequest` (`api/src/handlers/chat/types.rs:34`) has `stop: Option<Vec<String>>` (no string
  form), no `max_completion_tokens`, no `n`, no `logprobs`, and does not deny unknown fields, so those are
  ignored without an error.

## Not proven

Only one model and one platform; streaming was not checked; `logprobs` and `logit_bias` were not sent.
