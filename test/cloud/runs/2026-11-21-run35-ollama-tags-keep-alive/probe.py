"""Run 35: /api/tags dates and keep_alive durations after the fix (compare run 33).

Usage: python probe.py <base_url> <token_file> <model>
"""

import json
import sys
import urllib.error
import urllib.request

BASE, TOKEN_FILE, MODEL = sys.argv[1], sys.argv[2], sys.argv[3]
TOKEN = open(TOKEN_FILE, encoding="utf-8").read().strip()


def send(label, method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(BASE + path, data=data, method=method,
                                 headers={"Content-Type": "application/json", "Authorization": f"Bearer {TOKEN}"})
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            status, text = r.status, r.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        status, text = e.code, e.read().decode("utf-8", "replace")
    print(f"### {label}\n{method} {path} -> HTTP {status}\n{text[:300]}\n")


msg = [{"role": "user", "content": "Say hi."}]
send("1 /api/tags", "GET", "/api/tags")
for value in ["5m", "30s", "1h", "-1", 0, 5]:
    send(f"keep_alive {value!r}", "POST", "/v1/chat/completions",
         {"model": MODEL, "messages": msg, "think_mode": "off", "max_tokens": 8, "keep_alive": value})
send("keep_alive 'soon' (invalid)", "POST", "/v1/chat/completions",
     {"model": MODEL, "messages": msg, "think_mode": "off", "max_tokens": 8, "keep_alive": "soon"})
