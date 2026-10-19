# Text-to-Speech (TTS) Neural Synthesis API (`/v1/audio/speech`)

`POST /v1/audio/speech` (alias: `POST /v1/audio/execute` with task `text_to_speech`)

Synthesizes speech with an installed ONNX TTS package or a GGUF speech backbone and its audio-generation projector. Returns a Base64 WAV data URI in JSON. Model/language coverage and audible quality depend on the selected package.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/audio/speech` (or `/v1/audio/execute`)
* **Content-Type:** `application/json`
* **Authentication:** `Authorization: Bearer <session-token>`
* **Output Format:** Playable WAV Data URI (`data:audio/wav;base64,...`) inside JSON response

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Allowed Values | Description |
|:---|:---|:---:|:---|:---|:---|
| **`input`** | String | **Yes** | — | Any UTF-8 text string | The text content to synthesize into speech. |
| **`model`** | String | No | active TTS slot, else first eligible installed package | Installed TTS package ID | ONNX TTS or GGUF speech model with an audio-generation projector. |
| **`voice`** | String | No | `"default"` | Model voice ID, or GGUF reference audio source | ONNX: `"default"`/`"auto"` selects by language; Kokoro accepts its voice IDs, Piper has one voice. GGUF: default uses the native model's unconditioned voice; any other value is an audio data URI, authorized local file, or allowed remote URL for speaker conditioning. It is not a Kokoro voice ID. |
| **`language`** | String | No | ONNX: inferred; GGUF: native default | Language code such as `"cs"`, `"hi"`, `"en-US"` | ONNX: rules for the text's spelling/numbers, inferred from script/voice/model. GGUF: native language selector with explicit unsupported-language errors; Qwen's default is English. |
| **`speed`** | Float | No | `1.0` | ONNX: `0.25` – `4.0`; GGUF: `1.0` only | GGUF does not expose pace control; any other speed returns HTTP400 rather than being silently ignored. |
| **`stream`** | Boolean | No | `false` | `true`, `false` | `true`: Server-Sent Events. Kokoro/Piper emit a WAV per finished sentence (`speech.audio.delta`), then the whole WAV. GGUF and other families currently send only the final result. |
| **`keep_alive`** | Integer | No | `null` | Minutes (e.g. `5`, `0` to unload, `-1` to retain) | GGUF: positive minutes after successful completion; `0` unloads after the request; omitted/`-1` retains until replacement or shutdown. Native load/generation failure releases the speech model; validation rejected before dispatch leaves any existing resident unchanged. New requests invalidate older expiry timers. Chat and ONNX have separate owners. |

---

## 🎙️ Installed Voice Models

The following official Piper ONNX voice packages are available out-of-the-box:

| Model ID | Language | Gender / Style | Sampling Rate | Description |
|:---|:---|:---|:---|:---|
| **`piper-en-us-ryan-medium`** | English (US) | Male (Natural) | 22.05 kHz | Clean, expressive American English male narrator. |
| **`piper-en_US-amy-medium`** | English (US) | Female (Warm) | 22.05 kHz | Clear, articulate American English female voice. |
| **`piper-voices-hi_in-rohan-medium`** | Hindi (India) / Hinglish | Male | 22.05 kHz | Native Indian accent Hindi & Hinglish voice. |

### Which voices a model has

`GET /v1/audio/voices?model=<id>` (model optional: the active or first installed TTS model) lists the values accepted by `voice`. `"default"` comes first; each voice carries the language its name declares (Kokoro: `a` American English, `b` British English, `h` Hindi, …), `null` when unknown:

```json
{ "object": "list", "model": "Kokoro-82M-v1.0-ONNX-UINT8", "data": [
  { "id": "default", "language": null, "default": true },
  { "id": "af_heart", "language": "en-us", "default": false },
  { "id": "hf_alpha", "language": "hi", "default": false }
] }
```

A single-voice model (Piper) returns only `"default"`.

### GGUF speech (experimental)

A GGUF speech package needs its backbone and matching audio-generation mmproj. This uses the pinned llama.cpp `libmtmd` audio-generation API; it does not load the speech model into the chat context. The test package is `Qwen3-TTS-12Hz-1.7B-Base-GGUF-Q4_K_M` (Q4_K_M backbone plus Q8 projector).

Qwen3-TTS supports en, zh, ja, ko, de, fr, ru, pt, es and it. It does not support Hindi. Supply `language` explicitly for non-English speech; omitted/`auto` uses the native default (English for this package), rather than ONNX script detection. An unsupported language returns an error naming the requested language and the supported names queried from the loaded vocabulary. The engine does not silently switch the selected model. These model capabilities come from the [official model card](https://huggingface.co/Qwen/Qwen3-TTS-12Hz-1.7B-Base), not a claim that each language has passed local listening acceptance.

```json
{
  "model": "Qwen3-TTS-12Hz-1.7B-Base-GGUF-Q4_K_M",
  "input": "Hello, welcome to the local speech engine.",
  "language": "en",
  "voice": "default",
  "speed": 1.0,
  "stream": false,
  "keep_alive": 5
}
```

For speaker conditioning, set `voice` to an audio data URI or audio source, for example `"data:audio/wav;base64,<reference PCM WAV bytes>"`. Local sources use the normal filesystem authorization/approval path; remote and inline sources use shared media limits and format decoding. The input is converted to mono at the projector's queried input rate and encoded by its real speaker encoder. Missing/unsupported encoders and corrupt/oversized references error. A reference influences the voice; it does not prove speaker identity or guarantee similarity. Without a reference the Base model may choose a different voice on each generation.

Inline references use the same configured attachment request limit as transcription and `/v1/audio/execute`, including Base64 expansion and JSON envelope space. Requests beyond that limit return HTTP413 with `request_too_large`.

GGUF generation is currently buffered: `stream:true` sends the final SSE result, with no intermediate `speech.audio.delta` parts. Native model loading/generation may take time before that event. Long inputs that hit the frame limit error instead of returning a silently truncated utterance. The native API is experimental and may change with the pinned library.

---

## 💻 Code Examples

### 1. cURL

```bash
curl -X POST http://localhost:8000/v1/audio/speech \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <session-token>" \
  -d '{
    "model": "piper-en-us-ryan-medium",
    "input": "Welcome to Cluaiz Engine native voice synthesis.",
    "voice": "default",
    "speed": 1.0,
    "keep_alive": 5
  }'
