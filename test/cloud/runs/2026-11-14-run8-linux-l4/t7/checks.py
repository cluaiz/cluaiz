"""Engine checks on a cloud VM against a running isolated server: every chat model with a short
and a long prompt (placement, context, speed), speech -> transcription round trip, embeddings,
GGUF speech, then mixed rounds. Writes results.json and prints one line per call."""
import base64
import json
import subprocess
import sys
import time
import urllib.error
import urllib.request

BASE, HOME, ROUNDS = sys.argv[1], sys.argv[2], int(sys.argv[3])
OUT = sys.argv[4]
TOKEN = open(f"{HOME}/session.token").read().strip()
rows = []


def gpu():
    out = subprocess.run(["nvidia-smi", "--query-gpu=memory.used,memory.free", "--format=csv,noheader,nounits"],
                         capture_output=True, text=True).stdout.strip()
    return out.replace("\n", " | ") or "no gpu"


def call(path, body, timeout=1800):
    req = urllib.request.Request(BASE + path, json.dumps(body).encode(),
                                 {"content-type": "application/json", "authorization": "Bearer " + TOKEN})
    start = time.time()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            data, status = json.loads(r.read()), r.status
    except urllib.error.HTTPError as e:
        data, status = json.loads(e.read() or b"null"), e.code
    except Exception as e:  # timeouts, refused connections
        data, status = {"error": repr(e)}, 0
    return status, data, time.time() - start


def record(name, model, status, data, secs, **extra):
    ok = status == 200 and not (isinstance(data, dict) and data.get("error"))
    row = {"name": name, "model": model, "ok": ok, "secs": round(secs, 2), "gpu_mib_used_free": gpu(), **extra}
    if not ok:
        row["error"] = json.dumps(data)[:600]
    rows.append(row)
    print(json.dumps(row), flush=True)
    return ok


def models():
    req = urllib.request.Request(BASE + "/v1/models", headers={"authorization": "Bearer " + TOKEN})
    with urllib.request.urlopen(req) as r:
        return json.loads(r.read()).get("data", [])


def find(needle):
    ids = [m["id"] for m in models() if needle.lower() in m["id"].lower()]
    return ids[0] if ids else None


def chat(model, prompt, max_tokens=160):
    s, d, t = call("/v1/chat/completions", {"model": model, "messages": [{"role": "user", "content": prompt}],
                                            "max_tokens": max_tokens, "think_mode": "off", "tool_search": False})
    e = (d or {}).get("engine") or {} if isinstance(d, dict) else {}
    load, tm = e.get("load") or {}, e.get("timings") or {}
    text = ""
    if isinstance(d, dict) and d.get("choices"):
        text = (d["choices"][0].get("message") or {}).get("content", "")[:120]
    return s, d, t, {"placement": load.get("placement"), "n_gpu_layers": load.get("n_gpu_layers"),
                     "n_ctx": load.get("n_ctx"), "prompt_tps": tm.get("prompt_per_second"),
                     "gen_tps": tm.get("predicted_per_second"), "reply": text}


LONG = " ".join(["Rivers carry water from mountains to the sea, shaping valleys and feeding farms."] * 140) + \
       "\nSummarise the text above in two sentences."

ids = {k: find(k) for k in ["qwen3-4b", "gemma-3-12b", "mistral-small", "bge-m3", "kokoro", "whisper", "qwen3-tts"]}
print("models:", json.dumps(ids), flush=True)

for key in ["qwen3-4b", "gemma-3-12b", "mistral-small"]:
    if ids[key]:
        for label, prompt in [("short", "Write three sentences about rivers."), ("long", LONG)]:
            s, d, t, info = chat(ids[key], prompt)
            record(f"chat-{label}", ids[key], s, d, t, **info)

wav_b64 = None
if ids["kokoro"]:
    s, d, t = call("/v1/audio/speech", {"model": ids["kokoro"], "input": "Rivers carry water to the sea."})
    record("tts-onnx", ids["kokoro"], s, d, t)
    uri = ((d or {}).get("output") or {}).get("audio_data") if isinstance(d, dict) else None
    if uri and "," in uri:
        wav_b64 = uri.split(",", 1)[1]
        open(f"{OUT}/kokoro.wav", "wb").write(base64.b64decode(wav_b64))
if ids["whisper"] and wav_b64:
    path = f"{OUT}/kokoro.wav"
    s, d, t = call("/v1/audio/transcriptions", {"model": ids["whisper"], "file": path})
    text = ((d or {}).get("output") or {}).get("text") if isinstance(d, dict) else None
    record("stt-onnx", ids["whisper"], s, d, t, transcript=text)
if ids["bge-m3"]:
    s, d, t = call("/v1/embeddings", {"model": ids["bge-m3"], "input": ["Rivers carry water.", "The sea is salty."]})
    record("embed-gguf", ids["bge-m3"], s, d, t)
if ids["qwen3-tts"]:
    s, d, t = call("/v1/audio/speech", {"model": ids["qwen3-tts"], "input": "Rivers carry water to the sea."})
    record("tts-gguf", ids["qwen3-tts"], s, d, t)

order = [("chat", ids["qwen3-4b"]), ("tts-onnx", ids["kokoro"]), ("embed-gguf", ids["bge-m3"]),
         ("stt-onnx", ids["whisper"]), ("tts-gguf", ids["qwen3-tts"])]
for rnd in range(ROUNDS):
    for name, model in order:
        if not model:
            continue
        if name == "chat":
            s, d, t, info = chat(model, "Write three sentences about rivers.")
            record(f"mix{rnd}-chat", model, s, d, t, **info)
        elif name == "tts-onnx" or name == "tts-gguf":
            s, d, t = call("/v1/audio/speech", {"model": model, "input": "Rivers carry water to the sea."})
            record(f"mix{rnd}-{name}", model, s, d, t)
        elif name == "embed-gguf":
            s, d, t = call("/v1/embeddings", {"model": model, "input": ["Rivers carry water."]})
            record(f"mix{rnd}-{name}", model, s, d, t)
        elif wav_b64:
            s, d, t = call("/v1/audio/transcriptions", {"model": model, "file": f"{OUT}/kokoro.wav"})
            record(f"mix{rnd}-{name}", model, s, d, t)

json.dump(rows, open(f"{OUT}/results.json", "w"), indent=1)
print(f"\nfailed {sum(1 for r in rows if not r['ok'])} of {len(rows)}")
