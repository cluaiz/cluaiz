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
| **`language`** | String | No | ONNX: inferred; GGUF: native default | Language code such as `"cs"`, `"hi"`, `"en-US"` | Phoneme runtimes: spelling/number rules from script/voice/model. GGUF: native selector; unsupported vocabulary languages return HTTP400 before model load. Audio8 determines language from text and rejects forcing; omit language or use `"auto"`. |
| **`speed`** | Float | No | `1.0` | `0.25` – `4.0` | Kokoro/Piper use model duration controls. GGUF applies shared pitch-preserving output tempo at the native sample rate; this is not a native model prosody control. Other families' support depends on their runtime. |
| **`stream`** | Boolean | No | `false` | `true`, `false` | `true`: Server-Sent Events. Kokoro/Piper emit WAV parts per settled sentence; GGUF emits new generated PCM as WAV parts (`speech.audio.delta`), then the whole WAV. Other families depend on their runtime. |
| **`keep_alive`** | Integer | No | `null` | Minutes (e.g. `5`, `0` to unload, `-1` to retain) | Positive minutes after successful completion; `0` unloads after the request; omitted/`-1` retains until replacement/shutdown. Execution failures release the selected resident; validation rejected before dispatch preserves existing residency. New requests invalidate older expiry timers. Changing TTS backends releases the previous TTS resident before loading its replacement, while preserving chat and non-TTS ONNX residents. |

---

## Choose the model, language and voice

Read `GET /v1/models/installed` and select an exact installed TTS ID. The API Explorer Model dropdown writes it into the JSON `model` field; its Voice dropdown reads `/v1/audio/voices?model=<id>` for that package. Switching models resets the previous package's speaker selection. A Hugging Face repository ID is not necessarily the installed ID.

| Package behavior | `voice` | `language` | Example |
|:---|:---|:---|:---|
| One embedded speaker | Omit or `default` | Package-supported language | Piper |
| Preset speaker bank | ID from `data[].id` in voice discovery | Supported explicit language, or inferred default | Kokoro |
| Generated voice with optional reference | `default`, or an authorized audio source if `voice_mode` is `reference_source` | Supported native language | Qwen3-TTS Base GGUF |
| Encoded reference profiles | Package-local profile ID when provided; otherwise omit/default | Automatic when capability says `auto` | Audio8 export |

Do not send a Kokoro speaker ID to Qwen or Audio8. Do not force a language absent from the selected package. A default-only voice list can still support several languages. These examples describe compatible package contracts; installation and subjective voice quality are separate checks.

For generated voices, send a complete narration in one request when `capabilities.coherent_input` is true. Separate unconditioned sentence/paragraph requests can choose different speakers. A speaker reference can influence consistency, but does not guarantee identity or remove all model noise. `stream:false` returns the completed utterance; `stream:true` can underrun when generation is slower than playback.

## Example voice packages

The following are example Piper packages. Install a compatible package first and use its exact registry ID; they are not bundled with every engine installation:

| Model ID | Language | Gender / Style | Sampling Rate | Description |
|:---|:---|:---|:---|:---|
| **`piper-en-us-ryan-medium`** | English (US) | Male (Natural) | 22.05 kHz | Clean, expressive American English male narrator. |
| **`piper-en_US-amy-medium`** | English (US) | Female (Warm) | 22.05 kHz | Clear, articulate American English female voice. |
| **`piper-voices-hi_in-rohan-medium`** | Hindi (India) / Hinglish | Male | 22.05 kHz | Native Indian accent Hindi & Hinglish voice. |

### Which voices a model has

The existing `data` array now has an additive sibling `capabilities` object:

```json
{"languages":["en","es"],"language_control":"explicit","voice_mode":"reference_source","coherent_input":true}
```

This is a schema example, not a named model's complete inventory. A default-only model can support multiple languages. Count named speakers from `data` entries with `default:false`; `default` is a control. GGUF languages come from actual header codec tokens and native ISO aliases without loading weights. ONNX languages come from voices/configuration; Audio8's automatic-language list comes from its export protocol. `GET /v1/audio/languages` remains STT-only.

`language_control` is `explicit` or `auto`. `voice_mode` is `preset`, `embedded`, `generated` (no declared reference encoder), `reference_source` (authorized GGUF audio), or `reference_profile` (package-local encoded profile). `coherent_input:true` tells clients to keep a narration together: independent unconditioned requests may choose different speakers. It does not guarantee speaker identity. A reference encoder is advertised only when the inspected matching projector declares one.

One TTS request is admitted at a time across ONNX/GGUF, before reference decoding. Overlap returns HTTP429 `speech_busy`; stopping a client releases admission after outstanding preparation/native work finishes. Native load/frame operations remain non-preemptible.

GGUF speech honors captured `optimization.n_gpu_layers` (null Auto, 0 CPU, -1 all GPU layers, positive exact count), generation/batch threads and mmap preference. CPU mode disables GPU operation/KV offload and GPU speaker encoding. Changing load settings or backbone/projector path/size/mtime replaces the resident. ONNX uses `onnx.n_gpu_layers` and its existing provider configuration.

Loads share the chat/ONNX/GGUF transition lock. Preflight uses the shared live-memory reserve policy, complete package bytes and configured media workspace; GGUF adds its fixed context/logits estimate. These estimates are not OS reservations or measured peaks. Explicit GPU admission failure errors; automatic native GPU-load failure may retry CPU after a new memory check. Other applications and later allocations can still change free memory.

Stage logs separate native preparation/generation/decoder/output and Audio8 prefill/Slow AR/Fast AR/decode-output. They exclude model loading; output includes sink backpressure and Audio8 stages include tensor transfers. They do not establish realtime speed. Buffering complete speech trades startup delay for uninterrupted playback; streaming can underrun when synthesis is slower than playback.

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

