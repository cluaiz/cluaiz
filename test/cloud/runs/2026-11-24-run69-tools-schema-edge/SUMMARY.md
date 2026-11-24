# Run 69: tools and response_format, the paths run 68 did not test (2026-11-24)

Run 68 tested `790819e2` (image and audio requests get tool schemas) and `5cec4478` (response_format, grammar and
tool_choice with tools) with thinking off, not streamed, text and image, one 2B vision model. This run covers the
rest on the same laptop: streaming, thinking on, `json_schema` with thinking and with streaming, audio, and a
text-only regression. One bug was found and fixed (`e654894f`).

Hashes: the unpushed history was rewritten during this run to redact two raw logs before the push (2026-11-24 about 20:30 IST);
the old hashes used in run 68 map to new ones: `790819e2` -> `1ffb2f82`, `5cec4478` -> `9dde6a3e`, `1dcf2212` ->
`730c3f79`. This run's fix was committed as `382c4bd2` and is now `e654894f` (same content).

| | |
|---|---|
| Machine | Windows 11 laptop, Ryzen 7 7435HS, RTX 3050 4 GB, shared with other work (another agent's Engine and other programs ran at the same time; see "Not proven") |
| Server | a copy of `target\debug\cluaiz.exe` + DLLs in a scratch folder outside git, run 68's isolated `CLUAIZ_HOME` (its `model_registry.json` points at GGUF files of earlier scratch homes, read only), port 8013, auth settings at their defaults |
| Binaries | debug builds (`cargo build -p cmd`): base = the code of `5cec4478` (HEAD `1dcf2212` has no code change after it; SHA-256 `f353bdb12ce4d39c671ae8e606a5bd6f1eed8ac23126c18762dc39e890a5efaf`, the same file as run 68's final); fix = the code of `e654894f` (SHA-256 `4868560c1060529f6be04261cca3735b31c1d8787cdca450012d055c6d67872a`) |
| Models | vision: `qwen3.5-2b-mtp-gguf-Q4_K_M` (Qwen3.5-2B Q4_K_M + mmproj-F16); text: `qwen3-0.6b-gguf-Q4_K_M`; audio: Qwen3-ASR-0.6B Q8_0 + its mmproj (the only audio model on the laptop, from the Hugging Face cache, hard-linked into a scratch folder; registered with the chat template from its own GGUF) |
| Settings | vision, thinking off: temperature 0, `max_tokens` 128 (run 68); vision, thinking on: temperature 0.6, `max_tokens` 2048; text, thinking off: temperature 0.8, `max_tokens` 200 (run 65); text, thinking on: temperature 0.8, `max_tokens` 2048 |
| Script | `edgecheck.py` (reuses run 68's `fixcheck.py`, which reuses run 64's image and `get_weather` and run 65's cases and judges); `summarize.py` makes the tables; `prompt_probe.py` asks for the rendered prompt (`return_prompt`); `weather_question.wav` is the spoken question (Windows SAPI voice, 16 kHz mono) |

`raw/<folder>/`: `results.jsonl` (every request: status, judged result, answer, reasoning length and start,
finish reason, stream facts, and the engine log lines of that request), `summary.jsonl`, `engine.log`,
`console.txt`. Folders: `base` (base binary, all cases except audio), `fix` (fixed binary, text-model cases),
`fix-vision-text` (fixed binary, the Qwen3.5-2B text-only cases), `audio-smoke` (base binary, audio cases, 1 request each), `smoke` / `smoke2` (base binary, 1 request each, the
first look), `fix-aborted-lowram`, `fix-v-aborted-lowmem`, `fix-try` and `fix-media-lowram` (fixed binary, image and vision-model
requests the memory checks refused because the laptop had no free memory; kept as the record; only the 2 answered
requests in `fix-media-lowram` are counted). User name replaced by `<user>`.

How a case is judged:
- A streamed reply is read chunk by chunk (SSE) and folded into the shape of a non-streamed reply. A stream also
  needs `data: [DONE]`, a `finish_reason` and no error chunk.
- A tool case needs a `get_weather` call whose arguments are JSON with a non-empty `city`, and no call markup
  (`<tool_call>`) left in the answer text.
- A thinking case also needs a non-empty `reasoning_content` (the thinking text still appears).
- "Constrained" counts requests whose own log shows `Answer constrained` or `grammar on (lazy false`. Run 68's rule
  matched any `lazy false`, which also matches `grammar none (lazy false` (no grammar at all); re-counted with the
  narrower rule, run 68's numbers do not change (checked over all four of its `results.jsonl`).

## Results (10 requests per case)

| # | Case | Base | Fixed | Notes |
|---|---|---|---|---|
| 1 | image + `get_weather`, streamed | 10 / 10 | not re-run (no free memory) | `finish_reason: tool_calls`, `{"city":"Paris"}`, 2 SSE chunks; the fix does not touch this path (below) |
| 1 | same question without the image, streamed | 10 / 10 | 10 / 10 | |
| 2 | image + `get_weather`, thinking on | 10 / 10, reasoning 10 / 10 | not re-run | lazy call grammar (`lazy true, 1 triggers`), thinking first, then the call |
| 2 | the same, streamed | 10 / 10, reasoning 10 / 10 | 2 / 2 that passed the memory check (8 refused by it) | `raw/fix-media-lowram/` |
| 2 | image + `get_weather`, `tool_choice: "required"`, thinking on | 10 / 10, reasoning 10 / 10, constrained 10 / 10 | not re-run | the strict template grammar covers the thinking block; thinking text intact (median 244 characters) |
| 2 | image, "What colour is this image?", thinking on, no tools | 10 / 10 "red", reasoning 10 / 10 | not re-run | |
| 2 | the same with `get_weather` offered | 10 / 10 "red", no call, reasoning 10 / 10 | not re-run | |
| 3 | `json_schema`, thinking on, Qwen3-0.6B | **0 / 10** (JSON valid 10 / 10, reasoning 0 / 10) | **10 / 10** (valid 10 / 10, reasoning 10 / 10) | the bug, below |
| 3 | `json_schema` + `get_weather`, thinking on | 10 / 10, reasoning 10 / 10 | 10 / 10 | already one template grammar with the thinking block |
| 3 | the same, weather question (call or schema JSON) | 9 / 10 | 8 / 10 | the misses are grammar-valid JSON whose `name` runs on to `max_tokens` (`finish_reason: length`); the model always chose JSON over the call |
| 3 | `json_schema`, thinking on, Qwen3.5-2B | 10 / 10, reasoning 10 / 10 | 10 / 10, reasoning 10 / 10 | Qwen3.5's template opens the thinking block in the prompt, so the old grammar already woke after thinking; after the fix it takes the template grammar (log line 10 / 10) and still thinks |
| 3 | `json_schema` + `get_weather`, thinking on, Qwen3.5-2B | 10 / 10, reasoning 10 / 10 | 10 / 10, reasoning 10 / 10 | |
| 3 | `json_object`, thinking on, Qwen3-0.6B | **0 / 10** (objects 10 / 10, 5 empty `{` + blank lines + `}`, reasoning 0 / 10) | **10 / 10** (no empty object, reasoning 10 / 10) | same bug |
| 3 | GBNF yes / no, thinking on, Qwen3-0.6B | 0 / 10 (answers in the grammar 10 / 10, reasoning 0 / 10) | 0 / 10 (same) | not changed: llama-server applies a user grammar as given, below |
| 4 | `json_schema`, streamed | 10 / 10 | 10 / 10 | |
| 4 | `json_schema` + `get_weather`, streamed | 10 / 10 | 10 / 10 | |
| 4 | `json_schema`, streamed, thinking on | **0 / 10** (reasoning 0 / 10) | **10 / 10** (reasoning 10 / 10) | same bug |
| 4 | `json_schema` + `get_weather`, streamed, thinking on | 10 / 10 | 10 / 10 | |
| 5 | audio + `get_weather` | 1 / 1 served, 0 calls | not run | see "Audio" |
| 6 | plain chat ("Say hello ...") | 10 / 10 | 10 / 10 | no grammar (`grammar none`) |
| 6 | weather question + `get_weather` (auto) | 10 / 10 calls | 10 / 10 | |
| 6 | `json_object` | 10 / 10 | 10 / 10 | after the fix through the template grammar (log line below) |
| 6 | GBNF yes / no | 10 / 10 | 10 / 10 | |

Constrained (log) was 10 / 10 in every schema, `json_object`, GBNF and `required` case, before and after. No
`not constrained` or `did not take` warning in any log. Full tables: `python -B summarize.py raw/base raw/fix`.

## Bug: `response_format` without tools blocked thinking (fixed in `e654894f`)

What happened: with thinking on, Qwen3-0.6B answered a `json_schema` or `json_object` request at once, with no
thinking (reasoning 0 / 10, 0.5 s median against 2.0 s after the fix). Half the `json_object` answers were empty
objects.

Cause: without tools, `engine/llama/src/native/stream.rs` turned the schema into a bare JSON grammar
(`json_schema_grammar`) and switched it on at the first token unless the prompt itself opened the thinking block
(`set_grammar(template_strict || !prompt_opens_thinking, "")`). Qwen3's template does not write `<think>` into the
prompt; the model writes it as its first token, and the JSON grammar refused that token. Qwen3.5's template writes
`<think>` into the prompt, so it was not affected. The bug is older than run 68's commits (the same condition was
there before `5cec4478`).

What llama-server does: it passes the schema into the template call for every chat request
(`inputs.json_schema`, `tools/server/server-common.cpp:1513`), and llama.cpp's template layer builds one strict
grammar that holds the thinking block and then the JSON (`common/chat-auto-parser-generator.cpp`,
`has_response_format`, `build_parser`).

Fix: `template_schema` (was `tools_schema`) is now set when tools are rendered natively or when llama.cpp's parser
reads the answer (`parse_answer`), and the no-tools template call carries `json_schema` too
(`engine/llama/src/native/stream.rs:303-312` and `:368-374` in `e654894f`; `template_schema` at `:309`, the no-tools `json_schema` at `:372`). The strict template grammar then
applies as it already did with tools. New log line (`stream.rs:873`): `Answer constrained by a JSON schema grammar
from the template, thinking block included (llama.cpp)`, 50 times in `raw/fix/engine.log` (the five schema /
`json_object` cases without tools). When the template's answer parser is not used (media prompts, content-only
templates), the old bare grammar stays.

Not changed: a GBNF `grammar` with thinking on. llama-server applies a user grammar as given from the first token
(`common/sampling.cpp`: "user-supplied grammars must not be prefilled"; the template layer does not wrap it), so
there too a model that writes its own `<think>` cannot think under a grammar. The docs said "after thinking"; they
now say when that holds (`docs/api/inference/chat_completions.md`).

## Audio

The only audio model on the laptop is Qwen3-ASR-0.6B, a speech recognizer. Its chat template (from its GGUF) has no
tools in it, so the API puts the tools into the system message as text (`template_renders_tools`,
`api/src/handlers/chat/tool_turns.rs:394`) instead of the native path that `790819e2` fixed. One request each
(`raw/audio-smoke/`): with `get_weather` 187 prompt tokens, without 60, so the schemas did reach the prompt; both
answers were the transcript `language English<asr_text>What is the weather in Paris right now?`, no call. The
native audio + tool path needs an audio chat model whose template renders tools; none is on this laptop.

Two registry attempts before the working setup: an entry without `metadata` made the Engine set the registry aside
(`model_registry.json.broken-...`; restored from a backup), and pointing `local_dir` at the Hugging Face snapshot
folder found no projector, because the files there are symlinks into `blobs/` and the projector is looked up next
to the canonical weight file. A folder of hard links fixed that.

## Why some cases were not re-run with the fixed binary

From 20:00 IST the laptop had 1.9 to 3.3 GB of available RAM (another agent's Engine, about 1.1 GB, and other
programs). Image requests then failed with `Media needs 1742571137 host bytes; live RAM budget is 0` (HTTP 200
stream with an error chunk; `raw/fix-aborted-lowram/`), and the 2B model did not load (`insufficient_memory`,
"1.20 GB short", HTTP 503; `raw/fix-v-aborted-lowmem/`). These are refusals of the memory checks, not answers.

The fix cannot change the image and tool cases: `template_schema` is unchanged when tools are rendered, and
`parse_answer` is false for any prompt with media, so those requests take the same code as the base binary. It
does change the Qwen3.5-2B text-only `json_schema` case (that prompt now gets the template grammar); that case was
re-run when the model fitted again: 10 / 10 with reasoning. The image path was refused by the memory check three
times in a row (`fix-aborted-lowram`, `fix-try`, `fix-media-lowram`: "live RAM budget" 0 to 0.95 GB against the
1.74 GB the image needs), so it was not tried again.

## Not proven

- Image cases with the fixed binary, except 2 streamed image + tool + thinking requests (both valid); memory, above.
  These requests take the same code as the base binary.
- Audio + tools on the native path (no audio chat model with a tool template on the laptop).
- GBNF with thinking on models that write their own `<think>` (unchanged, same as llama-server).
- One machine, debug builds, two chat models; at temperature 0 the 10 vision requests are near repeats.
- api / llama / runtime / cmd test suites not run (by instruction).
