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
|:---|:---|:---|:---|:---|:---|
| **`messages`** | Array | **Yes** | — | Array of message objects | Full dialogue history. Content supports plain text or multimodal arrays. |
| **`model`** | String | No | `"auto"` | Model ID or `"auto"` | Model to use. `"auto"` uses the active model loaded in the chat slot. |
| **`stream`** | Boolean | No | `false` | `true`, `false` | Whether to stream tokens incrementally via Server-Sent Events (SSE). |
| **`temperature`** | Float | No | `0.7` | `0.0 - 2.0` | Sampling temperature. Lower values (0.0) force factual logic; higher values increase creativity. |
| **`max_tokens`** | Integer | No | `4096` | $\ge 1$ | Maximum number of completion tokens to generate. |
| **`think_mode`** | String | No | `"auto"` | `"auto"`, `"on"`, `"off"`, `"low"`, `"medium"`, `"high"` | Controls Chain-of-Thought reasoning token emission. |
| **`tools`** | Array | No | `null` | Array of tool definitions | List of tools the model may call (Function Calling). |
| **`tool_choice`** | String / Object | No | `"auto"` | `"none"`, `"auto"`, `"required"` | Controls which tool the model must call. |
| **`top_p`** | Float | No | `0.95` | `0.0 - 1.0` | Nucleus sampling probability mass threshold. |
| **`top_k`** | Integer | No | `40` | $\ge 1$ | Top-k tokens to sample from at each step. |
| **`min_p`** | Float | No | `0.05` | `0.0 - 1.0` | Minimum probability threshold relative to the top token. |
| **`repetition_penalty`** | Float | No | `1.1` | $\ge 1.0$ | Penalizes repetitive phrases and token loops. |
| **`keep_alive`** | Integer | No | `null` | Seconds (e.g. `300`) | Inactivity timeout in seconds before unloading model from VRAM. |

---

## 🔒 Security Notice: Local File Paths

> [!WARNING]
> When passing local file paths (e.g. `C:/...`), the server resolves files directly on the host filesystem. For local single-user development (`127.0.0.1`), this is convenient. If deploying in multi-user or network environments (`0.0.0.0`), ensure requests are authenticated or use Base64 data / HTTPS URLs.

---

## 🎨 UI Preset Mapping

In the Developer Hub UI, 4 quick preset buttons are provided as client shortcuts:

| Preset Button | `think_mode` | `temperature` | Injected System Prompt |
|:---|:---|:---|:---|
| **Think Deep** | `"on"` | `0.0` | *"Analyze the request deeply step-by-step. Provide a highly detailed, comprehensive response."* |
| **Think Lite** | `"on"` | `0.5` | *"Think carefully but provide a balanced, concise response."* |
| **Long Answer** | `"off"` | `0.7` | *"Provide a detailed, thorough, and to-the-point answer."* |
| **Short Answer** | `"off"` | `0.7` | *"Provide a very concise, direct, and to-the-point answer."* |

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
