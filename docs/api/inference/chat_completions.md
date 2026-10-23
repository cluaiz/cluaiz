# Chat & Multimodal Completions API

`POST /v1/chat/completions`

Generates conversational completions, structured outputs, tool/function calling, and model-supported image/audio/video answers, with optional token streaming. Document attachments use shared extraction; PDFs use page images when the selected package declares image input and includes a projector.

## Select a model for the request

Read `GET /v1/models/installed` and use an exact `models[].id`. The API Explorer Model dropdown writes that value into the JSON `model`. Its choices follow declared tasks and the payload's media parts; the engine still validates the actual model/projector assets.

| Request content | Selected model / assets |
|:---|:---|
| Text string or `text` parts | Compatible chat/text-generation model |
| `image_url` | Vision-capable chat model with a matching vision projector |
| `audio_url` or `input_audio` | Audio-capable chat model with its matching audio projector; not a TTS model |
| `video_url`, `include_audio:false` | Vision model/projector; sampled frames become timestamped visual context |
| `video_url`, `include_audio:true` | Both visual and audio capabilities/assets |
| `file_url` | PDF pages go to the selected image-capable package with a projector; otherwise shared native extraction/OCR supplies text. DOC/DOCX/code/text use native text extraction. |

`model:"auto"` uses the configured active chat/vision slot. It does not promise to discover a model that supports every supplied attachment. Choose an explicit compatible ID when testing media. An ONNX image embedding model returns vectors through `/v1/embeddings`; it is not a vision chat model. Native `/v1/ingest` extracts text without selecting a model.

The content fragments later in this page belong inside `messages`. A complete request looks like this (replace the model ID with your installed vision package):

```json
{
  "model": "gemma-4-e4b-it-gguf-Q4_K_M",
  "messages": [{"role":"user","content":[
    {"type":"text","text":"Describe this image."},
    {"type":"image_url","image_url":{"url":"https://example.com/diagram.png"}}
  ]}],
  "stream": false,
  "max_tokens": 128,
  "keep_alive": 0
}
```

