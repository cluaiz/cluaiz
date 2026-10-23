"""Long tool session (LONG1) and equality after compaction (KV3).

LONG1: one chat of many turns that read files with the filesystem tool, on a small context, sent
the way the App sends it (session_id, plain-text history). Every turn must answer without a
context error; the report shows compactions and dropped turns.

KV3: in a short chat with tool steps, the same request with compaction held off and then with a
low compact_at_pct (the chat is compacted and the answer is computed on the cached prefix); the
same compacted prompt again from a clean KV must give the same answer. A control does the same
without any edit (cached prompt vs clean KV), to show the equality llama.cpp gives on its own.

    CLUAIZ_MODEL=<model id> python long_session.py --url http://127.0.0.1:8011 --token <token> \\
        [--turns 24] [--n-ctx 4096] [--file notes.txt]

The workspace must hold the file. Tool approvals are answered "approve" automatically.
"""
import argparse
import json
import os
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

QUESTIONS = [
    "{f} padho",
    "{f} ka pehla word kya hai?",
    "{f} me kitni lines hain?",
    "{f} ka aakhri word?",
    "{f} ki doosri line dikhao",
    "{f} me 'secret' word kahan aata hai?",
]


class Client:
    def __init__(self, url, token, model):
        self.url, self.model = url.rstrip("/"), model
        self.headers = {"Authorization": "Bearer " + token, "Content-Type": "application/json"}

    def call(self, path, body=None, method=None):
        request = urllib.request.Request(self.url + path, data=json.dumps(body).encode() if body is not None else None,
                                         headers=self.headers, method=method)
        try:
            return 200, json.load(urllib.request.urlopen(request, timeout=900))
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read() or b"{}")

    def chat(self, body):
        """Non-streaming chat; tool approvals asked meanwhile are approved"""
        if self.model:
            body["model"] = self.model
        result = {}
        worker = threading.Thread(target=lambda: result.update(zip(("code", "data"), self.call("/v1/chat/completions", body))))
        worker.start()
        seen = set()
        while worker.is_alive():
            _, pending = self.call("/v1/system/permission/pending")
            for item in pending.get("pending", []):
                if item.get("status") == "pending" and item["id"] not in seen:
                    seen.add(item["id"])
                    self.call("/v1/system/permission/approve", {"request_id": item["id"], "feedback": None})
            time.sleep(0.3)
        return result["code"], result["data"]


def long_session(client, args, session):
    history, rows = [], []
    for turn in range(args.turns):
        question = QUESTIONS[turn % len(QUESTIONS)].format(f=args.file)
        body = {"stream": False, "max_tokens": 300, "session_id": session, "llama": {"n_ctx": args.n_ctx},
                "messages": history + [{"role": "user", "content": question}]}
        code, data = client.chat(body)
        if code != 200:
            rows.append({"turn": turn + 1, "error": data.get("error", {}).get("message", str(code))})
            print(f"  turn {turn + 1:2d}: ERROR {rows[-1]['error'][:120]}")
            history += [{"role": "user", "content": question}, {"role": "assistant", "content": "(error)"}]
            continue
        answer = data["choices"][0]["message"]["content"] or ""
        plan = data.get("context_report", {}).get("plan", {})
        context = data.get("engine", {}).get("context", {})
        compaction = plan.get("compaction")
        rows.append({"turn": turn + 1, "used": plan.get("used_tokens"), "available": plan.get("available_tokens"),
                     "compaction": compaction, "dropped": context.get("dropped_messages", 0),
                     "prompt": context.get("prompt_tokens"), "cache_n": context.get("cache_n"),
                     "processed": context.get("processed_tokens"), "cause": context.get("cause")})
        print(f"  turn {turn + 1:2d}: used {plan.get('used_tokens')}/{plan.get('available_tokens')} "
              f"prompt {context.get('prompt_tokens')} cache {context.get('cache_n')} cause {context.get('cause')} "
              f"dropped {context.get('dropped_messages', 0)}"
              + (f" | compacted {compaction['freed_tokens']} tokens" if compaction else "")
              + f" | {answer[:60]!r}")
        history += [{"role": "user", "content": question}, {"role": "assistant", "content": answer}]
    return history, rows


