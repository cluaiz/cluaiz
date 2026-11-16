"""Run 19 workloads against a running Cluaiz server. One JSON line per result, appended to <out>/results.jsonl.

    python3 workloads.py <base url> <token> <out dir> <what> [args]
      chat <model> [n]            cold then warm requests: time to first token, generation t/s, server-reported usage
      speech <tts> <stt...>       TTS real-time factor for 3 texts, then STT of each file by every STT model
      embed <model>               200 sentences in batches of 20: sentences per second
      concurrency <model> <levels>  e.g. 1,2,4: that many requests at once, max_tokens 48
      provider <provider/model>   5 requests through the provider: latency and token usage
      mixed <seconds> <chat> <embed> <tts> <stt> [provider/model]   everything at once
"""
import base64, json, struct, sys, threading, time, urllib.error, urllib.request
from pathlib import Path

BASE, TOKEN, OUT, WHAT = sys.argv[1], sys.argv[2], Path(sys.argv[3]), sys.argv[4]
ARGS = sys.argv[5:]
H = {"content-type": "application/json", "authorization": "Bearer " + TOKEN}
LOCK = threading.Lock()
TEXTS = ["Rivers carry water from the mountains to the sea.",
         "The city library opens at nine in the morning and closes at six in the evening on weekdays.",
         "Good software is tested on small machines too, because many people run it on a cheap server or an old laptop, and it should still answer quickly and never lose their work."]


def emit(row):
    line = json.dumps(row, ensure_ascii=False)
    with LOCK:
        print(line, flush=True)
        with open(OUT / "results.jsonl", "a") as f:
            f.write(line + "\n")


def post(path, body, timeout=1800):
    req = urllib.request.Request(BASE + path, json.dumps(body).encode(), H)
    t = time.time()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, json.loads(r.read()), time.time() - t
    except urllib.error.HTTPError as e:
        return e.code, {"error": e.read().decode(errors="replace")[:400]}, time.time() - t
    except Exception as e:
        return 0, {"error": repr(e)[:400]}, time.time() - t


def chat_stream(model, prompt, max_tokens=64):
    body = {"model": model, "stream": True, "max_tokens": max_tokens, "temperature": 0, "think_mode": "off",
            "temporary_chat": "strict", "stream_options": {"include_usage": True},
            "messages": [{"role": "user", "content": prompt}]}
    req = urllib.request.Request(BASE + "/v1/chat/completions", json.dumps(body).encode(), H)
    t = time.time(); first = None; pieces = 0; text = ""; usage = None
    try:
        with urllib.request.urlopen(req, timeout=1800) as r:
            for raw in r:
                line = raw.decode(errors="replace").strip()
                if not line.startswith("data:") or line == "data: [DONE]":
                    continue
                d = json.loads(line[5:])
                usage = d.get("usage") or usage
                piece = ((d.get("choices") or [{}])[0].get("delta") or {}).get("content") or ""
                if piece:
                    first = first or time.time() - t; pieces += 1; text += piece
        total = time.time() - t
        out_tokens = (usage or {}).get("completion_tokens") or pieces
        gen = out_tokens / (total - first) if first and total > first else None
        return {"ok": bool(text), "ttft_s": round(first or -1, 2), "total_s": round(total, 2), "out_tokens": out_tokens,
                "in_tokens": (usage or {}).get("prompt_tokens"), "gen_tps": round(gen, 2) if gen else None, "text": text[:80]}
    except Exception as e:
        return {"ok": False, "total_s": round(time.time() - t, 2), "error": repr(e)[:300]}


def wav_seconds(wav):
    if len(wav) < 44 or wav[:4] != b"RIFF":
        return None
    rate = struct.unpack("<I", wav[24:28])[0]; block = struct.unpack("<H", wav[32:34])[0]
    i = 12
    while i + 8 <= len(wav):
        cid, size = wav[i:i + 4], struct.unpack("<I", wav[i + 4:i + 8])[0]
        if cid == b"data":
            return min(size, len(wav) - i - 8) / (rate * block)
        i += 8 + size
    return None


def tts(model, text):
    body = {"model": model, "input": text}
    if "kokoro" in model.lower():
        body["voice"] = "af_heart"
    s, d, t = post("/v1/audio/speech", body)
    uri = ((d.get("output") or {}).get("audio_data") or "") if isinstance(d, dict) else ""
    wav = base64.b64decode(uri.split(",", 1)[1]) if "," in uri else b""
    return s, wav, uri, t, d


def stt(model, uri):
    s, d, t = post("/v1/audio/transcriptions", {"model": model, "input_source": {"type": "base64", "data": uri}})
    text = ((d.get("output") or {}).get("text") or "") if isinstance(d, dict) else ""
    return s, text, t, d


if WHAT == "chat":
    model, n = ARGS[0], int(ARGS[1]) if len(ARGS) > 1 else 3
    for i in range(n):
        r = chat_stream(model, f"Note {i}. Write two sentences about rivers.", 64)
        emit({"test": "chat", "model": model, "request": "cold" if i == 0 else "warm", **r})

