# Speech-to-Text (STT) Transcriptions API (`/v1/audio/transcriptions`)

`POST /v1/audio/transcriptions` (alias: `POST /v1/audio/execute` with task `speech_to_text`)

Transcribes spoken audio, voice notes, and audio clips into clean text using ONNX-accelerated Whisper acoustic neural models. Supports multi-threaded Mel-spectrogram processing, automatic language detection, and Server-Sent Events (SSE) streaming tokens.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/audio/transcriptions` (or `/v1/audio/execute`)
* **Content-Type:** `application/json` (Pure JSON contract — `multipart/form-data` is **not** supported)
* **Authentication:** `Authorization: Bearer <session-token>` (Optional header: `x-caller: user`)
* **Streaming Protocol:** Server-Sent Events (SSE) via `text/event-stream` when `stream: true`

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Allowed Values / Formats | Description |
|:---|:---|:---:|:---|:---|:---|
| **`input_source`** | Object | No* | — | `{"type": "file" \| "audio" \| "base64" \| "url", "data": "..."}` | Audio input descriptor. For browser/client uploads, provide standard Base64 Data URI (`data:audio/wav;base64,...`). Desktop clients can supply an authorized local filesystem path. (*Required if `file` is omitted). |
| **`file`** | String | No* | — | Local file path or URI | Direct file path shortcut (OpenAI-compatible alternative to `input_source`). |
| **`model`** | String | No | `"whisper-large-v3-turbo-INT8"` | Installed STT model ID | Model identifier. Defaults to active or installed Whisper package (`whisper-large-v3-turbo-INT8`). |
| **`language`** | String | No | `"auto"` | ISO-639-1 code (e.g. `"en"`, `"hi"`, `"es"`, `"fr"`, `"auto"`) | Target spoken language. Setting `"auto"` runs Whisper language identification. |
| **`languages`** | String[] | No | `[]` | Up to 10 ISO-639-1 codes (e.g. `["hi", "en"]`) | Cluaiz extension: the languages the speaker uses. Without `language`, each request is identified among only these, so a speaker who switches between Hindi and English sentences gets each in its own language and never an unrelated one (Hindi written in Urdu script, a stray Vietnamese line). One code works like `language`. Empty: all languages the model knows. Like Google STT `alternativeLanguageCodes` and Azure candidate languages. |
| **`temperature`** | Float | No | `0.0` | `0.0 - 1.0` | Sampling temperature for greedy decoding. `0.0` is recommended for maximum transcription accuracy. |
| **`timestamps`** | Boolean | No | `false` | `true`, `false` | Whether to return word/segment timestamps in output (default: `false` for raw text). |
| **`stream`** | Boolean | No | `false` | `true`, `false` | When `true`, tokens are emitted incrementally via SSE. When `false`, returns single complete JSON object. |
| **`keep_alive`** | Integer | No | `null` | Minutes (e.g. `5`, `0` to unload, `-1` forever) | Session cache lifetime in memory before automatic session cleanup. |

---

### Which languages a model can transcribe

`GET /v1/audio/languages?model=<id>` (model optional: the active or first installed STT model) lists the codes accepted by `language` and `languages`:

```json
{ "object": "list", "model": "whisper-large-v3-turbo-FP16", "data": ["af", "am", "ar", "...", "hi", "...", "zh"] }
```

A model that reads one language only (Moonshine) returns an empty list.

---

## 🎵 Supported Audio Formats

* **WAV (`audio/wav`)**: Recommended for zero-decode latency (PCM 16-bit 16kHz mono).
* **MP3 (`audio/mp3`, `audio/mpeg`)**: Supported via built-in audio decoders.
* **OGG / Opus (`audio/ogg`, `audio/opus`)**: Supported.
* **FLAC (`audio/flac`)**: Supported.

> [!IMPORTANT]
> **No Multipart Uploads:** Unlike OpenAI's standard multipart endpoint, Cluaiz Engine uses a unified, secure **pure JSON contract**. Audio files must be encoded as Base64 Data URIs (`data:audio/wav;base64,...`) or passed as local authorized file paths.

---

## 💻 Code Examples

### 1. cURL (Base64 WAV Data URI - Non-streaming)

```bash
curl -X POST http://localhost:8000/v1/audio/transcriptions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <session-token>" \
  -d '{
    "model": "whisper-large-v3-turbo-INT8",
    "input_source": {
      "type": "audio",
      "data": "data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEARKwAAIhYAQACABAAZGF0YQAAAAA="
    },
    "language": "auto",
    "temperature": 0.0,
    "timestamps": false,
    "stream": false,
    "keep_alive": 5
  }'
