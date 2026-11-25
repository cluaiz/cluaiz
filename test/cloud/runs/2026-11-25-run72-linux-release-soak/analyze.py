"""Run 72: tables from the soak's raw files (soak.py JSONL, bg.jsonl, sampler.jsonl).

    python3 analyze.py <soak out dir>  -> prints Markdown, also written to <dir>/analysis.md

Latency: soak.py writes requests and responses as separate lines; a response is paired with the oldest
open request (FIFO; requests run one at a time except the `concurrent` scenario, whose requests are identical).
Request kind comes from the request body. bg.jsonl rows carry their own elapsed time. A bg failure is
"restart window" when it ended within 30 s of an engine_stopped / engine_started event, or spans one.
"""
import json
import sys
from collections import Counter, defaultdict
from pathlib import Path

d = Path(sys.argv[1])


def rows(name):
    p = d / name
    return [json.loads(l) for l in p.open(encoding="utf-8") if l.strip()] if p.exists() else []


def pct(values, q):
    v = sorted(values)
    if not v:
        return None
    k = (len(v) - 1) * q
    lo, hi = int(k), min(int(k) + 1, len(v) - 1)
    return round(v[lo] + (v[hi] - v[lo]) * (k - lo), 1)


def kind(body):
    if not isinstance(body, dict):
        return "bad_json"
    msgs = body.get("messages") or []
    text = json.dumps(msgs)
    if body.get("model") == "missing-model-release-soak":
        return "unknown_model"
    if not msgs:
        return "empty"
    if body.get("n") == 2:
        return "n"
    if body.get("logprobs"):
        return "logprobs"
    if "The marker is" in text:
        return "long_input"
    if body.get("max_completion_tokens"):
        return "max_tokens"
    if body.get("think_mode") == "on":
        return "think_on"
    if body.get("stream"):
        return "stream"
    if "Reply with the number" in text:
        return "multi"
    return "single_or_concurrent"


out = []
checks = rows("checks.jsonl")
events = rows("events.jsonl")
by = defaultdict(lambda: [0, 0])
for c in checks:
    by[c["scenario"]][0 if c["pass"] else 1] += 1
out.append("## soak.py scenario checks\n\n| Scenario | Pass | Fail |\n|---|---:|---:|")
for k in sorted(by):
    out.append(f"| {k} | {by[k][0]} | {by[k][1]} |")
tp, tf = sum(v[0] for v in by.values()), sum(v[1] for v in by.values())
out.append(f"| **total** | **{tp}** | **{tf}** |")
starts = [e for e in events if e.get("event") == "engine_started"]
fin = [e for e in events if e.get("event") == "finished"]
out.append(f"\nEngine starts: {len(starts)}; finished event: {fin[-1] if fin else 'none'}")

reqs = [r for r in rows("requests.jsonl") if not r.get("cancel_after_first_line")]
resps = [r for r in rows("responses.jsonl") if "first_line" not in r]
seq = sorted([("q", r["time"], r) for r in reqs] + [("a", r["time"], r) for r in resps],
             key=lambda x: (x[1], x[0] == "a"))
pending, lat, errs, li = [], defaultdict(list), Counter(), []
for typ, _, r in seq:
    if typ == "q":
        pending.append(kind(r.get("body")))
        continue
    if not pending:
        continue
    k = pending.pop(0)
    if "elapsed_ms" in r:
        lat[k].append(r["elapsed_ms"])
    if "error" in r:
        errs[(k, r["error"][:80])] += 1
    if k == "long_input":
        rep = r.get("reply") if isinstance(r.get("reply"), dict) else {}
        msg = ((rep.get("choices") or [{}])[0].get("message") or {}) if rep else {}
        content = msg.get("content") or ""
        li.append({"status": r.get("status"), "ms": r.get("elapsed_ms"), "error": r.get("error"),
                   "content": content[:100], "marker": "4917" in content})
