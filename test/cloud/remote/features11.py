"""Run 11 feature checks against a running isolated server, for the models new to the cloud runs and
the parts the llama.cpp update changed: native tool calls (two turns), thinking, vision, multi-turn
cache reuse, embeddings, llama.cpp speech (gen-audio input struct), ONNX TTS and STT with streaming.
One JSON line per check; results.json at the end.

    python3 features11.py <base url> <home> <out dir>
"""
import base64, json, struct, sys, time, urllib.error, urllib.request, zlib
from pathlib import Path

BASE, HOME, OUT = sys.argv[1], Path(sys.argv[2]), Path(sys.argv[3])
OUT.mkdir(parents=True, exist_ok=True)
TOKEN = (HOME / "session.token").read_text().strip()
H = {"content-type": "application/json", "authorization": "Bearer " + TOKEN}
rows = []
TOOLS = [{"type": "function", "function": {"name": "get_weather", "description": "Current weather of a city",
          "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}]


def call(path, body, timeout=1800):
    req = urllib.request.Request(BASE + path, json.dumps(body).encode(), H)
    start = time.time()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, json.loads(r.read()), time.time() - start
    except urllib.error.HTTPError as e:
        return e.code, {"error": e.read().decode(errors="replace")[:600]}, time.time() - start
    except Exception as e:
        return 0, {"error": repr(e)}, time.time() - start


def stream(path, body, timeout=1800):
    """SSE: returns (status, events, seconds to first event, total seconds)"""
    req = urllib.request.Request(BASE + path, json.dumps({**body, "stream": True}).encode(), H)
    start = time.time(); first = None; events = []
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            for raw in r:
                line = raw.decode(errors="replace").strip()
                if not line.startswith("data:"):
                    continue
                first = first or time.time() - start
                data = line[5:].strip()
                if data == "[DONE]":
                    break
                try:
                    events.append(json.loads(data))
                except ValueError:
                    events.append({"raw": data})
            return 200, events, first, time.time() - start
    except urllib.error.HTTPError as e:
        return e.code, [{"error": e.read().decode(errors="replace")[:600]}], first, time.time() - start
    except Exception as e:
        return 0, [{"error": repr(e)}], first, time.time() - start


def record(name, model, ok, secs, **extra):
    row = {"check": name, "model": model, "ok": bool(ok), "secs": round(secs, 2), **extra}
    rows.append(row); print(json.dumps(row, ensure_ascii=False), flush=True)


def models(kind=None):
    req = urllib.request.Request(BASE + "/v1/models", headers=H)
    with urllib.request.urlopen(req) as r:
        data = json.loads(r.read()).get("data", [])
    return [m["id"] for m in data if kind is None or kind in json.dumps(m).lower()]


def find(needle):
    ids = [m for m in models() if needle.lower() in m.lower()]
    return ids[0] if ids else None


def msg(d):
    return ((d.get("choices") or [{}])[0].get("message") or {}) if isinstance(d, dict) else {}


def chat(model, messages, **extra):
    return call("/v1/chat/completions", {"model": model, "messages": messages, "temperature": 0, "max_tokens": 400,
                                         "temporary_chat": "strict", **extra})


def png(w, h, pixel):
    rows_ = b"".join(b"\x00" + b"".join(bytes(pixel(x, y)) for x in range(w)) for y in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows_)) + chunk(b"IEND", b"")


def chat_checks(model):
    # tool call, then the answer after the tool result
    m = [{"role": "user", "content": "What is the weather in Paris right now? Use the tool."}]
    s, d, t = chat(model, m, tools=TOOLS, think_mode="off")
    calls = msg(d).get("tool_calls") or []
    args = calls[0]["function"]["arguments"] if calls else ""
    record("tool call", model, s == 200 and calls and calls[0]["function"]["name"] == "get_weather" and "paris" in args.lower(), t,
           calls=json.dumps(calls)[:200], error=d.get("error") if isinstance(d, dict) else None)
    if calls:
        m2 = m + [msg(d), {"role": "tool", "tool_call_id": calls[0].get("id", "call_0"), "content": '{"temp_c": 18, "sky": "cloudy"}'}]
        s, d, t = chat(model, m2, tools=TOOLS, think_mode="off")
        content = msg(d).get("content") or ""
        # the answer itself: no control tokens, and not the model's reasoning about the call
        record("answer after tool result", model, s == 200 and "18" in content and "<|" not in content
               and "the user asks" not in content.lower(), t, content=content[:160])
    # thinking: reasoning kept apart from the answer
    s, d, t = chat(model, [{"role": "user", "content": "Is 91 a prime number? Answer yes or no."}], max_tokens=2000)
    mm = msg(d); content = mm.get("content") or ""
    record("thinking", model, s == 200 and "no" in content.lower() and "<think>" not in content and "<|channel|>" not in content, t,
           reasoning_chars=len(mm.get("reasoning_content") or ""), content=content[:120])
    # second turn of a conversation reuses the first one's cache
    conv = [{"role": "system", "content": "You are a helpful assistant. " + "Keep answers short. " * 40},
            {"role": "user", "content": "Name three colours."}]
    s, d, t1 = chat(model, conv, think_mode="off", max_tokens=40)
    conv += [msg(d), {"role": "user", "content": "Now name three animals."}]
    s, d, t2 = chat(model, conv, think_mode="off", max_tokens=40)
    ctx = ((d.get("engine") or {}).get("context") or {}) if isinstance(d, dict) else {}
    record("second turn", model, s == 200 and bool(msg(d).get("content")), t1 + t2, cache_n=ctx.get("cache_n"),
           cause=ctx.get("cause"), processed=ctx.get("processed_tokens"))


def vision_check(model):
    img = "data:image/png;base64," + base64.b64encode(png(96, 96, lambda x, y: (20, 40, 220) if x < 48 else (240, 200, 20))).decode()
    s, d, t = chat(model, [{"role": "user", "content": [
        {"type": "text", "text": "Which colour is on the left half and which on the right half of this image?"},
        {"type": "image_url", "image_url": {"url": img}}]}], think_mode="off", max_tokens=80)
    c = (msg(d).get("content") or "").lower()
    record("vision", model, s == 200 and "blue" in c and "yellow" in c, t, content=c[:160], error=d.get("error") if isinstance(d, dict) else None)


ids = models()
print(json.dumps({"models": ids}), flush=True)
for needle in ("qwen3-30b-a3b", "gpt-oss-20b", "qwen3.5-9b", "phi-4-mini"):
    model = find(needle)
    if not model:
        record("model present", needle, False, 0); continue
    chat_checks(model)
    if needle == "qwen3.5-9b":
        vision_check(model)

# embeddings
emb = find("embeddinggemma")
if emb:
    s, d, t = call("/v1/embeddings", {"model": emb, "input": ["the cat sleeps", "a cat is sleeping", "stock prices fell"]})
    vecs = [e.get("embedding") for e in (d.get("data") or [])] if isinstance(d, dict) else []
    ok = s == 200 and len(vecs) == 3 and all(vecs)
    if ok:
        cos = lambda a, b: sum(x * y for x, y in zip(a, b)) / ((sum(x * x for x in a) ** .5) * (sum(y * y for y in b) ** .5))
        near, far = cos(vecs[0], vecs[1]), cos(vecs[0], vecs[2])
        ok = near > far
    record("embeddings", emb, ok, t, dims=len(vecs[0]) if vecs and vecs[0] else 0,
           similar=round(near, 3) if ok else None, different=round(far, 3) if ok else None)

# speech: Kokoro (ONNX) and Qwen3-TTS (llama.cpp gen-audio), then STT of each, plain and streaming
stt = find("moonshine") or find("whisper")
for needle, voice in (("kokoro", "af_heart"), ("qwen3-tts", None)):
    tts = find(needle)
    if not tts:
        record("tts present", needle, False, 0); continue
    body = {"model": tts, "input": "Rivers carry water from the mountains to the sea."}
    if voice:
        body["voice"] = voice
    s, d, t = call("/v1/audio/speech", body)
    uri = ((d.get("output") or {}).get("audio_data") or "") if isinstance(d, dict) else ""
    wav = base64.b64decode(uri.split(",", 1)[1]) if "," in uri else b""
    (OUT / f"tts_{needle}.wav").write_bytes(wav)
    record("tts", tts, s == 200 and len(wav) > 20000, t, wav_bytes=len(wav), error=d.get("error") if isinstance(d, dict) else None)
    if wav and stt:
        src = {"type": "base64", "data": uri}
        s, d, t = call("/v1/audio/transcriptions", {"model": stt, "input_source": src})
        text = ((d.get("output") or {}).get("text") or "") if isinstance(d, dict) else ""
        record("stt", f"{stt} <- {needle}", s == 200 and "water" in text.lower(), t, text=text[:160])
        s, ev, first, total = stream("/v1/audio/transcriptions", {"model": stt, "input_source": src})
        chunks = [e for e in ev if e.get("status") == "chunk"]
        done = next((e for e in ev if e.get("status") == "done"), {})
        final = json.dumps(done)[:200]
        record("stt streaming", f"{stt} <- {needle}", s == 200 and chunks and "water" in final.lower(), total,
               chunks=len(chunks), first_event_s=round(first, 2) if first else None, final=final)

(OUT / "results.json").write_text(json.dumps(rows, indent=1, ensure_ascii=False))
print(f"== features done: {sum(r['ok'] for r in rows)} of {len(rows)} passed", flush=True)
