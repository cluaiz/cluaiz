"""Run 33: what an Ollama client gets from the Engine's Ollama-named routes.

Sends Ollama-format requests to an isolated server and prints each status and body (bodies cut
to 600 characters). Usage: python probe.py <base_url> <token_file> <model>
"""

import json
import sys
import urllib.error
import urllib.request

BASE, TOKEN_FILE, MODEL = sys.argv[1], sys.argv[2], sys.argv[3]
TOKEN = open(TOKEN_FILE, encoding="utf-8").read().strip()
MSG = [{"role": "user", "content": "Reply with the single word: hello"}]


def send(label, method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(BASE + path, data=data, method=method,
                                 headers={"Content-Type": "application/json", "Authorization": f"Bearer {TOKEN}"})
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            status, ctype, text = r.status, r.headers.get("Content-Type"), r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        status, ctype, text = e.code, e.headers.get("Content-Type"), e.read().decode("utf-8", "replace")
    print(f"### {label}\n{method} {path} -> HTTP {status}, Content-Type: {ctype}\n{text[:600]}\n")


send("1 list models (Ollama)", "GET", "/api/tags")
send("2 chat, Ollama body, stream false", "POST", "/api/chat",
     {"model": MODEL, "messages": MSG, "stream": False, "think": False, "options": {"temperature": 0, "num_predict": 64}})
send("3 chat, Ollama body without stream (Ollama streams by default)", "POST", "/api/chat",
     {"model": MODEL, "messages": MSG, "think": False})
send("4 chat, Ollama body, stream true", "POST", "/api/chat",
     {"model": MODEL, "messages": MSG, "stream": True, "think": False})
send("5 keep_alive as Ollama's duration string", "POST", "/api/chat",
     {"model": MODEL, "messages": MSG, "stream": False, "keep_alive": "5m"})
send("6 generate (Ollama)", "POST", "/api/generate", {"model": MODEL, "prompt": "hello", "stream": False})
send("7 running models (Ollama)", "GET", "/api/ps")
send("8 show model (Ollama)", "POST", "/api/show", {"model": MODEL})
send("9 embeddings (Ollama)", "POST", "/api/embed", {"model": MODEL, "input": "hello"})
send("10 version (Ollama)", "GET", "/api/version")
