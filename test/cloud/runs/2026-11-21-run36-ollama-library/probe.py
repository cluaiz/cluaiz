"""Run 36: the official Ollama Python library against the Engine's Ollama routes (compare run 33).

Usage: python probe.py <host> <token_file> <model>
Prints what each call returned (text cut to 200 characters) or the error it raised.
"""

import json
import sys
import urllib.request

import ollama

HOST, TOKEN_FILE, MODEL = sys.argv[1], sys.argv[2], sys.argv[3]
TOKEN = open(TOKEN_FILE, encoding="utf-8").read().strip()
client = ollama.Client(host=HOST, headers={"Authorization": f"Bearer {TOKEN}"})
print(f"ollama library {getattr(ollama, '__version__', '?')}\n")


def step(label, fn):
    print(f"### {label}")
    try:
        print(fn())
    except Exception as e:  # the library raises on a reply it cannot read
        print(f"ERROR {type(e).__name__}: {e}")
    print()


def chat_once():
    r = client.chat(model=MODEL, messages=[{"role": "user", "content": "Reply with the single word: hello"}],
                    think=False, options={"temperature": 0, "num_predict": 16})
    return f"content={r.message.content[:200]!r} done={r.done} done_reason={r.done_reason} eval_count={r.eval_count} prompt_eval_count={r.prompt_eval_count}"


def chat_stream():
    parts, last = [], None
    for part in client.chat(model=MODEL, messages=[{"role": "user", "content": "Count from 1 to 5, one number per line."}],
                            think=False, stream=True, options={"temperature": 0}):
        parts.append(part.message.content)
        last = part
    return f"chunks={len(parts)} text={''.join(parts)[:200]!r} final done={last.done} done_reason={last.done_reason} eval_count={last.eval_count}"


def chat_thinking():
    r = client.chat(model=MODEL, messages=[{"role": "user", "content": "What is 2+3? Answer with the number."}],
                    think=True, options={"temperature": 0, "num_predict": 400})
    return f"thinking={len(r.message.thinking or '')} chars, content={r.message.content[:80]!r}"


def chat_json():
    r = client.chat(model=MODEL, messages=[{"role": "user", "content": "Give a JSON object with a key city set to Paris."}],
                    think=False, format="json", options={"temperature": 0})
    return f"content={r.message.content[:200]!r} parses={isinstance(json.loads(r.message.content), dict)}"


def generate():
    r = client.generate(model=MODEL, prompt="Reply with the single word: yes", system="Be brief.", think=False,
                        options={"temperature": 0, "num_predict": 16})
    return f"response={r.response[:200]!r} done={r.done}"


def listing():
    r = client.list()
    return [m.model for m in r.models]


def version():
    req = urllib.request.Request(HOST + "/api/version", headers={"Authorization": f"Bearer {TOKEN}"})
    return urllib.request.urlopen(req).read().decode()


step("1 chat, not streaming", chat_once)
step("2 chat, streaming (the library's default for stream=True)", chat_stream)
step("3 chat with think=True", chat_thinking)
step("4 chat with format=json", chat_json)
step("5 generate with a system text", generate)
step("6 list (/api/tags)", listing)
step("7 /api/version", version)
step("8 embed with a chat model (no embedding model installed)", lambda: client.embed(model=MODEL, input="hello"))