def kv3(client, args):
    """Equality after compaction, with a control: the same prompt cached vs from a clean KV
    (llama.cpp's results can differ slightly with the batch shape, so the control shows what
    equality to expect without any edit)"""
    session = f"kv3-{int(time.time())}"
    hold = {"compact_at_pct": 100}
    history = []
    # A short chat with tool steps and hints, compaction held off
    for question in QUESTIONS[:4]:
        question = question.format(f=args.file)
        body = {"stream": False, "max_tokens": 200, "session_id": session, "llama": {"n_ctx": args.n_ctx},
                "context_policy": hold, "messages": history + [{"role": "user", "content": question}]}
        _, data = client.chat(body)
        history += [{"role": "user", "content": question}, {"role": "assistant", "content": data["choices"][0]["message"]["content"] or ""}]
    question = f"Without using any tool: what was the secret word in {args.file}? Answer in one short sentence."
    base = {"stream": False, "max_tokens": 40, "temperature": 0, "seed": 42, "think_mode": "off", "tool_choice": "none",
            "session_id": session, "llama": {"n_ctx": args.n_ctx}, "messages": history + [{"role": "user", "content": question}]}
    answer = lambda d: d["choices"][0]["message"]["content"]
    ctx = lambda d: d["engine"]["context"]

    def clean(policy):
        # Forget the chat's saved KV and fill the context with something else first
        client.call(f"/v1/chat/memory/{session}", method="DELETE")
        client.chat({"stream": False, "max_tokens": 4, "llama": {"n_ctx": args.n_ctx},
                     "messages": [{"role": "user", "content": "Reply with OK."}]})
        return client.chat({**base, "context_policy": policy})[1]

    # Control: the uncompacted prompt computed on the cache vs from a clean KV
    client.chat({**base, "context_policy": hold})
    _, cached = client.chat({**base, "context_policy": hold})
    control_clean = clean(hold)
    # Compaction: computed on the cached prefix up to the first change vs from a clean KV
    client.chat({**base, "context_policy": hold})
    _, compacted = client.chat({**base, "context_policy": {"compact_at_pct": 1, "compact_to_pct": 0}})
    compacted_clean = clean(hold)
    return {
        "control_equal": answer(cached) == answer(control_clean),
        "control": (answer(cached), answer(control_clean), ctx(cached)["cache_n"], ctx(control_clean)["cache_n"]),
        "compaction": compacted["context_report"]["plan"].get("compaction"),
        "equal": answer(compacted) == answer(compacted_clean) and ctx(compacted)["prompt_tokens"] == ctx(compacted_clean)["prompt_tokens"],
        "compacted": (answer(compacted), answer(compacted_clean), ctx(compacted)["cache_n"], ctx(compacted_clean)["cache_n"],
                      ctx(compacted)["prompt_tokens"], ctx(compacted).get("cause"), ctx(compacted).get("edit_at")),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--url", default="http://127.0.0.1:8011")
    parser.add_argument("--token", default=os.environ.get("CLUAIZ_TOKEN"))
    parser.add_argument("--turns", type=int, default=24)
    parser.add_argument("--n-ctx", type=int, default=4096)
    parser.add_argument("--file", default="notes.txt")
    parser.add_argument("--session", default=f"long-{int(time.time())}")
    args = parser.parse_args()
    token = args.token
    if not token and os.environ.get("CLUAIZ_HOME"):
        token = Path(os.environ["CLUAIZ_HOME"], "session.token").read_text().strip()
    if not token:
        sys.exit("No token: pass --token, or set CLUAIZ_TOKEN or CLUAIZ_HOME")
    client = Client(args.url, token, os.environ.get("CLUAIZ_MODEL"))

    print(f"LONG1: {args.turns} turns, n_ctx {args.n_ctx}, chat {args.session}")
    history, rows = long_session(client, args, args.session)
    errors = [r for r in rows if "error" in r]
    compactions = [r for r in rows if r.get("compaction")]
    dropped = sum(r.get("dropped", 0) for r in rows)
    print(f"LONG1: {len(errors)} errors, {len(compactions)} compactions, {dropped} messages dropped by the engine")

    result = kv3(client, args)
    c = result["control"]
    print(f"KV3 control (no edit): equal {result['control_equal']} | cached (cache {c[2]}) {c[0]!r} | clean (cache {c[3]}) {c[1]!r}")
    k = result["compacted"]
    print(f"KV3 compaction {result['compaction']}: equal {result['equal']} | prompt {k[4]}, cause {k[5]} at {k[6]}, cache {k[2]}"
          f" | incremental {k[0]!r} | clean (cache {k[3]}) {k[1]!r}")
    sys.exit(0 if not errors and result["equal"] else 1)


if __name__ == "__main__":
    main()