Use a URL serving a real image, an authorized server-local path, or a real data URI. A browser-local filename alone does not upload its bytes. Multiple parts keep their message order; do not nest an embeddings-style `{type:"image",source:...}` inside chat.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/chat/completions`
* **Content-Type:** `application/json`
* **Authentication:** `Authorization: Bearer <session-token>` when API authentication is required
* **Streaming Protocol:** Server-Sent Events (SSE) via `text/event-stream` (when `stream: true`)

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Allowed Values / Range | Description |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`messages`** | Array | **Yes** | — | Array of message objects | Full dialogue history. Content supports plain text or arrays (`text`, `image_url`, `audio_url`, `input_audio`, `file_url`, `video_url`). `file_url` uses native document text or rendered PDF pages for a selected image-capable package with projector. `video_url` uses packaged decoding to timestamped image frames; optional `video_url.include_audio` defaults to false and requires selected-model audio support. See the [media route reference](../README.md). |
| **`model`** | String | No | `"auto"` | Model ID or `"auto"` | Model to use. `"auto"` uses the active model loaded in the chat or vision slot. |
| **`stream`** | Boolean | No | `false` | `true`, `false` | Whether to stream tokens incrementally via Server-Sent Events (SSE). |
| **`return_progress`** | Boolean | No | `false` | `true`, `false` | With `stream:true`, adds separate `event: cluaiz.progress` observations starting before model/source preparation. Completion chunks remain unchanged. Ignored for non-streaming responses. |
| **`temperature`** | Float | No | `0.7` | `0.0 - 2.0` | Sampling temperature. Lower values (0.0) force factual logic; higher values increase variety. |
| **`max_tokens`** | Integer | No | until the model ends or the context is full (`2048` with attachments) | $\ge 1$ | Maximum number of completion tokens to generate (alias: `n_predict`). Also the room kept free for the answer when the history is fitted; see [Context window](#-context-window--context-shifting). |
| **`think_mode`** | String / Integer | No | `"auto"` | `"auto"`, `"off"`, `"on"`, or integer (e.g. `768`) | Controls Chain-of-Thought reasoning token emission and budget. |
| **`reasoning_effort`** | String | No | `"auto"` | `"low"`, `"medium"`, `"high"`, `"max"`, `"auto"` | OpenAI-compatible alias for reasoning token budget. |
| **`skip_reasoning`** | Boolean | No | `false` | `true`, `false` | Bypasses the reasoning monologue phase entirely for lower latency. |
| **`temporary_chat`** | String | No | `null` | `"lite"`, `"strict"` | Privacy isolation mode: `"lite"` (no vector write) or `"strict"` (brain and memory bypass). |
| **`session_id`** | String | No | `null` | String | Session identifier used to preserve conversation tool-call state across multi-turn sessions. |
| **`top_p`** | Float | No | `0.95` | `0.0 - 1.0` | Nucleus sampling probability mass threshold. |
| **`top_k`** | Integer | No | `40` | $\ge 1$ | Top-k tokens to sample from at each step. |
| **`min_p`** | Float | No | `0.05` | `0.0 - 1.0` | Minimum probability threshold relative to the top token. |
| **`repetition_penalty`** | Float | No | `1.1` | $\ge 1.0$ | Penalizes repetitive phrases and token loops (alias: `repeat_penalty`). |
| **`presence_penalty`** | Float | No | `0.0` | $\ge 0.0$ | Penalizes new tokens based on whether they appear in the text so far. |
| **`frequency_penalty`** | Float | No | `0.0` | $\ge 0.0$ | Penalizes new tokens based on their frequency in the text so far. |
| **`stop`** | Array | No | `null` | Array of strings | Custom token strings that halt generation immediately when matched. |
| **`seed`** | Integer | No | `null` | Integer | Random seed for deterministic, reproducible generation. |
| **`keep_alive`** | Integer | No | `null` | Minutes (e.g. `5`, `0`, `-1`) | Positive minutes retain after completion; `0` unloads, omission/`-1` retains. This value is not seconds. |
| **`response_format`** | Object | No | `null` | Object | Enforces structured JSON output schema (e.g. `{"type": "json_object"}`). |
| **`tools`** | Array | No | `null` | Array of tool definitions | List of tools the model may call (Function Calling) via `<tool_call>` XML. |
| **`tool_choice`** | String / Object | No | `"auto"` | `"none"`, `"auto"` | `"none"` gives the model no tools (installed, loaded or suggested) for this request; any other value lets the model decide. |
| **`tool_search`** | Boolean | No | `true` | `true`, `false` | Suggest installed, switched-on tools that fit the message and let the model load them on demand. `false` leaves suggestions out. See [Tools on demand](#-tools-on-demand). |
| **`grammar`** | String | No | `null` | BNF / GBNF string | Formal BNF/GBNF grammar string for strict grammar-guided output generation. |
| **`chat_template_kwargs`** | Object | No | `null` | Object | Additional keyword arguments passed directly to the Jinja chat tokenizer template. |
| **`logit_bias`** | Object | No | `null` | Map of token ID to bias | Token ID probability bias adjustments (-100 to +100). |

> [!NOTE]
> **BitNet (1-bit / 1.58-bit) Models**:
> 1-bit and ternary quantized architectures operate natively with greedy argmax decoding for deterministic mathematical stability. For BitNet models, `temperature`, `top_p`, `top_k`, and `min_p` overrides are safely bypassed while penalty samplers remain active.

---

## Attachment processing events

Add `"return_progress":true` to the existing streaming chat request. Authorization remains required. Existing clients that omit this option receive their previous SSE contract.

Chat attachment preparation is bounded to one request per engine host. While another chat attachment request still owns preparation/generation buffers, a new attachment request returns `error.type:"resource_error"`, `error.code:"media_busy"` before model loading/source decoding. Retry after the current request finishes; a cancellation may still be stopping its native worker. Non-streaming requests return HTTP 503; streaming requests use the existing SSE error followed by `[DONE]` (and terminal progress when opted in). Plain text requests bypass this admission. This bound does not certify free RAM, accelerate image encoding or coordinate separate ONNX tasks.

```text
event: cluaiz.progress
data: {"id":"<same chat completion ID>","object":"cluaiz.processing","processing":{"stage":"encoding","state":"running","elapsed_ms":1234,"attachment":{"message_index":0,"part_index":1}}}
```

The numbers above illustrate the schema; they are not a benchmark. `elapsed_ms` is measured since request observation began, including preflight. `attachment` points to the original uploaded content part with zero-based indices, even when an internal system instruction is inserted. A sampled video image adds `frame_index` for that part's decoded frame. Paths, filenames and file contents are not included in progress events.

| Field | Contract |
|---|---|
| `stage` | Observed `queued`, `model_loading`, `reading`, `decoding`, `projector_loading`, `encoding`, `prefill`, `generation`, or terminal `request`. |
| `state` | `running`, `completed`, `reused`, `failed`, or `cancelled`. `projector_loading/reused` means the existing encoder owner was retained. |
| `work` | Optional `{ "completed":N, "total":M, "unit":"tokens" }` from successful native prefill chunks/batches, or `unit:"slices"` from completed native encoding slices for that attachment. These are different operations and must not be combined into a whole-request percentage. |
| Missing `work` | The operation exposes a stage boundary without a reliable fraction. Show an indeterminate animation and elapsed time. No encoder graph percentage or finish-time estimate is fabricated. |

An attachment may produce multiple native slices. Its encoding stage completes only after its final slice, and prefill uses the pinned native helper's own positions, batching and attention behavior. Generation follows successful full prefill. Read/decode observations apply to shared image/audio/document/video preparation; native encoding/prefill observations are wired to GGUF chat. ONNX inference SSE still has its existing task contract and has not gained these events.

The transport uses a bounded observation queue. Slow readers may miss intermediate stages; inference never waits for this queue. The terminal `request` status is retained separately and sent once before `[DONE]`. Normal answer deltas, error objects, reasoning, tool continuations and cancellation remain on their existing paths. HTTP keep-alive comments indicate connection liveness, not processing progress. Dropping the stream signals cancellation; interruption is observed at operation boundaries, with no hard real-time abort guarantee for a long native encode/load.

Early `usage.context_telemetry` remains a text estimate. For attachment requests, `context_telemetry_scope` identifies that it excludes media. Use native prefill `work` for actual processed input-token counts; neither text percentages nor prefill counts represent remaining RAM or upload capacity. The first SSE event is not the first generated answer token.

After projector loading, the incremental request-RAM check returns an error containing `required_bytes`, `workspace_bytes`, `additional_buffer_bytes`, `free_bytes`, `reserve_bytes` and `usable_bytes`. Cold projector admission separately reports its estimated host cost and usable RAM budget. These checks use the existing safety reserve and prepared geometry/copy estimates. Configured byte ceilings are limits, not automatic per-request allocation charges. Workspace is still a conservative configured estimate rather than a measured encoder peak. Under RAM pressure an idle projector can be dropped before a fresh admission attempt; Chat model placement, context, KV settings and SSD policy are unchanged.

---

## 🔒 Security Notice: Local File Paths

> [!WARNING]
> Local paths refer to the engine host and must pass filesystem policy. Bearer authentication and source authorization are separate checks; `x-caller: user` does not grant unrestricted access. Allowed remote sources and inline bytes use shared source/decoded limits. Always use the actual configured engine address and authentication settings.

---

## 🧠 Reasoning Budget Levels (`think_mode` & `reasoning_effort`)

| Mode / Effort | Budget Limit | Behavior Description |
|:---|:---:|:---|
| **`"auto"`** | None | Model generates reasoning with its default prompt template without budget cutoff. |
| **`"off"` / `"minimal"`** | `0` | **Think Tag Prefill:** Thinking tags prefilled (`<think></think>`). 0 reasoning tokens emitted. |
| **`"low"`** | `512` | Concise reasoning capped at 512 thinking tokens. |
| **`"medium"`** | `1024` | Balanced analytical reasoning capped at 1024 thinking tokens. |
| **`"high"`** | `Bounded by n_ctx` | Full reasoning bounded only by `max_tokens` and model context window (`n_ctx`). |
| **Custom Integer** (e.g. `256`, `768`, `2048`) | Exact Count | Clamped to `n_ctx` and `max_tokens.saturating_sub(32)` to guarantee answer tokens. |

### 📊 Token Usage Telemetry
Responses include explicit reasoning token counts inside `completion_tokens_details`:
```json
{
"usage": {
  "prompt_tokens": 42,
  "completion_tokens": 380,
  "total_tokens": 422,
  "completion_tokens_details": {
    "reasoning_tokens": 128
  }
}
}
```

---

## 📏 Context Window & Context Shifting

The engine fits every request into the loaded model's context (`n_ctx`), counting with the model's own tokenizer. The `context_shifting` setting (`settings.json` → `optimization`) decides what happens when a conversation outgrows it:

| Setting | Before the request (history too long) | While answering (context full) |
| :--- | :--- | :--- |
| **`Off`** | Error `exceed_context_size_error` (HTTP 400): *request (N tokens) exceeds the available context size (M tokens)* — llama-server's default. | Stops; `finish_reason: "length"`. |
| **`Auto`** (default) | Drops the oldest whole turns until the prompt plus the answer's room fit. Only the overflow when the model's KV cache can shift, else a quarter of the context so the next turns reuse the cache. | Stops with `"length"`, unless the request asked for more `max_tokens` than the room left: then llama.cpp's context shift (half of the old tokens after BOS) and the answer continues. |
| **`Minimal` / `Standard` / `Aggressive` / `Extreme`** | Same, freeing at least 5 / 10 / 25 / 50% of the context each time. | Same; each shift frees that share. |

Rules that hold in every mode:
- The system message and the newest message are never dropped. A user turn goes with its answer, and a tool result with its call.
- The answer's room is `max_tokens` (at most a quarter of the context, 256–2048 tokens), doubled for a model that thinks first.
- The KV cache is reused: the common prefix, then — as llama-server's `--cache-reuse 256` — shared chunks of at least 256 tokens after it, such as the history left once old turns were dropped. Models whose memory cannot shift (sliding-window or recurrent caches, multimodal prompts) skip the chunk reuse and context shift, as llama-server does; a sliding-window cache that no longer holds the positions a prefix needs is processed again.
- If the newest message alone does not fit, the request fails with `exceed_context_size_error` in every mode.

What happened is reported in the closing `engine.context` object (and `usage.prompt_tokens` uses its count):

```json
{
"context": {
  "n_ctx": 6144,
  "prompt_tokens": 4120,
  "cache_n": 3980,
  "dropped_messages": 2,
  "dropped_tokens": 2096,
  "shifted_tokens": 0,
  "n_tokens": 5302,
  "truncated": true,
  "stop": "eos"
}
}
```

`cache_n` = prompt tokens reused from the KV cache; `truncated` = the context cut the history or the answer (llama-server's field); `stop` = `eos`, `limit` (max_tokens or the context; `finish_reason: "length"`), `cancel` or `error`.

---

## 💾 Chat Memory (saved KV cache per chat)

Send a `session_id` with every request of a chat. After each answer the engine saves that chat's KV cache to disk and restores it when the chat continues after another chat, a model reload or a restart, so only the new tokens are processed (`engine.context.cache_n`). Private requests (`temporary_chat`) are never saved. Settings, endpoints and measurements: [Chat Memory API](./chat_memory.md).

---

## 🖼️ Multimodal Vision & Audio Guide

### 1. Vision Input (`image_url`)

Supports JPEG, PNG, WEBP, GIF, and BMP:

```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "Describe this image." },
    {
      "type": "image_url",
      "image_url": {
        "url": "https://example.com/diagram.png"
      }
    }
  ]
}
```

### 2. Document Input (`file_url`)

Direct document question answering and summarization. The same `file_url` payload supports native document text and selected-model PDF vision. A selected package declaring image support with a projector receives ordered rendered PDF pages, including their graphics. Text-only packages receive native extracted/OCR text. This route does not invoke the ingest HTTP API or chat tools to read the attachment.

> [!TIP]
> **Native text fallback:** Text-only LLMs can read extracted PDF/Word text without a vision projector. Native OCR recognizes words, not the meaning of diagrams. To ask about PDF graphics, explicitly select a complete compatible vision package. Visual pages share the configured decoded-byte and model-context limits; oversized PDFs fail instead of silently dropping pages. DOC/DOCX text extraction does not automatically expose embedded images.

#### 📋 Supported Document & Code Formats
| Category | Supported Extensions | Processing Mechanism |
|:---|:---|:---|
| **Documents** | `.pdf`, `.docx`, `.doc`, `.txt`, `.md`, `.csv`, `.json`, `.toml`, `.yaml`, `.yml` | Native extraction into text; PDFium/OCR for PDFs, OOXML for DOCX and the supported compound Word parser for DOC. Unsupported content is reported as omissions/errors. |
| **Source Code** | `.rs`, `.ts`, `.tsx`, `.js`, `.jsx`, `.py`, `.c`, `.cpp`, `.h`, `.go`, `.html`, `.css`, `.sql`, `.sh`, `.log` | Plain text tokenizer ingestion into context window |

> [!WARNING]
> **Archives & Executables Not Supported:** Compressed archives (`.zip`, `.tar`, `.gz`, `.rar`, `.7z`) and binary executables (`.exe`, `.bin`, `.dll`) cannot be processed as chat context attachments and are filtered out by the client.

```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "Summarize key findings from this document." },
    {
      "type": "file_url",
      "file_url": {
        "url": "data:application/pdf;base64,<actual-pdf-bytes>"
      }
    }
  ]
}
```

### 3. Sampled-Video Input (`video_url`)

Decodes sampled RGB frames at native timestamps relative to the first decoded frame, routing through the vision projector.

```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "Describe what happens across this video clip." },
    {
      "type": "video_url",
      "video_url": {
        "url": "C:/media/clip.mp4",
        "include_audio": false
      }
    }
  ]
}
```

### 4. Audio Input (`input_audio` & `audio_url`)

* **OpenAI Standard (`input_audio`):**

```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "Transcribe and summarize this audio." },
    {
      "type": "input_audio",
      "input_audio": {
        "data": "UklGRiQAAABXQVZFZm10IBAAAAABAAEA...",
        "format": "wav"
      }
    }
  ]
}
```

* **Cluaiz URL Extension (`audio_url`):**

```json
{
  "role": "user",
  "content": [
    { "type": "text", "text": "Analyze this audio file." },
    {
      "type": "audio_url",
      "audio_url": {
        "url": "C:/media/sample.wav"
      }
    }
  ]
}
```

---

## ⚠️ Request Size Limit & Payload Ceiling

* **Axum Request Ceiling:** The default Axum server request ceiling is **2 MiB**.
* Inline Base64 Data URIs (`data:...;base64,...`) expand binary sizes by ~33%. Ensure the total JSON request body remains below 2 MiB, or pass authorized local file paths (e.g. on Desktop/Tauri) to bypass inline byte limits.


---

## 🛠️ Tool & Function Calling Example

```json
{
  "model": "auto",
  "messages": [
    { "role": "user", "content": "What is the weather in Tokyo?" }
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_current_weather",
        "description": "Get current weather for a given location",
        "parameters": {
          "type": "object",
          "properties": {
            "location": { "type": "string", "description": "City name, e.g. Tokyo" },
            "unit": { "type": "string", "enum": ["celsius", "fahrenheit"] }
          },
          "required": ["location"]
        }
      }
    }
  ]
}
```

---

## 🔎 Tools on demand

Tools switched off in the App's Tools page are never offered and cannot run. Every other installed skill, plugin and MCP server can be used without binding it to the chat first, and none of them is put into the prompt in full unless the model asks for it:

1. **Loaded tools.** Tools bound to the chat (`/v1/chat/{session_id}/tools`, `tools`, or loaded earlier with `load_tool`) go into the system prompt with their whole schemas and instructions. A tool that does not fit is left out whole; nothing is cut in the middle.
2. **Suggestions.** The engine matches the user's message against the other tools on whole words (never parts of words, so "trust" does not match "rust"), including common Hinglish and Devanagari words ("padho" → read, "chalao" → run). A tool with clear evidence (its name, a trigger phrase, or two separate matches on its name, function names or trigger words) is shown as a short card with its load cost; weaker matches as a one-line title. Suggestions go into the current user message inside `<tools_hint>`, not into the system prompt. An ordinary message ("Hi") gets no tool text at all.
3. **Meta-tools.** With suggestions the model gets three functions, answered by the engine itself without an approval prompt:
   - `load_tool` (`{"id": "filesystem"}`, optional `"functions": [...]`): returns the tool's instructions and function schemas, whole, within the room left in the context. A tool that does not fit is refused with the reason, never cut. The tool's MCP server is started only now.
   - `unload_tool` (`{"id": "..."}`): removes the tool from the chat. Its earlier load result in the same answer is replaced by a short note, and it is not suggested again for 3 answers.
   - `find_tools` (`{"query": "read a file"}`): searches the other installed tools and returns cards. `tool_search` is still accepted as its old name.
4. **Calling.** Calling a loaded tool's functions asks for approval under the agent security mode, in streaming and non-streaming requests alike: a non-streaming request waits until the call is approved or rejected through `/v1/system/permission/approve|reject` (it is listed under `/v1/system/permission/pending`) or the 120-second timeout passes. A tool called by its id before it was loaded is loaded instead, so the model can call the right function next.

In a chat with `session_id`, a loaded tool stays bound for the tool's default number of turns (3 when the manifest sets none), so later messages have it in the prompt. The stream reports meta-tool calls like any tool call (`tool_calls`, then `tool_result`, with category `tools`). Send `tool_search: false` to turn suggestions off, or `tool_choice: "none"` for no tools at all.

---

## 💻 Code Examples

### 1. Python (Official OpenAI SDK)

```python
from openai import OpenAI

client = OpenAI(base_url="http://localhost:8000/v1", api_key="not-needed")

response = client.chat.completions.create(
    model="auto",
    messages=[
        {"role": "system", "content": "You are a concise assistant."},
        {"role": "user", "content": "Explain gravity in 2 sentences."}
    ],
    temperature=0.7
)

print(response.choices[0].message.content)
```

---

## 📤 Response Formats

### Streaming SSE Chunks with `reasoning_content`

When reasoning / think mode is active, thoughts are streamed in `reasoning_content`:

```text
data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1724391200,"model":"...","choices":[{"index":0,"delta":{"reasoning_content":"Let's consider the principles..."},"finish_reason":null}]}

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1724391200,"model":"...","choices":[{"index":0,"delta":{"content":"Gravity is the fundamental force..."},"finish_reason":null}]}

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","created":1724391200,"model":"...","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
```
