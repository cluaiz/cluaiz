# Chat & Multimodal Completions API

`POST /v1/chat/completions`

Generates conversational completions, structured outputs, tool / function calling, and multimodal analysis (Images and Audio) with optional real-time token streaming.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/chat/completions`
* **Content-Type:** `application/json`
* **Streaming Protocol:** Server-Sent Events (SSE) via `text/event-stream` (when `stream: true`)

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Allowed Values / Range | Description |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **`messages`** | Array | **Yes** | — | Array of message objects | Full dialogue history. Content supports plain text or multimodal arrays. |
| **`model`** | String | No | `"auto"` | Model ID or `"auto"` | Model to use. `"auto"` uses the active model loaded in the chat slot. |
| **`stream`** | Boolean | No | `false` | `true`, `false` | Whether to stream tokens incrementally via Server-Sent Events (SSE). |
| **`temperature`** | Float | No | `0.7` | `0.0 - 2.0` | Sampling temperature. Lower values (0.0) force factual logic; higher values increase creativity. |
| **`max_tokens`** | Integer | No | `2048` | $\ge 1$ | Maximum number of completion tokens to generate. |
| **`think_mode`** | String / Integer | No | `"auto"` | `"auto"`, `"off"`, `"low"`, `"medium"`, `"high"`, or integer (e.g. `768`) | Controls Chain-of-Thought reasoning token emission and budget. |
| **`reasoning_effort`** | String | No | `"auto"` | `"minimal"`, `"low"`, `"medium"`, `"high"` | OpenAI-compatible alias for reasoning token budget. |
| **`response_length`** | String / Integer | No | `"auto"` | `"auto"`, `"short"`, `"medium"`, `"long"`, or integer (e.g. `200`) | Logit-level progressive EOS bias for graceful answer completion without mid-sentence chops. |
| **`tools`** | Array | No | `null` | Array of tool definitions | List of tools the model may call (Function Calling). |
| **`tool_choice`** | String / Object | No | `"auto"` | `"none"`, `"auto"`, `"required"` | Controls which tool the model must call. |
| **`top_p`** | Float | No | `0.95` | `0.0 - 1.0` | Nucleus sampling probability mass threshold. |
| **`top_k`** | Integer | No | `40` | $\ge 1$ | Top-k tokens to sample from at each step. |
| **`min_p`** | Float | No | `0.05` | `0.0 - 1.0` | Minimum probability threshold relative to the top token. |
| **`frequency_penalty`** | Float | No | `0.0` | $\ge 0.0$ | Penalizes new tokens based on their frequency in the text so far. |
| **`presence_penalty`** | Float | No | `0.0` | $\ge 0.0$ | Penalizes new tokens based on whether they appear in the text so far. |
| **`repetition_penalty`** | Float | No | `1.1` | $\ge 1.0$ | Penalizes repetitive phrases and token loops. |
| **`seed`** | Integer | No | `null` | Integer | Random seed for deterministic generation. |
| **`keep_alive`** | Integer | No | `null` | Seconds (e.g. `300`) | Inactivity timeout in seconds before unloading model from VRAM. |

> [!NOTE]
> **BitNet (1-bit / 1.58-bit) Models**:
> 1-bit and ternary quantized architectures operate natively with greedy argmax decoding for deterministic mathematical stability. For BitNet models, `temperature`, `top_p`, `top_k`, and `min_p` overrides are safely bypassed while penalty samplers remain active.

---

## 🔒 Security Notice: Local File Paths

> [!WARNING]
> When passing local file paths (e.g. `C:/...`), the server resolves files directly on the host filesystem. For local single-user development (`127.0.0.1`), this is convenient. If deploying in multi-user or network environments (`0.0.0.0`), ensure requests are authenticated or use Base64 data / HTTPS URLs.

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
"usage": {
  "prompt_tokens": 42,
  "completion_tokens": 380,
  "total_tokens": 422,
  "completion_tokens_details": {
    "reasoning_tokens": 128
  }
}
```

---

## 📏 Progressive EOS Logit Bias (`response_length`)

Unlike standard engines (such as Ollama's `num_predict` or crude `max_tokens` cuts) which slice generation abruptly mid-sentence, Cluaiz applies **progressive logit-level EOS bias** to answer tokens (`!in_think_block`):

| Level | Bias Trigger | Mechanism Description |
|:---|:---:|:---|
| **`"auto"`** | None | Natural completion based purely on model training. |
| **`"short"`** | Token 30 | Increments EOS logit score (`+0.15/token` up to `+5.0`) after token 30 to gently guide graceful termination. |
| **`"medium"`** | Token 150 | Soft EOS bias begins after token 150 for balanced standard responses. |
| **`"long"`** | Token 500 | Engages soft bias after token 500 for exhaustive answers. |
| **Custom Integer** (e.g. `200`) | $N - 25\%$ | Soft bias begins in the final 25% of target tokens, ensuring the model finishes smoothly near $N$. |

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

### 2. Audio Input (`input_audio` & `audio_url`)

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
        "url": "C:/Users/Aryan/Music/sample.wav"
      }
    }
  ]
}
```

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

data: [DONE]
```
