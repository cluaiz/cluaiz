"""Run 72: extra concurrent users during the soak, beside test/release_soak/soak.py, against the same server.

    python3 bg_users.py --home <CLUAIZ_HOME> --port 8013 --model <id> --users 2 --minutes 120 --out raw/soak/bg.jsonl

soak.py runs one scenario at a time (three or more users only in its `concurrent` scenario) and restarts the
server on a schedule. These threads keep sending chats the whole time, so every soak scenario also runs next to
other requests. Each user picks at random (fixed seed per user): a short chat with thinking off, one with thinking
on, or a streamed chat. One JSON line per request: start/end time, kind, status, elapsed, pass, error. A request
that fails while soak.py restarts the server is kept and marked later by analyze.py from soak's events.jsonl.
Standard library only.
"""
import argparse
import http.client
import json
import random
import threading
import time


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--home", required=True)
    p.add_argument("--port", type=int, default=8013)
    p.add_argument("--model", required=True)
    p.add_argument("--users", type=int, default=2)
    p.add_argument("--minutes", type=float, default=120)
    p.add_argument("--seed", type=int, default=72)
    p.add_argument("--out", required=True)
    a = p.parse_args()
    if a.port in (8000, 8011):
        raise SystemExit("isolated port only")
    deadline = time.time() + a.minutes * 60
    lock = threading.Lock()

    def token():
        try:
            return open(f"{a.home}/session.token", encoding="utf-8").read().strip()
        except OSError:
            return ""

    def write(row):
        with lock, open(a.out, "a", encoding="utf-8") as f:
            f.write(json.dumps(row) + "\n")

    def user(index):
        rng = random.Random(a.seed * 100 + index)
        n = 0
        while time.time() < deadline:
            n += 1
            kind = rng.choice(["think_off", "think_on", "stream"])
            q = rng.choice([("What is 6*7? Give the number.", "42"), ("What is the capital of France? One word.", "paris"),
                            ("What is 9+10? Give the number.", "19")])
            body = {"model": a.model, "temperature": 0, "messages": [{"role": "user", "content": q[0]}],
                    "think_mode": "on" if kind == "think_on" else "off", "max_tokens": 1024 if kind == "think_on" else 64}
            if kind == "stream":
                body["stream"] = True
            start = time.time()
            row = {"user": index, "n": n, "kind": kind, "start": start}
            try:
                conn = http.client.HTTPConnection("127.0.0.1", a.port, timeout=600)
                conn.request("POST", "/v1/chat/completions", json.dumps(body),
                             {"Content-Type": "application/json", "Authorization": "Bearer " + token()})
                resp = conn.getresponse()
                raw = resp.read().decode("utf-8", "replace")
                conn.close()
                row["status"] = resp.status
                if kind == "stream":
                    text = "".join((json.loads(l[5:]).get("choices") or [{}])[0].get("delta", {}).get("content") or ""
                                   for l in raw.splitlines() if l.startswith("data: {"))
                    ok = resp.status == 200 and "data: [DONE]" in raw
                else:
                    reply = json.loads(raw)
                    text = reply["choices"][0]["message"].get("content") or "" if resp.status == 200 else raw[:300]
                    ok = resp.status == 200
                row["answer"] = text[-120:]
                row["has_expected"] = q[1] in text.lower()
                row["pass"] = ok
            except Exception as exc:
                row["pass"] = False
                row["error"] = repr(exc)[:300]
                time.sleep(2)
            row["end"] = time.time()
            row["elapsed_ms"] = round((row["end"] - start) * 1000, 1)
            write(row)

    threads = [threading.Thread(target=user, args=(i,)) for i in range(a.users)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()


if __name__ == "__main__":
    main()
