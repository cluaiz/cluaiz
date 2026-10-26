---
method: "audio.run"
transport: "POST /v1/cep"
category: "ai-inference"
category_title: "AI & Inference Core"
group: "audio"
summary: "Run the existing speech/transcription executor; chunks are task events"
scope: "read"
execution: "task"
mcp: true
owner_only: false
destructive: false
idempotent: true
params:
  - name: "input_source"
    type: "object"
    req: true
    desc: "Input descriptor: text for speech generation, or a local/remote/inline audio source for transcription"
    example: {"data":"Speech input","type":"text"}
    fields:
      - name: "data"
        type: "string"
        req: true
        desc: "The text; a file path; an http(s) URL; or a `data:audio/...;base64,` URI"
        example: "Speech input"
      - name: "type"
        type: "string"
        req: true
        desc: "text (speech), or file / url / audio / base64 (transcription)"
        example: "text"
  - name: "instruction"
    type: "string | null"
    req: false
    desc: "Not supported: a non-empty value is refused"
    example: null
  - name: "keep_alive"
    type: "integer | null"
    req: false
    desc: "Minutes the model stays loaded after the task: -1 = until replaced, 0 = unload now, N = N minutes"
    example: null
  - name: "model"
    type: "string | null"
    req: false
    desc: "An installed speech or transcription model id (default: the active slot, else the first installed model for the task)"
    example: null
  - name: "parameters"
    type: "object | null"
    req: false
    desc: "Task options"
    example: null
    fields:
      - name: "beam_size"
        type: "integer | null"
        req: false
        desc: "Only 1 is supported"
        example: null
      - name: "language"
        type: "string | null"
        req: false
        desc: "Speech: the text's language (\"cs\", \"hi\", \"en-US\"). Transcription: the spoken language, or \"auto\""
        example: null
      - name: "languages"
        type: "string[]"
        req: false
        desc: "The speaker's languages (ISO 639-1): without `language`, speech is detected among only these"
        example: null
      - name: "speaker_labels"
        type: "boolean | null"
        req: false
        desc: "Only false is supported"
        example: null
      - name: "speed"
        type: "number | null"
        req: false
        desc: "Speech rate, 0.25 to 4.0 (speech only)"
        example: null
      - name: "temperature"
        type: "number | null"
        req: false
        desc: "Transcription sampling temperature; only 0 (greedy) is supported"
        example: null
      - name: "timestamps"
        type: "boolean | null"
        req: false
        desc: "Only false is supported"
        example: null
      - name: "translate_to"
        type: "string | null"
        req: false
        desc: "\"en\" with a model that declares speech_translation; other targets are refused"
        example: null
      - name: "vad_filter"
        type: "boolean | null"
        req: false
        desc: "Only false is supported"
        example: null
      - name: "voice_id"
        type: "string | null"
        req: false
        desc: "A voice id from `audio.voices`; \"default\" / \"auto\" picks one for the text's language (speech only)"
        example: null
  - name: "task"
    type: "string | null"
    req: false
    desc: "text_to_speech, speech_to_text or speech_translation (default: from `input_source.type`, text = speech)"
    example: null
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "audio.run",
    "params": {
      "input_source": {
        "data": "Speech input",
        "type": "text"
      }
    }
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "task": {
        "status": "working",
        "task_id": "t_example"
      }
    }
  }
errors:
  - code: -32602
    type: "unsupported_parameter"
    when: "An option the model does not support: timestamps, VAD, speaker labels, beam size above 1, temperature above 0, voice / speed for transcription"
  - code: -32602
    type: "unsupported_language"
    when: "The speech model does not support `language`"
  - code: -32602
    type: "unsupported_task"
    when: "`task` is not text_to_speech, speech_to_text or speech_translation, or the model does not declare it"
  - code: -32602
    type: "invalid_input"
    when: "An unreadable source: a bare base64 string, a missing file, a non-audio file"
  - code: -32004
    type: "model_not_found"
    when: "No installed model with this id, or no installed model for the task"
  - code: -32602
    type: "model_contract_error"
    when: "The model's package does not match its declared task (missing voices, config or assets)"
  - code: -32002
    type: "forbidden"
    when: "The model or the source file is outside the caller's roots (`data.needs: \"roots\"`)"
  - code: -32010
    type: "speech_busy"
    when: "Another speech request is running (from the task's result)"
  - code: -32010
    type: "engine_error"
    when: "The model failed while running (from the task's result)"
  - code: -32008
    type: "cancelled"
    when: "`tasks.cancel` stopped it (from the task's result)"
---

# audio.run

Speech (text to audio) or transcription (audio to text), as a task. It runs the same code as `/v1/audio/speech` and `/v1/audio/transcriptions`; the difference is that the answer is a task you can poll, cancel, and read chunk by chunk.

## Input

`input_source.type` decides what is sent:

| `type` | `data` | Task |
|---|---|---|
| `text` | The text to speak | text_to_speech |
| `file` | A path on the Engine's machine (inside the caller's roots) | speech_to_text |
| `url` | An http(s) URL | speech_to_text |
| `audio` / `base64` | A `data:audio/<format>;base64,…` URI | speech_to_text |

`task` can say it explicitly (`text_to_speech`, `speech_to_text`, `speech_translation`).

## What it does

1. Picks the model (`model`, else the active slot, else the first installed model for the task) and checks every option before starting: unsupported options are refused, never ignored (timestamps, VAD, speaker labels, beam size above 1, temperature above 0, voice or speed for transcription).
2. Checks the model folder and a `file` source against the caller's roots.
3. Answers at once with a task handle.
4. Runs. Each chunk is a task event: `{"type": "transcript.delta", "index", "text"}` for transcription, `{"type": "speech.audio.delta", "index", "format": "wav", "audio"}` for speech.
5. The task's result is `{"model", "task", "output": {"text", "audio_data", "segments"}}`: `text` for transcription, `audio_data` (`data:audio/wav;base64,…`) for speech; `segments` is `null`.

`keep_alive`: minutes the model stays loaded after the task (`-1` until replaced, `0` unload now). [`tasks.cancel`](../../system/tasks/tasks.cancel.md) stops it.

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"audio.run","params":{"task":"speech_to_text","model":"whisper-large-v3-turbo-INT8","input_source":{"type":"file","data":"D:/audio/memo.wav"}}}'
```

Leave out the `Authorization` header when API authentication is off.

Measured on an isolated Engine (whisper-large-v3-turbo INT8, a 4-second WAV made with Windows speech): result `output.text` = "The secret word is mango. Please remember it!", in 15.2 s including the first load.
