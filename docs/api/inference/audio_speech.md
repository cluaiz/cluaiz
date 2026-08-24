# Text-to-Speech (TTS) API

`POST /v1/audio/speech` & `POST /v1/audio/execute`

Synthesizes speech audio from text using local ONNX neural voice engines (Kokoro, Piper, VITS).

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/audio/speech` (or `/v1/audio/execute`)
* **Content-Type:** `application/json`

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`input`** | String | **Yes** | — | The text string to synthesize into speech. |
| **`model`** | String | No | `"auto"` | TTS model identifier (e.g. `"kokoro-82m"`, `"piper"`). |
| **`voice`** | String | No | `"alloy"` | Voice persona / speaker identity. |
| **`speed`** | Float | No | `1.0` | Speed multiplier for generated speech (`0.25 - 4.0`). |

---

## 💻 Code Examples

### 1. cURL

```bash
curl -X POST http://localhost:8000/v1/audio/speech \
  -H "Content-Type: application/json" \
  -d '{
    "model": "kokoro-82m",
    "input": "Welcome to Cluaiz Engine native voice synthesis.",
    "voice": "alloy",
    "speed": 1.0
  }'
```

### 2. Python (Requests)

```python
import requests
import base64

payload = {
    "model": "kokoro-82m",
    "input": "Hello! The local speech synthesis pipeline is active.",
    "voice": "alloy",
    "speed": 1.0
}

response = requests.post("http://localhost:8000/v1/audio/speech", json=payload)
data = response.json()

if data.get("status") == "success":
    audio_uri = data["output"]["audio_data"]
    # Extract Base64 data from data URI: data:audio/wav;base64,...
    header, b64_str = audio_uri.split(",", 1)
    with open("output.wav", "wb") as f:
        f.write(base64.b64decode(b64_str))
    print("Saved audio to output.wav")
```

---

## 📤 Response Format

```json
{
  "status": "success",
  "task": "text_to_speech",
  "model": "kokoro-82m",
  "output": {
    "text": null,
    "audio_data": "data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEA...",
    "segments": null
  }
}
```