A GGUF speech package needs its backbone and matching audio-generation mmproj. This uses the pinned llama.cpp `libmtmd` audio-generation API; it does not load the speech model into the chat context. The exercised local package used a Qwen3-TTS Base Q4_K_M backbone plus a BF16 projector. Other compatible exports can have different projector precision; use the matching assets for the installed package.

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

GGUF generation emits new native PCM as WAV parts during generation and then the whole utterance. Model loading and prompt evaluation still precede the first part; polling can also flush the native codec's pending frames. `speed` uses a streaming Sonic tempo processor with the sample rate unchanged. Closing the consumer cancels at prompt/frame boundaries; native load/generation failures release the resident speech model. Long inputs that hit the frame limit error instead of returning a silently truncated utterance. The native API is experimental and may change with the pinned library.

### Audio8 exported package (experimental)

The native ORT route requires `runtime_manifest.json`, its declared Slow AR/Fast AR and codec graphs with external data, and `tokenizer/tokenizer.json`. Only the inspected FP16 `valid_prefix` KV-cache and `semantic_then_eos` logits contract is accepted. Missing assets or incompatible tensor layouts return errors before synthesis; an older incomplete installation needs its missing package files restored.

Omit `voice` for the official processor's no-reference prompt. A named voice selects package-local `voices/<id>/meta.json` with nonempty `reference_text`, a matching `model_fingerprint`, and `codes.npy` containing a C-order little-endian uint16 or int64 `[num_codebooks,T]` reference matrix. Unknown voices, escaping paths and invalid code ranges error. It does not accept arbitrary speaker audio through this named-voice route. The generation uses genuine Slow/Fast AR logits and codec books, retaining typed FP16 cache deltas at the evaluated positions. `stream:true` sends guarded codec-window WAV parts; the final WAV joins those same parts. Speed uses the shared output tempo processor. A generation/context limit is an error, not successful truncated speech. These implementation paths do not certify multilingual listening quality or speaker similarity.

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

### 3. JavaScript / Browser (Play Completed WAV)

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

  if (!res.ok) throw new Error(await res.text());
  const json = await res.json();
  if (json.status === "success") {
    // Directly play the returned WAV Data URI
    const audio = new Audio(json.output.audio_data);
    await audio.play(); // Call from a user gesture if the browser blocks autoplay.
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

Play the `audio` parts in `index` order, back to back (each is a complete WAV). ONNX sentence parts are clamped, not loudness-normalised; the final `audio_data` is the whole text normalised. GGUF parts and final audio use the same PCM16 encoding/tempo stream without per-part normalization. A failed/cancelled native generation can deliver earlier valid parts followed by an error; it does not deliver a successful final result. Remaining families' progressive output depends on their runtime.

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

GGUF speech placement follows captured engine optimization settings: CPU stays CPU; Auto can split backbone layers across GPU/RAM with the codec placed independently. The existing host/device reserves and media limits still apply, and an explicit GPU placement that exceeds admission returns an error. Placement is selected at load time; unload/reload a retained speech model to reconsider newly freed VRAM. These estimates do not guarantee native peak memory or uninterrupted realtime playback.

## More payload examples

Use your exact installed IDs if they differ from the examples.

### Kokoro preset speaker and Hindi text

```json
{
  "model": "Kokoro-82M-v1.0-ONNX-UINT8",
  "input": "नमस्ते। आज हम स्थानीय इंजन का उपयोग कर रहे हैं।",
  "voice": "hf_alpha",
  "language": "hi",
  "speed": 1,
  "stream": false,
  "keep_alive": 0
}
```

Use `hf_alpha` only if this installed package lists it. For Piper, use its matching language package and `default`; the voice ID cannot turn an English Piper package into a Hindi package.

### Qwen Base with speaker conditioning

```json
{
  "model": "qwen3-tts-12hz-1.7b-base-gguf-Q4_K_M",
  "input": "Read this complete paragraph with the reference voice.",
  "voice": "C:/audio/reference.wav",
  "language": "en",
  "speed": 1,
  "stream": true,
  "keep_alive": 5
}
```

The projector must declare a speaker encoder. The reference path is on the server and must pass filesystem policy; a real audio data URI or allowed remote source is an alternative. Omit `voice` for unconditioned output. The backbone's quantization and the codec/projector precision can differ; the locally exercised package used a BF16 projector. Download size is not peak runtime memory.

### Audio8 without a reference profile

```json
{
  "model": "audio8-tts-preview-0.6b-onnx-int4",
  "input": "Keep this narration together in one request.",
  "speed": 1,
  "stream": false,
  "keep_alive": 0
}
```

Omit `voice` and `language` for this example. Audio8's manifest-selected package includes Slow AR, Fast AR, codec, external tensor data and tokenizer; selecting one graph alone is not a complete installation. A named reference profile must already exist in the package. These examples do not certify realtime performance or voice similarity.

## Common request errors

Missing/invalid authentication returns HTTP 401 when required. An uninstalled model returns HTTP 404 `model_not_found`. Wrong-task packages, unsupported language/voice controls, invalid speed, incompatible assets and unknown top-level fields return errors. `response_format`, `device` and chat `messages` are not fields of this route. Concurrent TTS returns HTTP 429 `speech_busy`. A frame/context limit is a failure, not a successful shortened narration.

An SSE response can start with HTTP 200 and later emit `status:"error"`; only the terminal `done` event carries the successful complete result. The abbreviated Base64 strings in response examples illustrate the schema and are not playable WAV fixtures.
