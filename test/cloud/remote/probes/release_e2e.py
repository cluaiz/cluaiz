"""Run 49: release end-to-end HTTP checks against a running isolated Engine.

    python release_e2e.py --home <CLUAIZ_HOME> --port 8013 --out raw/http_checks.jsonl [--only name,name]

The server is started by the caller (so its log and environment are kept). Every check writes one JSON
line: name, pass, detail, and the trimmed request and reply, so the summary is written from the file.
Never point it at the owner's own home or port 8000.
"""
import argparse
import base64
import http.client
import json
import os
import struct
import threading
import sys
import time
import zlib

CHAT = "qwen3.5-0.8b-gguf-Q4_K_M"
EMBED = "embeddinggemma-300m-gguf-Q8_0"
TTS = "kokoro-82m-v1.0-onnx-UINT8"
STT = "moonshine-tiny-onnx-INT8"


class Client:
    def __init__(self, port, token):
        self.port = int(port)
        self.token = token

    def request(self, method, path, body=None, token=True, timeout=600, raw=False):
        conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout)
        headers = {"Content-Type": "application/json"}
        if token:
            headers["Authorization"] = f"Bearer {self.token}"
        conn.request(method, path, json.dumps(body) if body is not None else None, headers)
        response = conn.getresponse()
        data = response.read()
        conn.close()
        if raw:
            return response.status, data, dict(response.getheaders())
        try:
            return response.status, json.loads(data) if data else None
        except ValueError:
            return response.status, data.decode("utf-8", "replace")

    def cep(self, method, timeout=600, **params):
        status, reply = self.request("POST", "/v1/cep", {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}, timeout=timeout)
        return reply

    def chat(self, body, timeout=600):
        body = dict({"model": CHAT, "temperature": 0}, **body)
        return self.request("POST", "/v1/chat/completions", body, timeout=timeout)

    def stream(self, path, body, timeout=600):
        conn = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout)
        conn.request("POST", path, json.dumps(body), {"Content-Type": "application/json", "Authorization": f"Bearer {self.token}"})
        response = conn.getresponse()
        lines = []
        while True:
            line = response.readline()
            if not line:
                break
            line = line.decode("utf-8", "replace").strip()
            if line:
                lines.append(line)
        conn.close()
        return response.status, lines


def trim(value, limit=1500):
    text = json.dumps(value, ensure_ascii=False) if not isinstance(value, str) else value
    return text if len(text) <= limit else text[:limit] + f"... [{len(text)} chars]"


def content_of(reply):
    try:
        return reply["choices"][0]["message"].get("content") or ""
    except Exception:
        return ""


def png_square(size=64, rgb=(255, 0, 0)):
    """A solid colour PNG, written here so the run needs no image file"""
    row = b"\x00" + bytes(rgb) * size
    raw = row * size
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b"")


