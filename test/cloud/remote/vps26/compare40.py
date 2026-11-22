"""Tables for a run40.sh result folder: W1 per engine and round, W4/W5 per level, website users per block.
Usage: python compare40.py <results-run40 folder>   (prints Markdown)"""
import csv, glob, gzip, json, os, statistics, sys
from datetime import datetime, timezone

root = sys.argv[1]


def opener(path):
    """A file or its .gz copy (large result files are gzipped in the repo)"""
    if not os.path.exists(path) and os.path.exists(path + ".gz"):
        return gzip.open(path + ".gz", "rt", encoding="utf-8")
    return open(path, encoding="utf-8")


def rows(path):
    with opener(path) as f:
        return [json.loads(l) for l in f if l.strip().startswith("{")]


def med(v):
    v = [x for x in v if x is not None]
    return round(statistics.median(v), 2) if v else None


def ts(s):
    s = s.replace("Z", "")
    return datetime.fromisoformat(s[:19]).replace(tzinfo=timezone.utc).timestamp()


def resources(folder):
    """(start, end, engine anonymous memory peak MB, machine memory peak MB) per phase name."""
    out = {}
    p = os.path.join(folder, "resources.csv")
    if not os.path.exists(p) and not os.path.exists(p + ".gz"):
        return out
    for r in csv.DictReader(opener(p)):
        ph = r["phase"]
        t = ts(r["ts"])
        a, b, e, m = out.get(ph, (t, t, 0.0, 0.0))
        out[ph] = (min(a, t), max(b, t), max(e, float(r["engine_anon_mb"] or 0)), max(m, float(r["mem_used_mb"] or 0)))
    return out


web = []
wp = os.path.join(root, "web_users.csv")
if os.path.exists(wp) or os.path.exists(wp + ".gz"):
    for r in csv.DictReader(opener(wp)):
        web.append((ts(r["ts"]), int(r["status"]), int(r["ms"])))


def web_in(a, b):
    sel = [w for w in web if a <= w[0] <= b]
    if not sel:
        return "-"
    fails = sum(1 for w in sel if w[1] == 0 or w[1] >= 500)
    ms = sorted(w[2] for w in sel)
    return f"{len(sel) - fails}/{len(sel)} ok, p95 {ms[int(0.95 * (len(ms) - 1))]} ms"


print(f"## {os.path.basename(os.path.abspath(root))}\n")
man = glob.glob(os.path.join(root, "*", "manifest.json"))
if man:
    m = json.load(open(man[0], encoding="utf-8"))
    mc, pr, en = m.get("machine", {}), m.get("params", {}), m.get("engines", {})
    print(f"Machine: {mc.get('cpu_model')}, {mc.get('vcpus')} vCPU, {mc.get('mem_total_mb')} MB RAM; "
          f"threads {pr.get('threads')}; Cluaiz commit {en.get('cluaiz_commit')}\n")

print("### W1 chat (ticket label as JSON)\n")
print("| Round | Engine | Model | At once | OK | Seconds per request p50 | First token p50 | Gen t/s p50 | Out tokens p50 | Right label | Engine memory peak MB | Website during step |")
print("|---|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|")
for folder in sorted(glob.glob(os.path.join(root, "r*_w1"))):
    rnd = os.path.basename(folder).split("_")[0]
    res = resources(folder)
    for sp in sorted(glob.glob(os.path.join(folder, "steps_*.jsonl"))):
        reqs = rows(sp.replace("steps_", "requests_"))
        for s in rows(sp):
            if s.get("phase_kind") != "ramp":
                continue
            rq = [r for r in reqs if r.get("model") == s["model"] and r.get("level") == s["level"] and r.get("phase_kind") == "ramp"]
            ph = [k for k in res if s["model"] in k and "W1" in k]
            peak = max((res[k][2] for k in ph), default=0)
            end = ts(s["ts"])
            print(f"| {rnd} | {s['engine']} | {s['model']} | {s['level']} | {s['ok']}/{s['total']} | {s['p50_s']} | {s['ttft_p50_s']} | "
                  f"{med([r.get('gen_tps') for r in rq])} | {med([r.get('out_tokens') for r in rq])} | {s.get('label_accuracy')} | "
                  f"{round(peak)} | {web_in(end - s['wall_s'], end)} |")

for folder in sorted(glob.glob(os.path.join(root, "cluaiz_w4_w5"))):
    print("\n### W4 embeddings and W5 speech (Cluaiz)\n")
    print("| Workload | Part | At once | OK | HTTP 429 | p50 s | p95 s |")
    print("|---|---|---:|---|---:|---:|---:|")
    for sp in sorted(glob.glob(os.path.join(folder, "steps_*.jsonl"))):
        reqs = rows(sp.replace("steps_", "requests_"))
        for s in rows(sp):
            part = s.get("phase_kind")
            rq = [r for r in reqs if r.get("level") == s.get("level") and r.get("workload") == s.get("workload")]
            n429 = sum(1 for r in rq if r.get("http_status") == 429)
            print(f"| {s.get('workload')} | {part} | {s.get('level')} | {s.get('ok')}/{s.get('total')} | {n429} | {s.get('p50_s')} | {s.get('p95_s')} |")

if web:
    fails = sum(1 for w in web if w[1] == 0 or w[1] >= 500)
    ms = sorted(w[2] for w in web)
    print(f"\nWebsite, whole run: {len(web) - fails}/{len(web)} requests OK, p50 {ms[len(ms) // 2]} ms, "
          f"p95 {ms[int(0.95 * (len(ms) - 1))]} ms, max {ms[-1]} ms")

cs = os.path.join(root, "cpu_stat.log")
if os.path.exists(cs):
    lines = [l.split() for l in open(cs) if l.strip()]
    if len(lines) > 1:
        print("\n### CPU steal between blocks\n")
        for a, b in zip(lines, lines[1:]):
            va, vb = [int(x) for x in a[3:13]], [int(x) for x in b[3:13]]
            tot = sum(vb) - sum(va)
            print(f"- {a[1]}: steal {100 * (vb[7] - va[7]) / tot:.1f}%, busy {100 * (tot - (vb[3] - va[3]) - (vb[4] - va[4])) / tot:.1f}%")
