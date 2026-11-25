# Models & Engine Vault

<!-- Generated from the CEP registry by api/tests/cep_docs.rs; do not edit. -->

| Method | Scope | Runs as | Summary |
|---|---|---|---|
| [`models.discard`](./models/models.discard.md) | write | sync | Deletes what a stopped models.download left (its `.part` and finished files), so the next download starts over; an installed model is never touched |
| [`models.download`](./models/models.download.md) | write | task | Downloads and registers a model (catalog id, or Hugging Face owner/name[:QUANT]) as `cluaiz pull` does; the `:QUANT` variant, else the first |
| [`models.embedding_room`](./models/models.embedding_room.md) | read | sync | Where each embedding model would run beside the chat model (GPU, CPU or no room) on today's free memory, and the one to recommend |
| [`models.inspect`](./models/models.inspect.md) | read | sync | Raw GGUF metadata and tensor table of an installed model |
| [`models.list`](./models/models.list.md) | read | sync | Installed registry entries, optionally filtered by category |
| [`models.load`](./models/models.load.md) | write | task | Loads an installed GGUF model using the existing Engine host. A task: its progress is the share of the weights read, its events the load's steps (started with the plan, a llama.cpp fallback, context, warmup, a retry in a smaller placement, at most 3 loads) |
| [`models.load_history`](./models/models.load_history.md) | read | sync | This machine's chat-model loads, refusals, failures and unloads, oldest first: placement, VRAM planned and taken, attempts, time (the newest 500 are kept) |
| [`models.remove`](./models/models.remove.md) | write | sync | Unloads the selected model if active and removes its installed package |
| [`models.unload`](./models/models.unload.md) | write | sync | Unloads a loaded model now (`model`), or every loaded model (`all: true`): the chat model as `keep_alive: 0` does, an embedding, speech or ONNX model when no request is using it (`busy` otherwise). A model that is not loaded is no error |
| [`providers.list`](./providers/providers.list.md) | admin | sync | The model providers and their listed models; keys are never returned |
| [`providers.models`](./providers/providers.models.md) | admin | sync | The models a provider offers (its GET /models), with the context size it reports |
| [`providers.remove`](./providers/providers.remove.md) | admin | sync | Removes a provider, with its key if it was kept in the file |
| [`providers.set`](./providers/providers.set.md) | admin | sync | Adds a provider, or changes its URL, key source and listed models |
| [`providers.test`](./providers/providers.test.md) | admin | sync | Sends one short request of the model's kind: the URL, the key and the model work |

All categories: [CEP reference](../README.md). Folder: `docs/api/cep/models/`.
