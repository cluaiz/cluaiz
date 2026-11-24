"""Run 64 tables from raw/*.jsonl: correct answers ("red" in the reply) per model x layout (probe requests left
out), hint tokens, prompt tokens, and the tool checks (get_weather call or not, engine timings.prompt_n).
Usage: python summarize_run64.py raw > raw/tables.md
"""
import collections
import glob
import json
import os
import sys

raw = sys.argv[1]
print("| model | layout | red / runs | hint_tokens seen | prompt_tokens seen | timings.prompt_n seen |")
print("|---|---|---|---|---|---|")
for path in sorted(glob.glob(os.path.join(raw, "*.jsonl"))):
    if path.endswith(".tools.jsonl"):
        continue
    model = os.path.basename(path)[:-6]
    agg = collections.defaultdict(lambda: [0, 0, set(), set(), set()])
    for line in open(path, encoding="utf-8"):
        d = json.loads(line)
        if d["probe"]:
            continue
        r = d["reply"]
        msg = ((r.get("choices") or [{}])[0]).get("message") or {}
        a = agg[d["layout"]]
        a[1] += 1
        if d["layout"].startswith("TOOL"):
            a[0] += 1 if msg.get("tool_calls") else 0
        else:
            a[0] += 1 if "red" in (msg.get("content") or "").lower() else 0
        a[2].add(((r.get("context_report") or {}).get("tools") or {}).get("hint_tokens"))
        a[3].add((r.get("usage") or {}).get("prompt_tokens"))
        a[4].add((((r.get("engine") or {}).get("timings")) or {}).get("prompt_n"))
    for layout in sorted(agg):
        ok, n, h, p, pn = agg[layout]
        what = "tool call" if layout.startswith("TOOL") else "red"
        print(f"| {model} | {layout} | {ok} / {n} ({what}) | {sorted(h, key=str)} | {sorted(p, key=str)} | {sorted(pn, key=str)} |")

print()
print("| model | case | get_weather calls / runs | timings.prompt_n seen | usage.prompt_tokens seen |")
print("|---|---|---|---|---|")
for path in sorted(glob.glob(os.path.join(raw, "*.tools.jsonl"))):
    model = os.path.basename(path)[: -len(".tools.jsonl")]
    agg = collections.defaultdict(lambda: [0, 0, set(), set()])
    for line in open(path, encoding="utf-8"):
        d = json.loads(line)
        r = d["reply"]
        msg = ((r.get("choices") or [{}])[0]).get("message") or {}
        a = agg[d["case"]]
        a[1] += 1
        a[0] += 1 if any(c.get("function", {}).get("name") == "get_weather" for c in (msg.get("tool_calls") or [])) else 0
        a[2].add((((r.get("engine") or {}).get("timings")) or {}).get("prompt_n"))
        a[3].add((r.get("usage") or {}).get("prompt_tokens"))
    for case in sorted(agg):
        ok, n, pn, p = agg[case]
        print(f"| {model} | {case} | {ok} / {n} | {sorted(pn, key=str)} | {sorted(p, key=str)} |")
