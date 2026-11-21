"""Run 38: recheck the two Cluaiz problems run 26 reported, on the current code.

1. Embeddings with 2 requests at once: run 26 got HTTP 429 `embedding_busy` for 8,160 of 8,163.
2. Streamed chat token counts: run 26's harness found no usage chunk from Cluaiz, so it counted SSE chunks
   as tokens (10 per answer against llama.cpp's 15 tokens), which made Cluaiz's tokens per second look low.

Usage: python probe.py <host> <token_file> <chat_model> <embed_model>
"""

import json
import sys
import threading
import time
import urllib.error
import urllib.request

HOST, TOKEN_FILE, CHAT, EMBED = sys.argv[1:5]
TOKEN = open(TOKEN_FILE, encoding="utf-8").read().strip()


def post(path, body, stream=False):
    req = urllib.request.Request(HOST + path, json.dumps(body).encode(),
                                 {"Content-Type": "application/json", "Authorization": f"Bearer {TOKEN}"})
    return urllib.request.urlopen(req, timeout=300)


def embed_once(i, out):
    t0 = time.monotonic()
    try:
        with post("/v1/embeddings", {"model": EMBED, "input": [f"ticket {i}: the invoice page shows an error"] * 8}) as r:
            d = json.loads(r.read())
            out.append(("ok", r.status, len(d["data"]), round(time.monotonic() - t0, 2)))
    except urllib.error.HTTPError as e:
        out.append(("error", e.code, e.read().decode()[:160], round(time.monotonic() - t0, 2)))


def embeddings_parallel(n):
    out = []
    threads = [threading.Thread(target=embed_once, args=(i, out)) for i in range(n)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    return out


def chat_stream_counts():
    """The request run 26's harness sent, with its JSON label task."""
    body = {"model": CHAT, "stream": True, "stream_options": {"include_usage": True}, "temperature": 0,
            "max_tokens": 64, "think_mode": "off", "reasoning_effort": 0, "temporary_chat": "strict",
            "response_format": {"type": "json_object"},
            "messages": [{"role": "system", "content": "Label the support ticket. Reply with JSON {\"label\": ...}."},
                         {"role": "user", "content": "I was charged twice for my plan this month."}]}
    chunks, usage, text = 0, None, []
    with post("/v1/chat/completions", body) as r:
        for raw in r:
            line = raw.decode().strip()
            if not line.startswith("data:") or line[5:].strip() == "[DONE]":
                continue
            d = json.loads(line[5:])
            if isinstance(d.get("usage"), dict):
                usage = d["usage"]
            for c in d.get("choices") or []:
                piece = (c.get("delta") or {}).get("content") or ""
                if piece:
                    chunks += 1
                    text.append(piece)
    return {"content_chunks": chunks, "usage": usage, "text": "".join(text)}


print("### 1 embeddings, 1 request")
print(embeddings_parallel(1))
print("\n### 2 embeddings, 2 requests at once")
print(embeddings_parallel(2))
print("\n### 3 embeddings, 4 requests at once")
print(embeddings_parallel(4))
print("\n### 4 streamed chat: content chunks against the usage chunk")
for _ in range(3):
    print(chat_stream_counts())