elif WHAT == "speech":
    tts_model, stts = ARGS[0], ARGS[1:]
    for i, text in enumerate(TEXTS):
        s, wav, uri, t, d = tts(tts_model, text)
        secs = wav_seconds(wav)
        (OUT / f"tts_{i}.wav").write_bytes(wav)
        emit({"test": "tts", "model": tts_model, "chars": len(text), "ok": s == 200 and bool(secs), "gen_s": round(t, 2),
              "audio_s": round(secs, 2) if secs else None, "rtf": round(t / secs, 3) if secs else None,
              "error": d.get("error") if isinstance(d, dict) else None})
        for m in stts:
            if not secs:
                continue
            s2, out, t2, d2 = stt(m, uri)
            words = set(w.strip(".,").lower() for w in text.split()); got = set(w.strip(".,").lower() for w in out.split())
            emit({"test": "stt", "model": m, "ok": s2 == 200 and bool(out), "secs": round(t2, 2), "audio_s": round(secs, 2),
                  "rtf": round(t2 / secs, 3), "word_recall": round(len(words & got) / len(words), 2), "text": out[:120],
                  "error": d2.get("error") if isinstance(d2, dict) else None})

elif WHAT == "embed":
    model = ARGS[0]
    sentences = [f"Sentence number {i} is about {['rivers', 'cities', 'music', 'code'][i % 4]} and other things." for i in range(200)]
    t = time.time(); ok = 0; dims = 0; errors = []
    for i in range(0, 200, 20):
        s, d, _ = post("/v1/embeddings", {"model": model, "input": sentences[i:i + 20]})
        data = (d.get("data") or []) if isinstance(d, dict) else []
        ok += len(data) if s == 200 else 0
        dims = len(data[0]["embedding"]) if data else dims
        if s != 200:
            errors.append(str(d)[:200])
    total = time.time() - t
    emit({"test": "embeddings", "model": model, "ok": ok == 200, "sentences": ok, "secs": round(total, 2),
          "per_s": round(ok / total, 2), "dims": dims, "errors": errors[:2]})

elif WHAT == "concurrency":
    model, levels = ARGS[0], [int(x) for x in ARGS[1].split(",")]
    for c in levels:
        results = [None] * c
        def one(k):
            results[k] = chat_stream(model, f"Request {k} at level {c}. Name three {['colours', 'animals', 'fruits', 'cities'][k % 4]}.", 48)
        t = time.time(); threads = [threading.Thread(target=one, args=(k,)) for k in range(c)]
        [th.start() for th in threads]; [th.join() for th in threads]
        wall = time.time() - t; good = [r for r in results if r and r.get("ok")]
        tokens = sum(r.get("out_tokens") or 0 for r in good)
        lat = sorted(r["total_s"] for r in good)
        emit({"test": "concurrency", "model": model, "concurrent": c, "ok": len(good), "failed": c - len(good),
              "wall_s": round(wall, 2), "tokens": tokens, "total_tps": round(tokens / wall, 2),
              "latency_min_s": lat[0] if lat else None, "latency_max_s": lat[-1] if lat else None,
              "ttft_max_s": max((r["ttft_s"] for r in good), default=None),
              "errors": [r.get("error") for r in results if r and not r.get("ok")][:2]})

elif WHAT == "provider":
    model = ARGS[0]
    for i in range(5):
        r = chat_stream(model, f"Note {i}. In two sentences, why do rivers flow to the sea?", 128)
        emit({"test": "provider chat", "model": model, **r})

elif WHAT == "mixed":
    seconds, chat_m, embed_m, tts_m, stt_m = int(ARGS[0]), ARGS[1], ARGS[2], ARGS[3], ARGS[4]
    provider = ARGS[5] if len(ARGS) > 5 else None
    stop = time.time() + seconds; stats = {}
    s, wav, uri, _, _ = tts(tts_m, TEXTS[0])
    def worker(name, fn):
        st = stats.setdefault(name, {"done": 0, "failed": 0, "secs": []})
        while time.time() < stop:
            t = time.time()
            try:
                ok = fn()
            except Exception:
                ok = False
            st["done" if ok else "failed"] += 1; st["secs"].append(time.time() - t)
    jobs = {
        "chat A": lambda: chat_stream(chat_m, "Name three rivers.", 32)["ok"],
        "chat B": lambda: chat_stream(chat_m, "Name three mountains.", 32)["ok"],
        "embeddings": lambda: post("/v1/embeddings", {"model": embed_m, "input": TEXTS})[0] == 200,
        "tts": lambda: tts(tts_m, TEXTS[1])[0] == 200,
        "stt": lambda: stt(stt_m, uri)[0] == 200,
    }
    if provider:
        jobs["provider chat"] = lambda: chat_stream(provider, "Name three seas.", 32)["ok"]
    threads = [threading.Thread(target=worker, args=(k, f)) for k, f in jobs.items()]
    [th.start() for th in threads]; [th.join() for th in threads]
    for name, st in stats.items():
        secs = sorted(st["secs"])
        emit({"test": "mixed load", "job": name, "seconds": seconds, "done": st["done"], "failed": st["failed"],
              "avg_s": round(sum(secs) / len(secs), 2) if secs else None, "max_s": round(secs[-1], 2) if secs else None})