def token_id(gguf_path, text):
    """The id of a vocabulary entry, read from the GGUF header (for logit_bias)"""
    from gguf import GGUFReader
    reader = GGUFReader(gguf_path)
    field = reader.fields["tokenizer.ggml.tokens"]
    for index, part in enumerate(field.data):
        if bytes(field.parts[part]).decode("utf-8", "replace") == text:
            return index
    return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--home", required=True)
    parser.add_argument("--port", default="8013")
    parser.add_argument("--out", required=True)
    parser.add_argument("--only", default="")
    args = parser.parse_args()
    if args.port == "8000":
        sys.exit("never port 8000")
    token = open(os.path.join(args.home, "session.token"), encoding="utf-8").read().strip()
    c = Client(args.port, token)
    only = {n for n in args.only.split(",") if n}
    out = open(args.out, "a", encoding="utf-8")
    results = []

    def record(name, ok, detail="", request=None, reply=None, started=None):
        entry = {"name": name, "pass": bool(ok), "detail": detail, "secs": round(time.time() - started, 2) if started else None,
                 "request": trim(request) if request is not None else None, "reply": trim(reply) if reply is not None else None,
                 "at": time.strftime("%Y-%m-%dT%H:%M:%S")}
        out.write(json.dumps(entry, ensure_ascii=False) + "\n")
        out.flush()
        results.append(entry)
        print(f"{'PASS' if ok else 'FAIL'} {name:38s} {detail}", flush=True)

    def check(name):
        def wrap(fn):
            if only and name not in only:
                return
            started = time.time()
            try:
                fn(lambda ok, detail="", request=None, reply=None: record(name, ok, detail, request, reply, started))
            except Exception as error:
                record(name, False, f"exception: {type(error).__name__}: {error}", started=started)
        return wrap

    @check("health")
    def _(r):
        status, reply = c.request("GET", "/health", token=False)
        r(status == 200, f"HTTP {status}", reply=reply)

    @check("auth_required")
    def _(r):
        perm = json.load(open(os.path.join(args.home, "config", "permission.json"), encoding="utf-8"))
        required = (perm.get("api_auth") or {}).get("required", False)
        status, reply = c.request("POST", "/v1/chat/completions", {"model": CHAT, "messages": [{"role": "user", "content": "hi"}], "max_tokens": 8, "think_mode": "off"}, token=False)
        bad = c.request("POST", "/v1/chat/completions", {"model": CHAT, "messages": [{"role": "user", "content": "hi"}], "max_tokens": 8}, token=False)
        r(status == (401 if required else 200), f"api_auth.required={required} (default, not changed); HTTP {status} without a token", reply=reply)

    @check("v1_models")
    def _(r):
        status, reply = c.request("GET", "/v1/models")
        ids = [m["id"] for m in reply.get("data", [])] if isinstance(reply, dict) else []
        r(status == 200 and CHAT in ids, f"HTTP {status}, {len(ids)} models, chat model listed: {CHAT in ids}", reply=reply)

    @check("chat_basic")
    def _(r):
        body = {"messages": [{"role": "user", "content": "What is 2+2? Answer with the number only."}], "max_tokens": 256, "think_mode": "off"}
        status, reply = c.chat(body)
        text = content_of(reply)
        r(status == 200 and "4" in text and reply.get("usage", {}).get("completion_tokens", 0) > 0,
          f"HTTP {status}, content {text[:60]!r}, usage {reply.get('usage') if isinstance(reply, dict) else None}", body, reply)

    @check("chat_stream_usage")
    def _(r):
        body = {"model": CHAT, "temperature": 0, "stream": True, "stream_options": {"include_usage": True}, "think_mode": "off",
                "messages": [{"role": "user", "content": "Say hello in one word."}], "max_tokens": 64}
        status, lines = c.stream("/v1/chat/completions", body)
        data = [l[5:].strip() for l in lines if l.startswith("data:")]
        chunks = [json.loads(d) for d in data if d != "[DONE]"]
        text = "".join((ch.get("choices") or [{}])[0].get("delta", {}).get("content") or "" for ch in chunks if ch.get("choices"))
        usage = [ch["usage"] for ch in chunks if ch.get("usage")]
        r(status == 200 and data and data[-1] == "[DONE]" and usage and text.strip(),
          f"HTTP {status}, {len(chunks)} chunks, text {text[:40]!r}, usage chunk {bool(usage)}, ends [DONE] {bool(data) and data[-1] == '[DONE]'}", body, lines[-4:])

    @check("chat_stop_string")
    def _(r):
        body = {"messages": [{"role": "user", "content": "Count from 1 to 10 separated by commas, nothing else."}], "stop": "5", "max_tokens": 200, "think_mode": "off"}
        status, reply = c.chat(body)
        text = content_of(reply)
        fin = reply["choices"][0].get("finish_reason") if status == 200 else None
        r(status == 200 and "5" not in text and "4" in text and fin == "stop", f"HTTP {status}, content {text!r}, finish {fin}", body, reply)

    @check("chat_stop_list")
    def _(r):
        body = {"messages": [{"role": "user", "content": "Count from 1 to 10 separated by commas, nothing else."}], "stop": ["7", "8"], "max_tokens": 200, "think_mode": "off"}
        status, reply = c.chat(body)
        text = content_of(reply)
        r(status == 200 and "7" not in text and "6" in text, f"HTTP {status}, content {text!r}", body, reply)

    @check("chat_max_completion_tokens")
    def _(r):
        body = {"messages": [{"role": "user", "content": "Write a long story about a cat."}], "max_completion_tokens": 5, "think_mode": "off"}
        status, reply = c.chat(body)
        usage = reply.get("usage", {}) if isinstance(reply, dict) else {}
        fin = reply["choices"][0].get("finish_reason") if status == 200 else None
        r(status == 200 and 0 < usage.get("completion_tokens", 99) <= 5 and fin == "length", f"HTTP {status}, completion_tokens {usage.get('completion_tokens')}, finish {fin}", body, reply)

    @check("chat_refuses_n2_logprobs")
    def _(r):
        s1, a = c.chat({"messages": [{"role": "user", "content": "hi"}], "n": 2})
        s2, b = c.chat({"messages": [{"role": "user", "content": "hi"}], "logprobs": True})
        r(s1 == 400 and s2 == 400 and "n" in json.dumps(a) and "logprobs" in json.dumps(b), f"n=2 HTTP {s1}, logprobs HTTP {s2}", reply=[a, b])

    @check("chat_logit_bias")
    def _(r):
        path = next(os.path.join(dp, f) for dp, _, fs in os.walk(os.path.join(args.home, "models", "chat")) for f in fs
                    if f.endswith(".gguf") and "mmproj" not in f.lower() and "0.8b" in f.lower())
        yes, no = token_id(path, "Yes"), token_id(path, "No")
        base = {"messages": [{"role": "user", "content": "Is fire cold? Answer Yes or No only."}], "max_tokens": 8, "think_mode": "off"}
        s0, plain = c.chat(base)
        s1, forced = c.chat(dict(base, logit_bias={str(yes): 100}))
        s2, banned = c.chat(dict(base, logit_bias={str(no): -100}))
        s3, bad = c.chat(dict(base, logit_bias={"99999999": 5}))
        t0, t1, t2 = content_of(plain), content_of(forced), content_of(banned)
        r(s0 == 200 and s1 == 200 and s2 == 200 and s3 == 400 and t0.strip().startswith("No") and t1.strip().startswith("Yes") and not t2.strip().startswith("No"),
          f"'Yes'={yes} 'No'={no}; plain {t0!r}; Yes +100 {t1!r}; No -100 {t2!r}; unknown id HTTP {s3}", base, [plain, forced, banned, bad])

    @check("chat_tools")
    def _(r):
        tools = [{"type": "function", "function": {"name": "get_weather", "description": "Current weather of a city",
                  "parameters": {"type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]}}}]
        body = {"messages": [{"role": "user", "content": "What is the weather in Paris right now? Use the tool."}], "tools": tools, "max_tokens": 512}
        status, reply = c.chat(body)
        calls = reply["choices"][0]["message"].get("tool_calls") if status == 200 else None
        name = calls[0]["function"]["name"] if calls else None
        argsj = calls[0]["function"].get("arguments") if calls else None
        r(status == 200 and name == "get_weather" and "paris" in (argsj or "").lower(), f"HTTP {status}, call {name} {argsj}", body, reply)

    @check("chat_approval_mode")
    def _(r):
        base = {"messages": [{"role": "user", "content": "Say ok."}], "max_tokens": 16, "think_mode": "off"}
        s1, a = c.chat(dict(base, approval_mode="strict"))
        s2, b = c.chat(dict(base, approval_mode="bogus"))
        cfg = c.cep("config.get", section="engine")
        mode = (cfg.get("result") or {}).get("engine", (cfg.get("result") or {})).get("agent_security_mode") if isinstance(cfg, dict) else None
        r(s1 == 200 and s2 == 400 and "approval_mode" in json.dumps(b), f"strict HTTP {s1}, bogus HTTP {s2}, global mode after: {mode}", base, [a, b])

    @check("chat_vision")
    def _(r):
        image = {"type": "image_url", "image_url": {"url": "data:image/png;base64," + base64.b64encode(png_square(448)).decode()}}
        text = {"type": "text", "text": "What colour is this image? One word."}
        answers = {}
        for order, parts in (("text_first", [text, image]), ("image_first", [image, text])):
            status, reply = c.chat({"messages": [{"role": "user", "content": parts}], "max_tokens": 64, "think_mode": "off"})
            answers[order] = (status, content_of(reply))
        r(all(s == 200 and "red" in t.lower() for s, t in answers.values()), f"red 448x448 PNG: {answers}", "text+image in both orders", answers)

    @check("chat_multiturn_cache")
    def _(r):
        history = [{"role": "system", "content": "You are a helpful assistant. " * 40}, {"role": "user", "content": "Name a fruit. One word."}]
        s1, a = c.chat({"messages": history, "max_tokens": 16, "think_mode": "off", "session_id": "run49-cache"})
        history += [{"role": "assistant", "content": content_of(a)}, {"role": "user", "content": "Name a colour. One word."}]
        s2, b = c.chat({"messages": history, "max_tokens": 16, "think_mode": "off", "session_id": "run49-cache"})
        u1, u2 = a.get("usage", {}), b.get("usage", {})
        t1, t2 = u1.get("time_to_first_token_ms"), u2.get("time_to_first_token_ms")
        r(s1 == 200 and s2 == 200 and t2 and t1 and t2 < t1 / 2, f"turn 1 prompt {u1.get('prompt_tokens')} TTFT {t1} ms; turn 2 prompt {u2.get('prompt_tokens')} TTFT {t2} ms (the serve log line '[Context] prompt N tokens (M from the cache)' is the proof)", reply=[u1, u2])

    @check("embeddings")
    def _(r):
        body = {"model": EMBED, "input": ["the cat sat", "a dog ran"]}
        status, reply = c.request("POST", "/v1/embeddings", body)
        data = reply.get("data", []) if isinstance(reply, dict) else []
        dims = [len(d["embedding"]) for d in data]
        r(status == 200 and len(dims) == 2 and dims[0] > 100, f"HTTP {status}, dims {dims}", body, {"usage": reply.get("usage") if isinstance(reply, dict) else reply})

    wav_holder = {}

    @check("tts_speech")
    def _(r):
        body = {"model": TTS, "input": "The quick brown fox jumps over the lazy dog.", "voice": "default"}
        status, reply = c.request("POST", "/v1/audio/speech", body)
        uri = ((reply.get("output") or {}).get("audio_data") or "") if isinstance(reply, dict) else ""
        data = base64.b64decode(uri.split(",", 1)[1]) if "," in uri else b""
        wav_holder["wav"] = data
        r(status == 200 and data[:4] == b"RIFF" and len(data) > 20000, f"HTTP {status}, WAV {len(data)} bytes in audio_data (documented JSON reply)", body, {k: v for k, v in reply.items() if k != "output"} if isinstance(reply, dict) else reply)

    @check("stt_transcription")
    def _(r):
        wav = wav_holder.get("wav")
        body = {"model": STT, "input_source": {"type": "audio", "data": "data:audio/wav;base64," + base64.b64encode(wav).decode()}, "language": "en"}
        status, reply = c.request("POST", "/v1/audio/transcriptions", body)
        text = (reply.get("text") or (reply.get("output") or {}).get("text") or "") if isinstance(reply, dict) else ""
        r(status == 200 and "fox" in text.lower(), f"HTTP {status}, text {text!r}", "TTS wav", reply)

    @check("ingest_text")
    def _(r):
        doc = "# Release notes\n\n" + "Cluaiz ingests a Markdown file into text chunks. " * 30
        body = {"source": "data:text/markdown;base64," + base64.b64encode(doc.encode()).decode(), "output_controls": {"return_text": True}}
        status, reply = c.request("POST", "/v1/ingest", body)
        r(status == 200 and "Cluaiz ingests" in json.dumps(reply), f"HTTP {status}", "markdown data URI", reply)

    # Ollama routes
    @check("ollama_version")
    def _(r):
        status, reply = c.request("GET", "/api/version")
        r(status == 200 and isinstance(reply, dict) and reply.get("version"), f"HTTP {status} {reply}", reply=reply)

    @check("ollama_tags")
    def _(r):
        status, reply = c.request("GET", "/api/tags")
        m = [x for x in reply.get("models", []) if x.get("name", "").startswith(CHAT)] if isinstance(reply, dict) else []
        size = m[0].get("details", {}).get("parameter_size") if m else None
        r(status == 200 and m and size and size != "Unknown", f"HTTP {status}, {len(reply.get('models', []))} models, chat parameter_size {size}", reply=reply)

    @check("ollama_show")
    def _(r):
        s1, a = c.request("POST", "/api/show", {"model": CHAT})
        s2, b = c.request("POST", "/api/show", {"model": "no-such-model"})
        r(s1 == 200 and isinstance(a, dict) and a.get("details") and s2 == 404, f"known HTTP {s1} details {a.get('details') if isinstance(a, dict) else None}; unknown HTTP {s2}", reply=[a, b])

    @check("ollama_chat")
    def _(r):
        body = {"model": CHAT, "messages": [{"role": "user", "content": "What is 3+3? Number only."}], "stream": False, "keep_alive": "5m", "think": False, "options": {"temperature": 0}}
        status, reply = c.request("POST", "/api/chat", body)
        text = (reply.get("message") or {}).get("content", "") if isinstance(reply, dict) else ""
        r(status == 200 and "6" in text and reply.get("done") is True, f"HTTP {status}, content {text!r}, done {reply.get('done') if isinstance(reply, dict) else None}", body, reply)

    @check("ollama_chat_stream")
    def _(r):
        body = {"model": CHAT, "messages": [{"role": "user", "content": "Say hi."}], "think": False, "options": {"temperature": 0, "num_predict": 16}}
        status, lines = c.stream("/api/chat", body)
        objs = [json.loads(l) for l in lines]
        r(status == 200 and objs and objs[-1].get("done") is True, f"HTTP {status}, {len(objs)} NDJSON lines, last done {objs[-1].get('done') if objs else None}", body, lines[-2:])

    @check("ollama_generate")
    def _(r):
        body = {"model": CHAT, "prompt": "The capital of France is", "stream": False, "think": False, "options": {"temperature": 0, "num_predict": 16}}
        status, reply = c.request("POST", "/api/generate", body)
        text = reply.get("response", "") if isinstance(reply, dict) else ""
        r(status == 200 and "paris" in text.lower(), f"HTTP {status}, response {text!r}", body, reply)

    @check("ollama_embed")
    def _(r):
        body = {"model": EMBED, "input": "hello world"}
        status, reply = c.request("POST", "/api/embed", body)
        emb = reply.get("embeddings") if isinstance(reply, dict) else None
        r(status == 200 and emb and len(emb[0]) > 100, f"HTTP {status}, {len(emb or [])} x {len(emb[0]) if emb else 0}", body, {"model": reply.get("model") if isinstance(reply, dict) else reply})

    @check("ollama_ps")
    def _(r):
        status, reply = c.request("GET", "/api/ps")
        models = reply.get("models", []) if isinstance(reply, dict) else []
        r(status == 200 and models and models[0].get("context_length"), f"HTTP {status}, {[(m.get('name'), m.get('size_vram'), m.get('context_length')) for m in models]}", reply=reply)

    # CEP
    @check("cep_system_reads")
    def _(r):
        out = {}
        for method in ["system.info", "system.live", "system.ps", "system.plan", "models.list", "models.load_history", "tools.list",
                       "approvals.list", "memory.search", "notes.list", "workspace.get", "kv.status", "prompts.list", "storage.status",
                       "tokens.list", "tasks.list", "providers.list", "context.reports"]:
            params = {"query": "cluaiz", "chat_id": "run49-cache"} if method == "memory.search" else ({"model": CHAT} if method == "system.plan" else ({"chat_id": "run49-cache"} if method == "context.reports" else {}))
            reply = c.cep(method, **params)
            out[method] = "ok" if isinstance(reply, dict) and "result" in reply else (reply.get("error") if isinstance(reply, dict) else reply)
        bad = {k: v for k, v in out.items() if v != "ok"}
        r(not bad, f"{len(out) - len(bad)}/{len(out)} answered; errors: {bad}", reply=out)

    @check("cep_live_port")
    def _(r):
        reply = c.cep("config.get", section="engine")
        res = reply.get("result", {}) if isinstance(reply, dict) else {}
        eng = res.get("engine", res)
        r(eng.get("api_port") == int(args.port), f"engine.api_port {eng.get('api_port')}, saved {eng.get('saved_api_port')}, live {args.port}", reply={"api_port": eng.get("api_port"), "saved_api_port": eng.get("saved_api_port")})

    @check("cep_system_events")
    def _(r):
        first = c.cep("system.events")
        res = first.get("result", {}) if isinstance(first, dict) else {}
        cursor = res.get("latest_seq")
        started = time.time()
        empty = c.cep("system.events", after=cursor, wait_ms=1500)
        waited = time.time() - started
        holder = {}
        def poll():
            holder["reply"] = c.cep("system.events", after=cursor, wait_ms=10000)
            holder["secs"] = time.time() - t0
        t0 = time.time()
        th = threading.Thread(target=poll)
        th.start()
        time.sleep(1)
        task = c.cep("chat.import_app", chat_id="run49-no-such-chat", dry_run=True)["result"]["task"]
        c.cep("tasks.result", task_id=task["task_id"], wait_ms=5000)
        th.join()
        got = (holder.get("reply") or {}).get("result", {}).get("events", [])
        r(isinstance(empty, dict) and "result" in empty and not empty["result"].get("events") and waited >= 1.2 and got and holder.get("secs", 99) < 9,
          f"{len(res.get('events', []))} events, latest_seq {cursor}; empty long poll waited {waited:.1f} s; a poll during activity returned {len(got)} new events after {holder.get('secs', 0):.1f} s: {[e.get('name') for e in got][:5]}", reply=[empty, holder.get("reply")])

    @check("cep_config_roundtrip")
    def _(r):
        before = c.cep("config.get", section="storage")["result"]
        val = (before.get("storage") or before).get("cleanup_policy")
        after = c.cep("config.set", section="storage", patch={"cleanup_policy": val})
        r(isinstance(after, dict) and "result" in after, f"cleanup_policy {val!r} written back unchanged", reply=after)

    total = len(results)
    passed = sum(1 for e in results if e["pass"])
    print(f"== {passed}/{total} passed", flush=True)


if __name__ == "__main__":
    main()