out.append("\n## soak.py request latency (ms)\n\n| Kind | n | p50 | p95 | max |\n|---|---:|---:|---:|---:|")
for k in sorted(lat):
    out.append(f"| {k} | {len(lat[k])} | {pct(lat[k], .5)} | {pct(lat[k], .95)} | {max(lat[k])} |")
if errs:
    out.append("\nClient-side errors: " + "; ".join(f"{k[0]}: {k[1]} x{v}" for k, v in errs.items()))

out.append("\n## long_input replies\n\n| # | status | ms | marker in content | content (first 100 chars) / error |\n|---|---|---:|---|---|")
for i, x in enumerate(li, 1):
    shown = (x["content"] or x["error"] or "").replace("|", "/").replace("\n", " ")
    out.append(f"| {i} | {x['status']} | {x['ms']} | {x['marker']} | {shown} |")

bg = rows("bg.jsonl")
marks = [e["time"] for e in events if e.get("event") in ("engine_stopped", "engine_started")]


def in_restart(b):
    return any(abs(b["end"] - m) < 30 or b["start"] <= m <= b["end"] for m in marks)


bgk = defaultdict(lambda: {"pass": 0, "fail_restart": 0, "fail_other": 0, "ms": [], "expected": 0})
for b in bg:
    s = bgk[b["kind"]]
    if b.get("pass"):
        s["pass"] += 1
        s["ms"].append(b["elapsed_ms"])
        s["expected"] += int(bool(b.get("has_expected")))
    elif in_restart(b):
        s["fail_restart"] += 1
    else:
        s["fail_other"] += 1
out.append("\n## extra users (bg_users.py)\n\n| Kind | pass | fail in restart window | fail other | expected answer in passes | p50 ms | p95 ms |\n|---|---:|---:|---:|---:|---:|---:|")
for k in sorted(bgk):
    s = bgk[k]
    out.append(f"| {k} | {s['pass']} | {s['fail_restart']} | {s['fail_other']} | {s['expected']} | {pct(s['ms'], .5)} | {pct(s['ms'], .95)} |")
other = [b for b in bg if not b.get("pass") and not in_restart(b)]
if other:
    out.append("\nbg failures outside restart windows (first 5): " + "; ".join(
        f"{b['kind']} {b.get('status')} {str(b.get('error', b.get('answer', '')))[:100]}" for b in other[:5]))

sm = [s for s in rows("sampler.jsonl") if s.get("rss_kib")]
if sm:
    t0 = sm[0]["time"]
    out.append("\n## memory over time (sampler.jsonl, first sample of each 10 minutes)\n\n| min | pid | RSS MiB | GPU proc MiB | GPU used,free,util |\n|---:|---|---:|---:|---|")
    nxt = 0
    for s in sm:
        m = (s["time"] - t0) / 60
        if m >= nxt:
            out.append(f"| {m:.0f} | {s['pid']} | {s['rss_kib'] / 1024:.0f} | {s['gpu_proc_mib']} | {s['gpu_used_free_util']} |")
            nxt += 10
    perpid = defaultdict(list)
    for s in sm:
        perpid[s["pid"]].append(s)
    out.append("\n### per server process\n\n| pid | samples | minutes alive | RSS first MiB | RSS last MiB | RSS max MiB | GPU first | GPU last | GPU max |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for pid, ss in perpid.items():
        g = [x["gpu_proc_mib"] for x in ss if x["gpu_proc_mib"] is not None]
        out.append(f"| {pid} | {len(ss)} | {(ss[-1]['time'] - ss[0]['time']) / 60:.1f} | {ss[0]['rss_kib'] / 1024:.0f} | "
                   f"{ss[-1]['rss_kib'] / 1024:.0f} | {max(x['rss_kib'] for x in ss) / 1024:.0f} | "
                   f"{g[0] if g else None} | {g[-1] if g else None} | {max(g) if g else None} |")

text = "\n".join(out) + "\n"
(d / "analysis.md").write_text(text, encoding="utf-8")
print(text)
