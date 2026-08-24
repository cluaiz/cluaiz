# Speech-to-Text (STT) Transcriptions API

`POST /v1/audio/transcriptions` & `POST /v1/audio/execute`

Transcribes spoken audio into text using local Whisper acoustic neural models with multi-threaded Mel-spectrogram processing and automatic language detection.

---

## 📌 Endpoint Information

* **HTTP Method:** `POST`
* **Path:** `/v1/audio/transcriptions` (or `/v1/audio/execute`)
* **Content-Type:** `application/json`

---

## 📥 Request Parameters

| Parameter | Type | Required | Default | Description |
|:---|:---|:---|:---|:---|
| **`input_source`** | Object | **Yes** | — | Audio input source object: `{"type": "url" \| "base64" \| "file", "data": "..."}`. |
| **`model`** | String | No | `"whisper-large-v3-turbo"` | Whisper model identifier. |
| **`language`** | String | No | `"auto"` | Two-letter ISO language code (e.g. `"en"`, `"hi"`, `"auto"`). |
| **`timestamps`** | Boolean | No | `false` | Whether to return segment-level start and end timestamps. |
| **`temperature`** | Float | No | `0.0` | Sampling temperature for transcription decoding. |

---

## 💻 Code Examples

### 1. cURL (Web URL or Local File Path)

```bash
curl -X POST http://localhost:8000/v1/audio/transcriptions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "whisper-large-v3-turbo",
    "input_source": {
      "type": "url",
      "data": "https://audio-samples.github.io/samples/mp3/blizzard_tts_unbiased/sample-3/real.mp3"
    },
    "language": "auto",
    "timestamps": true
  }'
```

### 2. Python (Requests)

```python
import requests

payload = {
    "model": "whisper-large-v3-turbo",
    "input_source": {
        "type": "url",
        "data": "https://audio-samples.github.io/samples/mp3/blizzard_tts_unbiased/sample-3/real.mp3"
    },
    "language": "auto",
    "timestamps": True
}

response = requests.post("http://localhost:8000/v1/audio/transcriptions", json=payload)
data = response.json()

if data.get("status") == "success":
    output = data.get("output", {})
    print("Transcription:", output.get("text"))
    if output.get("segments"):
        for seg in output["segments"]:
            print(f"[{seg['start']:.2f}s - {seg['end']:.2f}s]: {seg['text']}")
```

---

## 📤 Response Format

```json
{
  "status": "success",
  "task": "speech_to_text",
  "model": "whisper-large-v3-turbo",
  "info": null,
  "output": {
    "text": "Hello, this is a transcription test using Cluaiz Engine.",
    "audio_data": null,
    "segments": [
      {
        "start": 0.0,
        "end": 3.5,
        "text": "Hello, this is a transcription test using Cluaiz Engine.",
        "speaker": null
      }
    ]
  }
}
```
