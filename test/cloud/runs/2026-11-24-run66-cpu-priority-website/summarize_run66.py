"""Run 66: one table per arm from the raw files run66.sh writes.

Usage: python3 summarize_run66.py <results dir>   (prints a Markdown table)
Website: the sampler's once-a-second check of neighbour_app.py (GET /health: one indexed read and one
write in PostgreSQL), only in the arm's soak phase. "Over 5 s" counts checks that took 5 s or more or did
not return HTTP 200 (the checker gives up at 5 s).
Model: completion tokens from the usage chunk of each streamed answer, summed and divided by the soak's
wall time; seconds per request = wall time / answered requests (one slot, so this is the service time,
not the wait in the queue). Steal: this CPU's steal ticks over all its ticks between arm start and end.
"""
import csv
import gzip
import json
import re
import statistics
import sys
from pathlib import Path


def pct(vals, p):
    if not vals:
        return None
    s = sorted(vals)
    k = (len(s) - 1) * p / 100.0
    lo, hi = int(k), min(int(k) + 1, len(s) - 1)
    return s[lo] + (s[hi] - s[lo]) * (k - lo)


def cpu_lines(path, cpu):
    """{label: (total ticks, steal ticks)} for this CPU from cpu_stat.log."""
    out, label = {}, None
    for line in Path(path).read_text().splitlines():
        m = re.match(r"^\S+Z (\S+)$", line)
        if m:
            label = m.group(1)
            continue
        if label and line.startswith(f"cpu{cpu} "):
            v = list(map(int, line.split()[1:]))
            out[label] = (sum(v[:8]), v[7])
    return out


def website(rows):
    ms = [float(r["neighbour_ms"]) for r in rows if r.get("neighbour_ms")]
    bad = sum(1 for r in rows if r.get("neighbour_http") and (int(float(r["neighbour_http"])) != 200 or float(r["neighbour_ms"]) >= 5000))
    return ms, bad


def main():
    root = Path(sys.argv[1])
    cpu = sys.argv[2] if len(sys.argv) > 2 else "2"
    cl = cpu_lines(root / "cpu_stat.log", cpu)
    print("| Arm | Engine | Priority | Website checks | Median ms | p95 ms | Max ms | Over 5 s | Answers OK | Tokens/s (usage) | s per request | Median gen t/s (engine log) | Usage chunk | Machine memory peak MB | Engine anon peak MB | Engine CPU avg % | Steal % |")
    print("|---|---|---|---:|---:|---:|---:|---:|---|---:|---:|---:|---|---:|---:|---:|---:|")
    b = root / "baseline" / "resources.csv"
    if b.exists():
        rows = list(csv.DictReader(b.open()))
        ms, bad = website(rows)
        st = ""
        if "baseline_start" in cl and "baseline_end" in cl:
            dt = cl["baseline_end"][0] - cl["baseline_start"][0]
            st = f"{100.0 * (cl['baseline_end'][1] - cl['baseline_start'][1]) / max(1, dt):.2f}"
        print(f"| website alone | - | - | {len(ms)} | {statistics.median(ms):.1f} | {pct(ms, 95):.1f} | {max(ms):.1f} | {bad} | - | - | - | - | - | {max(float(r['mem_used_mb']) for r in rows):.0f} | - | - | {st} |")
    for d in sorted(p for p in root.iterdir() if p.is_dir() and p.name != "baseline" and p.name != "inputs"
                    and (p / "resources.csv").exists()):
        name, eng, prio = d.name.split("_", 2)
        rows = [r for r in csv.DictReader((d / "resources.csv").open()) if r["phase"].endswith("_soak_c32")]
        if not rows:
            continue
        ms, bad = website(rows)
        step = json.loads((d / f"steps_native_{eng}.jsonl").read_text().splitlines()[-1])
        reqs = [json.loads(l) for l in (d / f"requests_native_{eng}.jsonl").read_text().splitlines() if l.strip()]
        reqs = [r for r in reqs if r.get("phase_kind") == "soak"]
        ok = [r for r in reqs if r.get("ok")]
        with_usage = sum(1 for r in ok if r.get("in_tokens") is not None)
        toks = sum(int(r["out_tokens"] or 0) for r in ok)
        wall = float(step["wall_s"])
        log = d / f"engine_native_{eng}.log"
        gz = d / f"engine_native_{eng}.log.gz"
        gen = []
        text = log.read_text(errors="replace") if log.exists() else (
            gzip.open(gz, "rt", errors="replace").read() if gz.exists() else "")
        if text:
            for line in text.splitlines():
                # Cluaiz: "llama.cpp timings: prompt ... | generation 7 tokens in 1042 ms (6.72 t/s)"
                m = re.search(r"generation \d+ tokens in [\d.]+ ms \(([\d.]+) t/s\)", line)
                # llama-server: "       eval time = ... tokens per second)" (the line after "prompt eval time")
                if not m and "prompt eval" not in line:
                    m = re.search(r"\beval time =.*?([\d.]+) tokens per second", line)
                if m:
                    gen.append(float(m.group(1)))
        st = ""
        if f"{name}_start" in cl and f"{name}_end" in cl:
            a, z = cl[f"{name}_start"], cl[f"{name}_end"]
            st = f"{100.0 * (z[1] - a[1]) / max(1, z[0] - a[0]):.2f}"
        mem = max(float(r["mem_used_mb"]) for r in rows)
        anon = max(float(r["engine_anon_mb"]) for r in rows)
        ecpu = statistics.mean(float(r["engine_cpu_pct"]) for r in rows)
        gen_s = f"{statistics.median(gen):.2f} (n={len(gen)})" if gen else "?"
        print(f"| {name} | {eng} | {prio} | {len(ms)} | {statistics.median(ms):.1f} | {pct(ms, 95):.1f} | {max(ms):.1f} | {bad} | {len(ok)} / {len(reqs)} | {toks / wall:.2f} | {wall / max(1, len(ok)):.2f} | {gen_s} | {with_usage} / {len(ok)} | {mem:.0f} | {anon:.0f} | {ecpu:.1f} | {st} |")


if __name__ == "__main__":
    main()