```

### 2. cURL (Authorized Local File Path / Direct File Field)

```bash
curl -X POST http://localhost:8000/v1/audio/transcriptions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <session-token>" \
  -H "x-caller: user" \
  -d '{
    "model": "whisper-large-v3-turbo-INT8",
    "file": "C:/Users/Aryan/Downloads/harvard.mp3",
    "language": "en",
    "temperature": 0.0,
    "stream": false,
    "keep_alive": 5
  }'
```

### 3. Python (Microphone Recording / File Upload)

```python
import requests
import base64

# Read audio file and create Data URI
with open("voice_memo.wav", "rb") as f:
    audio_bytes = f.read()
audio_uri = "data:audio/wav;base64," + base64.b64encode(audio_bytes).decode("utf-8")

headers = {
    "Content-Type": "application/json",
    "Authorization": "Bearer <session-token>"
}

payload = {
    "model": "whisper-large-v3-turbo-INT8",
    "input_source": {
        "type": "audio",
        "data": audio_uri
    },
    "language": "auto",
    "temperature": 0.0,
    "timestamps": False,
    "stream": False,
    "keep_alive": 5
}

response = requests.post("http://localhost:8000/v1/audio/transcriptions", json=payload, headers=headers)
data = response.json()

if data.get("status") == "success":
    transcript = data["output"]["text"]
    print("Transcript:", transcript)
else:
    print("Error:", data.get("error"))
```

### 4. JavaScript / TypeScript (Web Audio API / Tauri)

```typescript
async function transcribeBlob(blob: Blob, token: string): Promise<string> {
  const reader = new FileReader();
  const base64DataUri = await new Promise<string>((resolve) => {
    reader.onloadend = () => resolve(reader.result as string);
    reader.readAsDataURL(blob);
  });

  const res = await fetch("http://localhost:8000/v1/audio/transcriptions", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": `Bearer ${token}`
    },
    body: JSON.stringify({
      model: "whisper-large-v3-turbo-INT8",
      input_source: {
        type: "audio",
        data: base64DataUri
      },
      language: "auto",
      temperature: 0.0,
      timestamps: false,
      stream: false,
      keep_alive: 5
    })
  });

  const json = await res.json();
  if (json.status !== "success") {
    throw new Error(json.error?.message || "Transcription failed");
  }
  return json.output.text;
}
```

---

## 📤 Response Schema & Fields

### Complete JSON Response (`stream: false`)

```json
{
  "status": "success",
  "task": "speech_to_text",
  "model": "whisper-large-v3-turbo-INT8",
  "info": null,
  "output": {
    "text": "Hello, this is a live speech-to-text transcription test using Cluaiz Engine.",
    "audio_data": null,
    "segments": null
  }
}
```

### Real-Time SSE Stream Format (`stream: true`)

When streaming is enabled, tokens are emitted incrementally using typed Server-Sent Events:
```text
data: {"status":"chunk","token":"Hello"}

data: {"status":"chunk","token":", this"}

data: {"status":"chunk","token":" is a test"}

data: {"status":"done","result":{"output":{"text":"Hello, this is a test"}}}
```
