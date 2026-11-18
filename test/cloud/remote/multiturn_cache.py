"""Multi-turn chat without a session id, past a sliding window (run 21): a later turn must reuse the
KV cache of the earlier ones and still use a fact from the first message.
Usage: multiturn_cache.py <base url> <CLUAIZ_HOME> <model id>"""
import json, os, sys, urllib.request

BASE, HOME, MODEL = sys.argv[1], sys.argv[2], sys.argv[3]
token_file = os.path.join(HOME, "session.token")
TOKEN = open(token_file, encoding="utf-8").read().strip() if os.path.exists(token_file) else ""
filler = " ".join(f"Paragraph {i}: the river town kept its records in a stone hall, and each season the clerks "
                  f"copied the ledgers of grain, salt and timber into new books so the old ones could rest." for i in range(1, 45))
first = "Remember this: the secret code is BLUE-42. Then read the notes below and reply only OK.\n\n" + filler


def chat(messages, label):
    body = json.dumps({"model": MODEL, "messages": messages, "max_tokens": 24, "temperature": 0, "stream": False}).encode()
    headers = {"Content-Type": "application/json"}
    if TOKEN:
        headers["Authorization"] = f"Bearer {TOKEN}"
    out = json.load(urllib.request.urlopen(urllib.request.Request(f"{BASE}/v1/chat/completions", data=body, headers=headers), timeout=1800))
    text = out["choices"][0]["message"]["content"].strip()
    ctx = (out.get("engine") or {}).get("context") or {}
    print(f"[{label}] {text!r} | prompt {ctx.get('n_tokens')} | cache_n {ctx.get('cache_n')} | processed {ctx.get('processed_tokens')} | cause {ctx.get('cause')} | edit_at {ctx.get('edit_at')}", flush=True)
    return text, ctx


msgs = [{"role": "user", "content": first}]
a1, _ = chat(msgs, "turn 1")
msgs += [{"role": "assistant", "content": a1}, {"role": "user", "content": "What is the secret code? Answer with the code only."}]
a2, c2 = chat(msgs, "turn 2")
msgs += [{"role": "assistant", "content": a2}, {"role": "user", "content": "Write the code backwards, letters only."}]
a3, c3 = chat(msgs, "turn 3")
ok = "BLUE-42" in a2.upper().replace(" ", "") and (c2.get("cache_n") or 0) > 1000 and (c3.get("cache_n") or 0) > 1000
print("RESULT:", "PASS" if ok else "FAIL")
