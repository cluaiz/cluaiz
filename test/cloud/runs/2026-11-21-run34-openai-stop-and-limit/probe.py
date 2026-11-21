"""Run 34: `stop` and `max_completion_tokens` on an installed model after the fix (compare run 32).

Usage: python probe.py <base_url> <token_file> <model>
Prints each request's status, finish_reason, completion tokens and answer (cut to 300 characters).
"""

import json
import sys
import urllib.error
import urllib.request

BASE, TOKEN_FILE, MODEL = sys.argv[1], sys.argv[2], sys.argv[3]
TOKEN = open(TOKEN_FILE, encoding="utf-8").read().strip()
COUNT = [{"role": "user", "content": "Count from 1 to 10, one number per line. Only the numbers."}]


def post(body):
    req = urllib.request.Request(BASE + "/v1/chat/completions", data=json.dumps(body).encode(), method="POST",
                                 headers={"Content-Type": "application/json", "Authorization": f"Bearer {TOKEN}"})
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            return r.status, r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode("utf-8", "replace")


def show(label, body):
    status, text = post(body)
    print(f"### {label}\nHTTP {status}")
    if body.get("stream"):
        content, finish = "", None
        for line in text.splitlines():
            if line.startswith("data: ") and line[6:].strip() not in ("", "[DONE]"):
                chunk = json.loads(line[6:])
                for choice in chunk.get("choices", []):
                    content += (choice.get("delta") or {}).get("content") or ""
                    finish = choice.get("finish_reason") or finish
        print(f"finish_reason: {finish}\nanswer: {content[:300]!r}\n")
        return
    try:
        data = json.loads(text)
        choice = data["choices"][0]
        print(f"finish_reason: {choice['finish_reason']}, completion_tokens: {data['usage']['completion_tokens']}")
        print(f"answer: {choice['message']['content'][:300]!r}\n")
    except Exception:
        print(text[:300] + "\n")


base = {"model": MODEL, "think_mode": "off", "temperature": 0}
show("1 stop as a string", {**base, "messages": COUNT, "stop": "5"})
show("2 stop as a list", {**base, "messages": COUNT, "stop": ["5"]})
show("3 stop with two strings, the later one first in the text", {**base, "messages": COUNT, "stop": ["7", "3"]})
show("4 stop while streaming", {**base, "messages": COUNT, "stop": ["5"], "stream": True})
show("5 max_completion_tokens 5", {**base, "messages": COUNT, "max_completion_tokens": 5})
show("6 max_tokens wins over max_completion_tokens", {**base, "messages": COUNT, "max_tokens": 8, "max_completion_tokens": 3})
show("7 no stop (control)", {**base, "messages": COUNT})
