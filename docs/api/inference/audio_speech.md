# Text-to-Speech (TTS) Neural Synthesis API (`/v1/audio/speech`)

`POST /v1/audio/speech` (alias: `POST /v1/audio/execute` with task `text_to_speech`)

Synthesizes high-fidelity natural speech audio from input text using ONNX-accelerated neural voice engines (Piper, VITS). Outputs playable Base64 WAV Data URIs ready for instant browser playback or file saving without intermediate disk writes.

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
| **`model`** | String | No | `"piper-en-us-ryan-medium"` | Installed TTS package ID | Neural TTS voice model package ID. |
| **`voice`** | String | No | `"default"` | Speaker persona | Voice persona or speaker ID within multi-speaker packages. |
| **`speed`** | Float | No | `1.0` | `1.0` | Voice playback speed multiplier. |
| **`keep_alive`** | Integer | No | `null` | Minutes (e.g. `5`, `0` to unload, `-1` forever) | Session memory retention policy in minutes. |

---

## 🎙️ Installed Voice Models

The following official Piper ONNX voice packages are available out-of-the-box:

| Model ID | Language | Gender / Style | Sampling Rate | Description |
|:---|:---|:---|:---|:---|
| **`piper-en-us-ryan-medium`** | English (US) | Male (Natural) | 22.05 kHz | Clean, expressive American English male narrator. |
| **`piper-en_US-amy-medium`** | English (US) | Female (Warm) | 22.05 kHz | Clear, articulate American English female voice. |
| **`piper-voices-hi_in-rohan-medium`** | Hindi (India) / Hinglish | Male | 22.05 kHz | Native Indian accent Hindi & Hinglish voice. |

---

## 💻 Code Examples

### 1. cURL

```bash
curl -X POST http://localhost:8000/v1/audio/speech \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <session-token>" \
  -d '{
    "model": "piper-en-us-ryan-medium",
    "input": "Welcome to Cluaiz Engine native voice synthesis."
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
    "input": "Hello! The local speech synthesis pipeline is active and working."
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
      input: text
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
