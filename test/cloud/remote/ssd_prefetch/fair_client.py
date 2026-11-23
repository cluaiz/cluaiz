#!/usr/bin/env python3
"""One server load, many requests: run 23 case G's four prompts, then three long prompts for prompt speed.

Usage: fair_client.py <prompts.json> <port> <kind> [pp_sizes]
  kind: llama (llama-server or bmoe-server: timings in the answer) or cluaiz (cluaiz serve: timings
  come from its log, so this prints the wall time only).
  pp_sizes: comma list of approximate prompt lengths in tokens (default 512,2048,4096; empty = none).

stdout: one JSON line per request (prompt name, server timings when given, client wall seconds, text).
stderr: one table row per request.
Same request body for every server: greedy, thinking off, no prompt cache reuse.
"""
import json
import sys
import time
import urllib.request

prompts_path, port, kind = sys.argv[1], int(sys.argv[2]), sys.argv[3]
pp_sizes = [int(x) for x in (sys.argv[4] if len(sys.argv) > 4 else "512,2048,4096").split(",") if x]
URL = f"http://127.0.0.1:{port}/v1/chat/completions"


def long_text(n_tokens, seed):
    # Distinct from the first word, so no two long prompts share a prefix the server could reuse.
    # About 22 tokens per sentence for the Qwen tokenizer. Run 48 (Qwen3.6-35B-A3B) prompt_n for 512 / 2048 /
    # 4096: 527 / 2165 / 4404 in llama-server and bmoe-server, 525 / 2163 / 4402 in cluaiz serve (its chat
    # template adds 2 tokens fewer). A first try with 13 per sentence gave 880 tokens for 512.
    words = ["river", "stone", "market", "lantern", "harbor", "orchard", "bridge", "valley", "signal", "garden"]
    out = [f"Report {seed}. Read the notes below and reply with one short sentence."]
    i = 0
    while len(out) * 22 < n_tokens:
        out.append(f"Note {seed}-{i}: the {words[i % 10]} near the {words[(i * 3 + seed) % 10]} counted {(i * 7919 + seed) % 10000} items.")
        i += 1
    return " ".join(out)


jobs = [(p["name"], p["text"], p["max_tokens"]) for p in json.load(open(prompts_path))]
jobs += [(f"pp{n}", long_text(n, n), 16) for n in pp_sizes]

for name, text, max_tokens in jobs:
    body = {"messages": [{"role": "user", "content": text}], "max_tokens": max_tokens, "temperature": 0,
            "stream": False, "cache_prompt": False}
    if kind == "cluaiz":
        body.update({"model": "qwen3.6-35b-a3b-mtp-gguf-UD-IQ4_XS", "think_mode": "off", "tool_search": False})
    else:
        body["chat_template_kwargs"] = {"enable_thinking": False}
    req = urllib.request.Request(URL, json.dumps(body).encode(), {"Content-Type": "application/json"})
    t0 = time.time()
    r = json.loads(urllib.request.urlopen(req, timeout=7200).read())
    wall = time.time() - t0
    t = r.get("timings") or {}
    msg = (r.get("choices") or [{}])[0].get("message", {})
    print(json.dumps({"prompt": name, "timings": t, "usage": r.get("usage"), "wall_s": round(wall, 2),
                      "text": msg.get("content", "")}), flush=True)
    if t:
        print(f"| {name} | prompt {t.get('prompt_n')} tok {t.get('prompt_per_second', 0):.2f} t/s | "
              f"gen {t.get('predicted_n')} tok {t.get('predicted_per_second', 0):.2f} t/s | wall {wall:.1f} s |",
              file=sys.stderr, flush=True)
    else:
        print(f"| {name} | wall {wall:.1f} s | usage {r.get('usage')} |", file=sys.stderr, flush=True)