```

### 2. Python (Save to Disk as WAV)

```python
import requests
import base64

headers = {
    "Content-Type": "application/json",
    "Authorization": "Bearer <session-token>"
}

payload = {
    "model": "piper-en-us-ryan-medium",
    "input": "Hello! The local speech synthesis pipeline is active and working.",
    "voice": "default",
    "speed": 1.0,
    "keep_alive": 5
}

response = requests.post("http://localhost:8000/v1/audio/speech", json=payload, headers=headers)
data = response.json()

if data.get("status") == "success":
    audio_uri = data["output"]["audio_data"]
    # Data URI format: data:audio/wav;base64,<actual-bytes>
    _, b64_str = audio_uri.split(",", 1)
    with open("output.wav", "wb") as f:
        f.write(base64.b64decode(b64_str))
    print("Saved audio file to output.wav successfully.")
else:
    print("TTS Error:", data.get("error"))
```

### 3. JavaScript / Browser (Instant Web Audio Playback)

```javascript
async function playSpeech(text, token) {
  const res = await fetch("http://localhost:8000/v1/audio/speech", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": `Bearer ${token}`
    },
    body: JSON.stringify({
      model: "piper-en-us-ryan-medium",
      input: text,
      voice: "default",
      speed: 1.0,
      keep_alive: 5
    })
  });

  const json = await res.json();
  if (json.status === "success") {
    // Directly play the returned WAV Data URI
    const audio = new Audio(json.output.audio_data);
    audio.play();
  }
}
```

---

## 📡 Streaming (`"stream": true`)

The response is `text/event-stream`. Kokoro and Piper send one event per finished sentence, then the whole audio:

```text
data: {"status":"chunk","type":"speech.audio.delta","index":0,"format":"wav","audio":"data:audio/wav;base64,UklGR..."}
data: {"status":"chunk","type":"speech.audio.delta","index":1,"format":"wav","audio":"data:audio/wav;base64,UklGR..."}
data: {"status":"done","result":{"status":"success","task":"text_to_speech","model":"...","output":{"audio_data":"data:audio/wav;base64,..."}}}
```

Play the `audio` parts in `index` order, back to back (each is a complete WAV). Parts are clamped, not loudness-normalised; the final `audio_data` is the whole text normalised. Other TTS families send only the final event.

## 📤 Response Schema & Fields

### Success Response (`HTTP 200`)

```json
{
  "status": "success",
  "task": "text_to_speech",
  "model": "piper-en-us-ryan-medium",
  "info": null,
  "output": {
    "text": null,
    "audio_data": "data:audio/wav;base64,UklGRsYLAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQILAAA...",
    "segments": null
  }
}
```

### Response Fields

* **`status`**: Request execution state (`"success"` or `"error"`).
* **`task`**: Task discriminator (`"text_to_speech"`).
* **`model`**: Actual executed voice package ID.
* **`output.audio_data`**: Standard RFC 2397 Data URI containing `audio/wav` header and Base64-encoded PCM bytes.
