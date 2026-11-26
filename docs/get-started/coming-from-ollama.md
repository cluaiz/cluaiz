<!-- name: Coming from Ollama
description: What carries over from Ollama (GGUF models, command names, API routes) and the Cluaiz command for each common task. -->

# Coming from Ollama

If you use Ollama today, most of what you know carries over: the same kind of model files (GGUF,
run by llama.cpp), the same command names, and Ollama's own chat, generate, embed and model routes,
so the Ollama libraries work by pointing them at the Engine. This page lists what is the same, what
is different, and what to change. The API behaviour was checked with the official Ollama Python
library in [run 36](../../test/benchmarks/api-compatibility.md#run-36) (library 0.6.1,
Engine 0.1.0, Windows, Qwen3-0.6B).

![Ollama commands and routes next to their Cluaiz equivalents: the commands map one to one; /api/chat, /api/generate, /api/embed, /api/tags, /api/pull and /api/version answer in Ollama's shape; /api/ps and /api/show are not there; a Modelfile becomes settings.json and request fields](../assets/get-started/from-ollama.png)

## Commands

| Ollama | Cluaiz | Note |
|---|---|---|
| `ollama pull <model>` | `cluaiz pull <model>` | a Cluaiz catalogue id (`qwen3:8b`) or a Hugging Face repository (`unsloth/Qwen3.5-4B-GGUF:Q4_K_M`) |
| `ollama run <model>` | `cluaiz run <model>` | `cluaiz run <model> "prompt"` answers once |
| `ollama list` | `cluaiz list` | the first column is the installed ID; use it as `model` in API calls |
| `ollama show <model>` | `cluaiz show <model>` | also prints the plan this machine would get: GPU layers, context, KV cache type |
| `ollama ps` | `cluaiz ps` | the model the running server has loaded |
| `ollama rm <model>` | `cluaiz rm <model>` | |
| `ollama serve` | `cluaiz serve` | port 8000, not 11434 |
| `ollama create` with a Modelfile | no equivalent | see [Modelfile settings](#modelfile-settings) |

Cluaiz adds commands Ollama does not have, for example `cluaiz agent` (a coding agent in the
current folder), `cluaiz status` (the hardware the planner sees) and `cluaiz bench`. The full list:
[Commands reference](../cmd/commands_reference.md).

## Models

Cluaiz does not read Ollama's model store. Download the model again with `cluaiz pull`; the files go
to `~/.cluaiz/models` (or `<CLUAIZ_HOME>/models`). A stopped download continues where it stopped.

The installed ID is not the repository name: `cluaiz list` prints it, for example
`qwen3-0.6b-gguf-Q4_K_M` (the model of run 33). Use that ID as `model` in API calls. More: [Model
downloading](model-downloading.md).

## Server address

| Ollama | Cluaiz |
|---|---|
| `http://localhost:11434` | `http://localhost:8000` |
| `OLLAMA_HOST` | `CLUAIZ_HOST` and `CLUAIZ_PORT` for one run; `cluaiz permission port <n>` to save a port |
| no key by default | no key by default; with `api_auth.required` on, send `Authorization: Bearer <token>` ([permission.json](../engine/permission.md)) |

## API: what to change in your code

**With the Ollama library:** only the address and, when keys are on, the token.

```python
import ollama

client = ollama.Client(host="http://localhost:8000",
                       headers={"Authorization": "Bearer sk-cluaiz-..."})  # only when keys are on
reply = client.chat(model="qwen3-0.6b-gguf-Q4_K_M",  # the ID from `cluaiz list`
                    messages=[{"role": "user", "content": "Hello"}])
print(reply.message.content)
```

What run 36 checked with that library: `chat` (one reply, and streaming, which the library reads line
by line), `think=True` (the thinking arrives in `message.thinking`), `format="json"`, `generate` with a
system text, `list`, and `/api/version`. `done_reason`, `eval_count` and `prompt_eval_count` are filled
from the Engine's own counts.

| Ollama route | In Cluaiz |
|---|---|
| `POST /api/chat` | Ollama's request and reply; streams by default, one JSON object per line (run 36). As in Ollama, only the messages and the request's `tools` go to the model: no Engine functions, no workspace agent files, no notes (run 62) |
| `POST /api/generate` | the prompt (and `system`) as one chat turn in the model's template; the answer in `response` (run 36). As in Ollama, nothing else goes to the model: no tools (a `tools` field is ignored), no workspace agent files, no notes (run 57). `raw`, `suffix`, `template` and `context` are not read |
| `POST /api/embed` | `{"model", "input"}` to `{"model", "embeddings"}` through `/v1/embeddings`; needs an embedding model ([Embeddings](../api/inference/embeddings.md)) |
| `GET /api/tags` | the installed models; `modified_at` is the model file's change time (run 35) |
| `POST /api/show` | an installed model's details, GGUF header (`model_info`) and chat template, without loading it; no tensor list and no `capabilities` yet ([Show a model](../api/models/show_model.md)) |
| `POST /api/pull` | Ollama's request ([Pull a model](../api/models/pull_model.md)) |
| `GET /api/version` | this Engine's version (`0.1.0`), not an Ollama version |
| `GET /api/ps` | the chat model the server has loaded; `size_vram` is the VRAM its plan was given (planned, not read from the driver), `context_length` the planned context (run 37) |
| `/api/embeddings` (the old route), `/api/create`, `/api/copy`, `/api/delete`, `/api/push` | not there (HTTP 404). Use `cluaiz rm` or CEP `models.remove` |

**With an OpenAI library:** send OpenAI requests to `POST /v1/chat/completions` with the base URL
`http://localhost:8000/v1` ([OpenAI compatibility](../api/inference/openai_compatibility.md)).

**Request fields** on the Ollama routes:

| Ollama field | In Cluaiz |
|---|---|
| `options.temperature`, `top_p`, `top_k`, `min_p`, `seed`, `presence_penalty`, `frequency_penalty`, `repeat_penalty`, `stop` | used, under the Engine's names |
| `options.num_predict` | the answer limit (`-1` = no limit) |
| `options.num_ctx` | the context size of this load (`llama.n_ctx`); without it the planner chooses from free memory |
| other `options` (for example `num_gpu`, `num_thread`, `mirostat`) | ignored; load settings are in `settings.json` or the `llama` object of `/v1/chat/completions` |
| `think`: `true`, `false` or a level | thinking on or off; a level (`low`, `medium`, `high`) sets the thinking budget |
| `format`: `"json"` or a JSON schema | the answer is held to it by a grammar, also together with `tools` (one grammar for the answer and the calls) |
| `keep_alive` | as in Ollama: a number is seconds, a string a duration (`"5m"`). On the OpenAI routes a number means minutes |
| `images` (base64) | sent to a vision model as images |
| `tools`, `tool_calls` | passed through; the model's calls come back in `message.tool_calls` |
| `raw`, `suffix`, `context` (generate) | ignored |

Not tested in run 36: `num_ctx`, `images`, `tools` and `/api/embed` with an embedding model (the run
had only a chat model, and `/api/embed` refused it with HTTP 400, as `/v1/embeddings` does).

## Modelfile settings

There is no Modelfile. What it sets goes elsewhere:

| Modelfile line | In Cluaiz |
|---|---|
| `FROM` | `cluaiz pull` |
| `PARAMETER temperature`, `top_p`, ... | `settings.json` → `samplers` (default for every chat), or the request fields |
| `PARAMETER num_ctx`, `num_gpu` | `settings.json` → `optimization` (`custom_context_window`, `n_gpu_layers`), or the request's `llama` object; unset means the planner decides |
| `SYSTEM` | a `system` message in the request |
| `TEMPLATE` | the chat template inside the GGUF file is used |

Settings reference: [Engine configuration](../reference/engine-configuration.md) and [Load and
speed settings](../engine/llm_optimization.md).

## What works differently

- **Placement.** Before a load, the Engine measures free RAM and VRAM and chooses GPU layers,
  context size and KV cache type for this machine ([how](../engine/llm_optimization.md)). `cluaiz
  show <model>` prints that plan without loading.
- **One chat model at a time** is loaded for chat. Embedding and speech models load beside it when
  memory allows; requests to one model wait in line.
- **Beyond chat:** the same server runs speech, transcription, embeddings, document reading, tools,
  memory and an agent loop ([API overview](../api/README.md)).

## Not available yet

- `/api/show` and the model-management routes (`create`, `copy`, `delete`, `push`).
- Importing models from Ollama's store.
- A Modelfile or saved model variants.
- The Ollama JavaScript library was not tested; it reads the same replies as the Python library.
